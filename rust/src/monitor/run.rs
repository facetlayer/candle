//! The monitor-mode supervision lifecycle.
//!
//! Uses std threads (no tokio). The flow:
//!
//! 1. open the DB and spawn `sh -c <shell>` (cwd = projectDir[/root]);
//! 2. register a `processes` row (pid = shell, log_collector_pid = self);
//! 3. stream stdout/stderr lines into `process_output`;
//! 4. a 500ms grace period distinguishes a fast failure from a real start;
//! 5. poll the stdin queue (when enabled) and run periodic cleanup;
//! 6. on exit, log `process_exited` and delete the `processes` row.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::db::cleanup::maybe_run_cleanup;
use crate::db::open_database_at;
use crate::db::process_table::{
    create_process_entry, delete_process_entry, find_process_entry, CreateProcessEntry,
};
use crate::db::stdin_messages::{clear_stdin_messages, pop_stdin_message};
use crate::debug::debug_log;
use crate::logs::log_type::STOPPED_WHILE_STARTING_MESSAGE;
use crate::logs::process_logs::save_process_log;
use crate::logs::ProcessLogType;
use crate::monitor::MonitorLaunchInfo;

const GRACE_PERIOD_MS: u64 = 500;
const STDIN_POLL_INTERVAL_MS: u64 = 500;
const CLEANUP_INTERVAL_MS: u128 = 60 * 1000;
/// After the child exits, how long to keep collecting output the reader threads
/// haven't forwarded yet. Normally both pipes close right after the exit and
/// the drain ends at once; the timeout only matters when a background
/// grandchild inherited the pipes and holds them open, possibly forever. Losing
/// a few lines written after the timeout in that case is acceptable, and
/// keeping it short means the exit (and `ps` / `kill`) isn't held up.
const POST_EXIT_DRAIN_MS: u64 = 500;

/// Events forwarded from the reader / wait threads to the supervisor.
enum LineEvent {
    Stdout(String),
    Stderr(String),
    /// Child exited with the given code (`None` if terminated by a signal).
    Exit(Option<i32>),
}

/// Human-readable message for a process exit. A `None` exit code means the
/// process was terminated by a signal (e.g. killed by `candle stop`/`restart`),
/// so don't render it as a bogus "code null".
fn exit_message(code: Option<i32>) -> String {
    match code {
        Some(c) => format!("Process exited with code {c}"),
        None => "Process was stopped".to_string(),
    }
}

/// Persist one grace-period event.
///
/// Returns `Some(code)` when the event was the child exiting, `None` for output
/// lines (which are written to `process_output` as they arrive).
fn record_grace_event(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    event: LineEvent,
) -> Option<Option<i32>> {
    let (log_type, line) = match event {
        LineEvent::Exit(code) => return Some(code),
        LineEvent::Stdout(line) => (ProcessLogType::Stdout, line),
        LineEvent::Stderr(line) => (ProcessLogType::Stderr, line),
    };

    debug_log(&format!("[monitor] {log_type:?}: {line}"));
    let _ = save_process_log(conn, command_name, project_dir, log_type, Some(&line));
    None
}

/// Collect output still in flight after the `Exit` event.
///
/// The reader threads and the wait thread share one channel, so `Exit` can
/// arrive before the last lines the child wrote. Keep receiving until every
/// sender has dropped (both pipes hit EOF) or `timeout` elapses. Returns the
/// number of lines passed to `save`.
fn drain_after_exit(
    rx: &mpsc::Receiver<LineEvent>,
    timeout: Duration,
    mut save: impl FnMut(LineEvent),
) -> usize {
    let deadline = Instant::now() + timeout;
    let mut saved = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            // There is only one wait thread, so no second Exit is coming.
            Ok(LineEvent::Exit(_)) => {}
            Ok(event) => {
                save(event);
                saved += 1;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return saved,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                debug_log("[monitor] output pipes still open after exit; stopped draining");
                return saved;
            }
        }
    }
}

/// Human-readable message for a process that died during the startup grace
/// period. `stopped_by_candle` is set when Candle itself signalled it (see
/// [`stopped_by_candle`]); that is a deliberate stop, not a failed start.
fn start_failed_message(code: Option<i32>, stopped_by_candle: bool) -> String {
    match code {
        Some(c) => format!("Process failed to start: exited with code {c}"),
        None if stopped_by_candle => STOPPED_WHILE_STARTING_MESSAGE.to_string(),
        None => "Process failed to start: stopped by a signal".to_string(),
    }
}

/// Whether Candle stopped this process on purpose. `kill` marks the row
/// `killed_at` before it sends any signal, and cleanup / `erase-database`
/// delete rows outright, so a row that is marked killed or already gone means
/// a deliberate stop. Only this monitor otherwise removes its own row.
fn stopped_by_candle(conn: &Connection, command_name: &str, project_dir: &str, pid: i64) -> bool {
    match find_process_entry(conn, command_name, project_dir, pid) {
        Ok(Some(entry)) => entry.killed_at.is_some(),
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Run the supervision lifecycle to completion, blocking until the child exits
/// (or a startup failure short-circuits). Returns the child's exit code, if any.
pub fn run(launch_info: MonitorLaunchInfo) -> Option<i32> {
    let MonitorLaunchInfo {
        command_name,
        project_dir,
        shell,
        root,
        enable_stdin,
        database_path,
    } = launch_info;

    debug_log(&format!(
        "[monitor] starting shell command {shell:?} enableStdin={enable_stdin}"
    ));

    let conn = match open_database_at(&database_path) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!(
                "Error: failed to open database at {}: {e}",
                database_path.display()
            );
            std::process::exit(1);
        }
    };

    // launchDir = root ? join(projectDir, root) : projectDir. `Path::join`
    // replaces the base when `root` is absolute, so this is the same directory
    // `resolve_launch_dir` reports in the start banner and `list`.
    let launch_dir = match &root {
        Some(r) => Path::new(&project_dir).join(r),
        None => Path::new(&project_dir).to_path_buf(),
    };

    // Spawn the monitored service. A spawn failure maps to the Node
    // `waitForStart` reject path: log process_start_failed, exit, do NOT create
    // (or delete) a process row.
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(&shell)
        .current_dir(&launch_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if enable_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            debug_log(&format!("[monitor] failed to start: {e}"));
            // Both a missing cwd and a missing `sh` surface as ENOENT; say which.
            let reason = if launch_dir.is_dir() {
                format!("could not run 'sh': {e}")
            } else {
                format!("root directory does not exist: {}", launch_dir.display())
            };
            let _ = save_process_log(
                &conn,
                &command_name,
                &project_dir,
                ProcessLogType::ProcessStartFailed,
                Some(&format!("Process failed to start: {reason}")),
            );
            std::process::exit(1);
        }
    };

    let child_pid = child.id() as i64;
    let my_pid = std::process::id() as i64;

    debug_log(&format!("[monitor] launched subprocess, pid={child_pid}"));

    let _ = create_process_entry(
        &conn,
        &CreateProcessEntry {
            command_name: command_name.clone(),
            project_dir: project_dir.clone(),
            pid: child_pid,
            log_collector_pid: Some(my_pid),
            shell: Some(shell.clone()),
            root: root.clone(),
        },
    );

    // Reader + wait threads forward events over a channel.
    let (tx, rx) = mpsc::channel::<LineEvent>();

    let stdout = child.stdout.take().expect("stdout piped");
    let tx_out = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx_out.send(LineEvent::Stdout(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let stderr = child.stderr.take().expect("stderr piped");
    let tx_err = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx_err.send(LineEvent::Stderr(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Stdin polling thread (own DB connection; pop needs &mut). The `done` flag
    // lets us stop it once the child exits (mirrors Node's clearInterval).
    let done = Arc::new(AtomicBool::new(false));
    let stdin_handle = if enable_stdin {
        let mut child_stdin = child.stdin.take();
        let _ = clear_stdin_messages(&conn, &command_name, &project_dir);

        let cmd_name = command_name.clone();
        let proj_dir = project_dir.clone();
        let db_path = database_path.clone();
        let done = Arc::clone(&done);

        Some(thread::spawn(move || {
            let mut poll_conn = match open_database_at(&db_path) {
                Ok(c) => c,
                Err(_) => return,
            };
            while !done.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(STDIN_POLL_INTERVAL_MS));
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let stdin = match child_stdin.as_mut() {
                    Some(s) => s,
                    None => break,
                };
                match pop_stdin_message(&mut poll_conn, &cmd_name, &proj_dir) {
                    Ok(Some(msg)) => {
                        debug_log(&format!("[monitor] writing stdin message: {}", msg.data));
                        if stdin.write_all(msg.data.as_bytes()).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {}
                }
            }
        }))
    } else {
        None
    };

    // Wait thread: forwards the exit code once the child terminates.
    let tx_exit = tx;
    thread::spawn(move || {
        let code = child.wait().ok().and_then(|s| s.code());
        let _ = tx_exit.send(LineEvent::Exit(code));
    });

    // Grace period: collect output until the deadline or an early exit.
    let grace_deadline = Instant::now() + Duration::from_millis(GRACE_PERIOD_MS);
    let mut exited_during_grace = false;
    let mut exit_code: Option<i32> = None;

    loop {
        let remaining = grace_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(event) => {
                if let Some(code) = record_grace_event(&conn, &command_name, &project_dir, event) {
                    exited_during_grace = true;
                    exit_code = code;
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // The deadline can expire with events already queued: a process that dies
    // instantly still emits its error output first, and writing those lines can
    // outlast the window on a loaded machine. Drain what has already arrived
    // before deciding, or a fast failure gets misreported as a successful start.
    while !exited_during_grace {
        let Ok(event) = rx.try_recv() else { break };
        if let Some(code) = record_grace_event(&conn, &command_name, &project_dir, event) {
            exited_during_grace = true;
            exit_code = code;
        }
    }

    if exited_during_grace {
        drain_after_exit(&rx, Duration::from_millis(POST_EXIT_DRAIN_MS), |event| {
            record_grace_event(&conn, &command_name, &project_dir, event);
        });
    }

    // A nonzero exit within the grace period is a start failure: log it, delete
    // the row, and stop. (Asymmetry vs the spawn-failure branch above, which
    // never created a row.)
    if exited_during_grace && exit_code != Some(0) {
        debug_log(&format!(
            "[monitor] process failed during grace period, pid={child_pid}, code={exit_code:?}"
        ));
        let _ = save_process_log(
            &conn,
            &command_name,
            &project_dir,
            ProcessLogType::ProcessStartFailed,
            Some(&start_failed_message(
                exit_code,
                exit_code.is_none()
                    && stopped_by_candle(&conn, &command_name, &project_dir, child_pid),
            )),
        );
        let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);
        done.store(true, Ordering::Relaxed);
        if let Some(handle) = stdin_handle {
            let _ = handle.join();
        }
        return exit_code;
    }

    debug_log(&format!("[monitor] process started, pid={child_pid}"));
    let _ = save_process_log(
        &conn,
        &command_name,
        &project_dir,
        ProcessLogType::ProcessStarted,
        None,
    );

    // Exited cleanly (code 0) within the grace period.
    if exited_during_grace {
        let _ = save_process_log(
            &conn,
            &command_name,
            &project_dir,
            ProcessLogType::ProcessExited,
            Some(&exit_message(exit_code)),
        );
        let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);
        done.store(true, Ordering::Relaxed);
        if let Some(handle) = stdin_handle {
            let _ = handle.join();
        }
        return exit_code;
    }

    // Main loop: stream output until the process exits, running cleanup ~60s.
    let mut last_cleanup = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(LineEvent::Stdout(line)) => {
                let _ = save_process_log(
                    &conn,
                    &command_name,
                    &project_dir,
                    ProcessLogType::Stdout,
                    Some(&line),
                );
            }
            Ok(LineEvent::Stderr(line)) => {
                let _ = save_process_log(
                    &conn,
                    &command_name,
                    &project_dir,
                    ProcessLogType::Stderr,
                    Some(&line),
                );
            }
            Ok(LineEvent::Exit(code)) => {
                exit_code = code;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if last_cleanup.elapsed().as_millis() >= CLEANUP_INTERVAL_MS {
            let _ = maybe_run_cleanup(&conn);
            last_cleanup = Instant::now();
        }
    }

    drain_after_exit(&rx, Duration::from_millis(POST_EXIT_DRAIN_MS), |event| {
        record_grace_event(&conn, &command_name, &project_dir, event);
    });

    debug_log(&format!(
        "[monitor] process exited, pid={child_pid}, code={exit_code:?}"
    ));
    let _ = save_process_log(
        &conn,
        &command_name,
        &project_dir,
        ProcessLogType::ProcessExited,
        Some(&exit_message(exit_code)),
    );
    let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);

    done.store(true, Ordering::Relaxed);
    if let Some(handle) = stdin_handle {
        let _ = handle.join();
    }

    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_collects_lines_sent_after_exit_until_disconnect() {
        let (tx, rx) = mpsc::channel::<LineEvent>();
        let reader = tx.clone();
        tx.send(LineEvent::Exit(Some(1))).unwrap();
        drop(tx);
        // A reader thread that forwards its last lines after the wait thread
        // has already reported the exit.
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            reader.send(LineEvent::Stderr("late error".into())).unwrap();
            reader
                .send(LineEvent::Stdout("late output".into()))
                .unwrap();
        });

        // The supervisor has already consumed the Exit event.
        assert!(matches!(rx.recv().unwrap(), LineEvent::Exit(Some(1))));

        let mut lines = Vec::new();
        let saved = drain_after_exit(&rx, Duration::from_secs(5), |e| match e {
            LineEvent::Stdout(l) | LineEvent::Stderr(l) => lines.push(l),
            LineEvent::Exit(_) => unreachable!(),
        });
        handle.join().unwrap();
        assert_eq!(saved, 2);
        assert_eq!(lines, vec!["late error", "late output"]);
    }

    #[test]
    fn drain_gives_up_when_a_pipe_stays_open() {
        let (tx, rx) = mpsc::channel::<LineEvent>();
        tx.send(LineEvent::Stdout("queued".into())).unwrap();
        let start = Instant::now();
        // `tx` stays alive, like a grandchild holding the pipe open.
        let saved = drain_after_exit(&rx, Duration::from_millis(100), |_| {});
        assert_eq!(saved, 1);
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(tx);
    }

    #[test]
    fn post_exit_drain_is_bounded_to_half_a_second() {
        assert_eq!(POST_EXIT_DRAIN_MS, 500);
        let (tx, rx) = mpsc::channel::<LineEvent>();
        let start = Instant::now();
        drain_after_exit(&rx, Duration::from_millis(POST_EXIT_DRAIN_MS), |_| {});
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(450), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
        drop(tx);
    }

    #[test]
    fn start_failed_message_distinguishes_a_deliberate_stop() {
        assert_eq!(
            start_failed_message(Some(2), false),
            "Process failed to start: exited with code 2"
        );
        assert_eq!(
            start_failed_message(None, false),
            "Process failed to start: stopped by a signal"
        );
        assert_eq!(
            start_failed_message(None, true),
            STOPPED_WHILE_STARTING_MESSAGE
        );
    }

    #[test]
    fn stopped_by_candle_reads_the_kill_mark() {
        use crate::db::process_table::update_process_killed_at;
        use crate::db::{get_database, temp_db_dir};

        let dir = temp_db_dir("monitor-stopped-by-candle");
        let conn = get_database(Some(&dir)).unwrap();
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "svc".to_string(),
                project_dir: "/proj".to_string(),
                pid: 4242,
                log_collector_pid: None,
                shell: None,
                root: None,
            },
        )
        .unwrap();

        assert!(!stopped_by_candle(&conn, "svc", "/proj", 4242));
        update_process_killed_at(&conn, "svc", "/proj", 4242, 1).unwrap();
        assert!(stopped_by_candle(&conn, "svc", "/proj", 4242));
        delete_process_entry(&conn, "svc", "/proj", 4242).unwrap();
        assert!(stopped_by_candle(&conn, "svc", "/proj", 4242));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
