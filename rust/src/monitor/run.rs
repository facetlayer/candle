//! Supervise a service with std threads, recording output and lifecycle rows.
//! Keep supervising background group members after the shell exits.

use std::cell::Cell;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::db::cleanup::{evict_logs_of, maybe_run_cleanup, resolve_eviction_config};
use crate::db::open_database_at;
use crate::db::process_table::{
    create_process_entry, delete_process_entry, find_process_entry, mark_leader_exited,
    CreateProcessEntry,
};
use crate::db::stdin_messages::{clear_stdin_messages, pop_stdin_message};
use crate::debug::debug_log;
use crate::kill::{kill_process_group_and_wait, process_group_alive, KILL_GRACE_PERIOD};
use crate::logs::log_type::{
    killed_by_signal_message, KILLED_BY_SIGNAL, STOPPED_WHILE_STARTING_MESSAGE,
};
use crate::logs::process_logs::{save_run_log, save_run_logs};
use crate::logs::ProcessLogType;
use crate::monitor::MonitorLaunchInfo;

const GRACE_PERIOD_MS: u64 = 500;
const STDIN_POLL_INTERVAL_MS: u64 = 500;
const CLEANUP_INTERVAL_MS: u128 = 60 * 1000;
/// Bound post-exit draining when descendants keep output pipes open.
const POST_EXIT_DRAIN_MS: u64 = 500;
/// Poll interval for group members surviving the shell.
const GROUP_POLL_INTERVAL_MS: u64 = 250;

/// Events forwarded from the reader / wait threads to the supervisor.
enum LineEvent {
    Stdout(String),
    Stderr(String),
    Exit(ChildExit),
}

/// How the child ended: its exit code, or the signal that terminated it.
/// Both are `None` only if waiting on the child failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ChildExit {
    code: Option<i32>,
    signal: Option<i32>,
}

/// Format the exit, distinguishing deliberate stops from signal crashes.
fn exit_message(exit: ChildExit, stopped_by_candle: bool) -> String {
    match (exit.code, exit.signal) {
        (Some(c), _) => format!("Process exited with code {c}"),
        (None, Some(sig)) if !stopped_by_candle => killed_by_signal_message(sig),
        _ => "Process was stopped".to_string(),
    }
}

/// Environment defaults preserving even empty overrides. PYTHONUNBUFFERED
/// prevents Python from buffering output piped to the monitor.
const DEFAULT_SERVICE_ENV: [(&str, &str); 1] = [("PYTHONUNBUFFERED", "1")];

/// The entries of [`DEFAULT_SERVICE_ENV`] that `is_set` doesn't report as set.
fn default_service_env(
    is_set: impl Fn(&str) -> bool,
) -> impl Iterator<Item = (&'static str, &'static str)> {
    DEFAULT_SERVICE_ENV
        .into_iter()
        .filter(move |(name, _)| !is_set(name))
}

/// Split unterminated lines at this size to bound memory.
const MAX_LINE_BYTES: usize = 64 * 1024;

/// Bound queued output; backpressure slows services to the database write rate.
const OUTPUT_QUEUE_LINES: usize = 2 * MAX_BATCH;

/// Where to cut `buf` so that a UTF-8 sequence left incomplete at its end
/// stays with the bytes that follow.
fn utf8_split_point(buf: &[u8]) -> usize {
    match std::str::from_utf8(buf) {
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        _ => buf.len(),
    }
}

/// Forward output until EOF, splitting long lines and decoding invalid UTF-8
/// lossily so a decoding error cannot close the pipe and cause SIGPIPE.
fn forward_lines(pipe: impl Read, tx: mpsc::SyncSender<LineEvent>, event: fn(String) -> LineEvent) {
    let mut reader = BufReader::new(pipe);
    // Carry incomplete UTF-8 bytes across chunks.
    let mut buf = Vec::new();
    loop {
        let room = (MAX_LINE_BYTES - buf.len()) as u64;
        match (&mut reader).take(room).read_until(b'\n', &mut buf) {
            Ok(0) => {
                if !buf.is_empty() {
                    let _ = tx.send(event(String::from_utf8_lossy(&buf).into_owned()));
                }
                return;
            }
            Ok(_) => {
                let mut carry = Vec::new();
                if buf.ends_with(b"\n") {
                    buf.pop();
                    if buf.ends_with(b"\r") {
                        buf.pop();
                    }
                } else if buf.len() >= MAX_LINE_BYTES {
                    carry = buf.split_off(utf8_split_point(&buf));
                }
                let line = String::from_utf8_lossy(&buf).into_owned();
                // Keep draining after supervisor exit to avoid blocking the child or SIGPIPE.
                let _ = tx.send(event(line));
                buf = carry;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// Most output lines written in one transaction.
const MAX_BATCH: usize = 1000;

/// An output line waiting to be written.
type OutputLine = (ProcessLogType, String);

/// Split events into output lines and the exit, if one is among them.
fn split_events(
    events: impl IntoIterator<Item = LineEvent>,
) -> (Vec<OutputLine>, Option<ChildExit>) {
    let mut lines = Vec::new();
    let mut exit = None;
    for event in events {
        match event {
            LineEvent::Stdout(line) => lines.push((ProcessLogType::Stdout, line)),
            LineEvent::Stderr(line) => lines.push((ProcessLogType::Stderr, line)),
            LineEvent::Exit(e) => exit = Some(e),
        }
    }
    (lines, exit)
}

/// Collect already-queued events up to MAX_BATCH or Exit, without waiting.
fn take_queued(
    rx: &mpsc::Receiver<LineEvent>,
    first: LineEvent,
) -> (Vec<OutputLine>, Option<ChildExit>) {
    let mut events = vec![first];
    while events.len() < MAX_BATCH && !matches!(events.last(), Some(LineEvent::Exit(_))) {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(_) => break,
        }
    }
    split_events(events)
}

/// Trim after this many lines...
const TRIM_AFTER_LINES: usize = 100_000;
/// ...or this many bytes, whichever comes first.
const TRIM_AFTER_BYTES: usize = 32 * 1024 * 1024;

/// Writes a service's output lines into `log_lines` for one run.
struct OutputWriter<'a> {
    conn: &'a Connection,
    run_id: Option<i64>,
    command_name: &'a str,
    project_dir: &'a str,
    /// Lines and bytes written since the last trim (see [`Self::trim_if_due`]).
    since_trim: Cell<(usize, usize)>,
}

impl OutputWriter<'_> {
    fn write(&self, lines: &[OutputLine]) {
        if lines.is_empty() {
            return;
        }
        for (log_type, line) in lines {
            debug_log(&format!("[monitor] {log_type:?}: {line}"));
        }
        let _ = save_run_logs(
            self.conn,
            self.run_id,
            self.command_name,
            self.project_dir,
            lines.iter().map(|(t, l)| (*t, l.as_str())),
        );
        self.trim_if_due(lines.len(), lines.iter().map(|(_, l)| l.len()).sum());
    }

    /// Trim flooding output between periodic cleanups. Keep at least the
    /// configured limit and the latest stretch, giving polling readers time
    /// to observe lines before eviction.
    fn trim_if_due(&self, lines: usize, bytes: usize) {
        let (mut total_lines, mut total_bytes) = self.since_trim.get();
        total_lines += lines;
        total_bytes += bytes;
        if total_lines >= TRIM_AFTER_LINES || total_bytes >= TRIM_AFTER_BYTES {
            let limit = resolve_eviction_config(self.project_dir).max_logs_per_service as i64;
            let keep = limit.max(total_lines as i64);
            debug_log(&format!("[monitor] trimming logs to {keep} lines"));
            let _ = evict_logs_of(self.conn, self.project_dir, self.command_name, keep);
            (total_lines, total_bytes) = (0, 0);
        }
        self.since_trim.set((total_lines, total_bytes));
    }

    /// Write queued output through Exit after the child is known to have exited.
    fn write_until_exit(&self, rx: &mpsc::Receiver<LineEvent>) {
        while let Ok(first) = rx.recv() {
            let (lines, exit) = take_queued(rx, first);
            self.write(&lines);
            if exit.is_some() {
                return;
            }
        }
    }

    /// Write the output that arrives after the `Exit` event (see
    /// [`drain_after_exit`]).
    fn write_after_exit(&self, rx: &mpsc::Receiver<LineEvent>) {
        let mut events = Vec::new();
        drain_after_exit(rx, Duration::from_millis(POST_EXIT_DRAIN_MS), |e| {
            events.push(e)
        });
        self.write(&split_events(events).0);
    }
}

/// Keep the row and collect output until the shell's process group is empty.
/// Mark `leader_exited` so kill targets the group. Detached (`setsid`) children
/// are outside this group. Return whether the group outlived the shell.
fn supervise_remaining_group(
    conn: &Connection,
    writer: &OutputWriter<'_>,
    rx: &mpsc::Receiver<LineEvent>,
    pgid: i64,
) -> bool {
    if !process_group_alive(pgid) {
        return false;
    }
    debug_log(&format!(
        "[monitor] shell exited, process group {pgid} still has members"
    ));
    let _ = mark_leader_exited(conn, writer.command_name, writer.project_dir, pgid);

    let poll = Duration::from_millis(GROUP_POLL_INTERVAL_MS);
    let mut pipes_open = true;
    let mut last_cleanup = Instant::now();
    while process_group_alive(pgid) {
        if pipes_open {
            match rx.recv_timeout(poll) {
                Ok(first) => writer.write(&take_queued(rx, first).0),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => pipes_open = false,
            }
        } else {
            thread::sleep(poll);
        }

        if last_cleanup.elapsed().as_millis() >= CLEANUP_INTERVAL_MS {
            let _ = maybe_run_cleanup(conn);
            last_cleanup = Instant::now();
        }
    }
    true
}

/// Format a started service's exit. If the group outlived the shell, the shell
/// status no longer describes the service; honor a later deliberate stop.
fn final_exit_message(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    pid: i64,
    exit: ChildExit,
    outlived: bool,
) -> String {
    let stopped = (outlived || exit.code.is_none())
        && stopped_by_candle(conn, command_name, project_dir, pid);
    if outlived && stopped {
        exit_message(ChildExit::default(), true)
    } else {
        exit_message(exit, stopped)
    }
}

/// Drain late reader output until all senders close or timeout. Exit can arrive
/// before the final pipe output. Return the number of saved lines.
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

/// Format an early exit, distinguishing deliberate stops from startup failures.
fn start_failed_message(exit: ChildExit, stopped_by_candle: bool) -> String {
    match (exit.code, exit.signal) {
        (Some(c), _) => format!("Process failed to start: exited with code {c}"),
        _ if stopped_by_candle => STOPPED_WHILE_STARTING_MESSAGE.to_string(),
        (None, Some(sig)) => format!("Process failed to start: {KILLED_BY_SIGNAL} {sig}"),
        (None, None) => "Process failed to start: stopped by a signal".to_string(),
    }
}

/// A killed or deleted row indicates a deliberate stop: kill marks before
/// signalling, and cleanup/erase may delete the row.
fn stopped_by_candle(conn: &Connection, command_name: &str, project_dir: &str, pid: i64) -> bool {
    match find_process_entry(conn, command_name, project_dir, pid) {
        Ok(Some(entry)) => entry.killed_at.is_some(),
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Supervise until exit or startup failure; return the child's exit code.
pub fn run(launch_info: MonitorLaunchInfo) -> Option<i32> {
    let MonitorLaunchInfo {
        command_name,
        project_dir,
        shell,
        root,
        enable_stdin,
        database_path,
        run_id,
        transient,
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

    let launch_dir = match &root {
        Some(r) => Path::new(&project_dir).join(r),
        None => Path::new(&project_dir).to_path_buf(),
    };

    // Give the service its own group so kill reaches reparented descendants
    // without signalling the monitor. Spawn failures create no process row.
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(&shell)
        .process_group(0)
        .envs(default_service_env(|name| std::env::var_os(name).is_some()))
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
            let _ = save_run_log(
                &conn,
                run_id,
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
            run_id,
            transient,
        },
    );

    let (tx, rx) = mpsc::sync_channel::<LineEvent>(OUTPUT_QUEUE_LINES);

    let stdout = child.stdout.take().expect("stdout piped");
    let tx_out = tx.clone();
    thread::spawn(move || forward_lines(stdout, tx_out, LineEvent::Stdout));

    let stderr = child.stderr.take().expect("stderr piped");
    let tx_err = tx.clone();
    thread::spawn(move || forward_lines(stderr, tx_err, LineEvent::Stderr));

    // Stdin polling needs its own mutable connection; stop when the child exits.
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

    // Also publish exit directly so startup checks bypass any output backlog.
    let exit_slot = Arc::new(OnceLock::<ChildExit>::new());
    let tx_exit = tx;
    let wait_slot = Arc::clone(&exit_slot);
    thread::spawn(move || {
        let exit = match child.wait() {
            Ok(status) => ChildExit {
                code: status.code(),
                signal: status.signal(),
            },
            Err(_) => ChildExit::default(),
        };
        let _ = wait_slot.set(exit);
        let _ = tx_exit.send(LineEvent::Exit(exit));
    });

    let writer = OutputWriter {
        conn: &conn,
        run_id,
        command_name: &command_name,
        project_dir: &project_dir,
        since_trim: Cell::new((0, 0)),
    };

    let grace_deadline = Instant::now() + Duration::from_millis(GRACE_PERIOD_MS);
    let mut exited_during_grace = false;
    let mut exit = ChildExit::default();

    loop {
        let remaining = grace_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(first) => {
                let (lines, event_exit) = take_queued(&rx, first);
                writer.write(&lines);
                if let Some(e) = event_exit {
                    exited_during_grace = true;
                    exit = e;
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Consult the exit slot: a queued Exit may sit behind startup output.
    if !exited_during_grace {
        if let Some(e) = exit_slot.get() {
            exited_during_grace = true;
            exit = *e;
            writer.write_until_exit(&rx);
        }
    }

    if exited_during_grace {
        writer.write_after_exit(&rx);
    }

    if exited_during_grace && exit.code != Some(0) {
        debug_log(&format!(
            "[monitor] process failed during grace period, pid={child_pid}, {exit:?}"
        ));
        let _ = save_run_log(
            &conn,
            run_id,
            &command_name,
            &project_dir,
            ProcessLogType::ProcessStartFailed,
            Some(&start_failed_message(
                exit,
                exit.code.is_none()
                    && stopped_by_candle(&conn, &command_name, &project_dir, child_pid),
            )),
        );
        let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);
        // Stop background children left by the failed start.
        let _ = kill_process_group_and_wait(child_pid, KILL_GRACE_PERIOD);
        done.store(true, Ordering::Relaxed);
        if let Some(handle) = stdin_handle {
            let _ = handle.join();
        }
        return exit.code;
    }

    debug_log(&format!("[monitor] process started, pid={child_pid}"));
    let _ = save_run_log(
        &conn,
        run_id,
        &command_name,
        &project_dir,
        ProcessLogType::ProcessStarted,
        None,
    );

    if exited_during_grace {
        let outlived = supervise_remaining_group(&conn, &writer, &rx, child_pid);
        writer.write_after_exit(&rx);
        let _ = save_run_log(
            &conn,
            run_id,
            &command_name,
            &project_dir,
            ProcessLogType::ProcessExited,
            Some(&final_exit_message(
                &conn,
                &command_name,
                &project_dir,
                child_pid,
                exit,
                outlived,
            )),
        );
        let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);
        done.store(true, Ordering::Relaxed);
        if let Some(handle) = stdin_handle {
            let _ = handle.join();
        }
        return exit.code;
    }

    let mut last_cleanup = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(first) => {
                let (lines, event_exit) = take_queued(&rx, first);
                writer.write(&lines);
                if let Some(e) = event_exit {
                    exit = e;
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if last_cleanup.elapsed().as_millis() >= CLEANUP_INTERVAL_MS {
            let _ = maybe_run_cleanup(&conn);
            last_cleanup = Instant::now();
        }
    }

    let outlived = supervise_remaining_group(&conn, &writer, &rx, child_pid);
    writer.write_after_exit(&rx);

    debug_log(&format!(
        "[monitor] process exited, pid={child_pid}, {exit:?}"
    ));
    let _ = save_run_log(
        &conn,
        run_id,
        &command_name,
        &project_dir,
        ProcessLogType::ProcessExited,
        Some(&final_exit_message(
            &conn,
            &command_name,
            &project_dir,
            child_pid,
            exit,
            outlived,
        )),
    );
    let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);

    done.store(true, Ordering::Relaxed);
    if let Some(handle) = stdin_handle {
        let _ = handle.join();
    }

    exit.code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_service_env_skips_variables_already_set() {
        assert_eq!(
            default_service_env(|_| false).collect::<Vec<_>>(),
            vec![("PYTHONUNBUFFERED", "1")]
        );
        assert_eq!(
            default_service_env(|name| name == "PYTHONUNBUFFERED").count(),
            0
        );
    }

    #[test]
    fn drain_collects_lines_sent_after_exit_until_disconnect() {
        let (tx, rx) = mpsc::channel::<LineEvent>();
        let reader = tx.clone();
        tx.send(LineEvent::Exit(ChildExit::default())).unwrap();
        drop(tx);
        // Simulate output arriving after Exit.
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            reader.send(LineEvent::Stderr("late error".into())).unwrap();
            reader
                .send(LineEvent::Stdout("late output".into()))
                .unwrap();
        });

        assert!(matches!(rx.recv().unwrap(), LineEvent::Exit(_)));

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
    fn take_queued_batches_what_is_queued_and_stops_at_exit() {
        let (tx, rx) = mpsc::channel::<LineEvent>();
        tx.send(LineEvent::Stderr("b".into())).unwrap();
        tx.send(LineEvent::Exit(ChildExit {
            code: Some(3),
            signal: None,
        }))
        .unwrap();
        tx.send(LineEvent::Stdout("after exit".into())).unwrap();

        let (lines, exit) = take_queued(&rx, LineEvent::Stdout("a".into()));
        assert_eq!(
            lines,
            vec![
                (ProcessLogType::Stdout, "a".to_string()),
                (ProcessLogType::Stderr, "b".to_string()),
            ]
        );
        assert_eq!(
            exit,
            Some(ChildExit {
                code: Some(3),
                signal: None
            })
        );
        assert!(matches!(rx.try_recv(), Ok(LineEvent::Stdout(_))));
    }

    #[test]
    fn take_queued_caps_the_batch() {
        let (tx, rx) = mpsc::channel::<LineEvent>();
        for i in 0..(MAX_BATCH + 5) {
            tx.send(LineEvent::Stdout(i.to_string())).unwrap();
        }
        let first = rx.recv().unwrap();
        let (lines, exit) = take_queued(&rx, first);
        assert_eq!(lines.len(), MAX_BATCH);
        assert_eq!(exit, None);
        assert_eq!(rx.try_iter().count(), 5);
    }

    #[test]
    fn take_queued_does_not_wait_for_more_output() {
        let (tx, rx) = mpsc::channel::<LineEvent>();
        let start = Instant::now();
        let (lines, _) = take_queued(&rx, LineEvent::Stdout("only".into()));
        assert_eq!(lines.len(), 1);
        assert!(start.elapsed() < Duration::from_millis(100));
        drop(tx);
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
        let code = |c| ChildExit {
            code: Some(c),
            signal: None,
        };
        let signal = |s| ChildExit {
            code: None,
            signal: Some(s),
        };
        assert_eq!(
            start_failed_message(code(2), false),
            "Process failed to start: exited with code 2"
        );
        assert_eq!(
            start_failed_message(signal(11), false),
            "Process failed to start: killed by signal 11"
        );
        assert_eq!(
            start_failed_message(signal(15), true),
            STOPPED_WHILE_STARTING_MESSAGE
        );
    }

    #[test]
    fn exit_message_tells_a_crash_from_a_deliberate_stop() {
        let signal = ChildExit {
            code: None,
            signal: Some(9),
        };
        assert_eq!(
            exit_message(signal, false),
            "Process was killed by signal 9"
        );
        assert_eq!(exit_message(signal, true), "Process was stopped");
        let code = ChildExit {
            code: Some(0),
            signal: None,
        };
        assert_eq!(exit_message(code, false), "Process exited with code 0");
    }

    fn forwarded(input: &[u8]) -> Vec<String> {
        let (tx, rx) = mpsc::sync_channel::<LineEvent>(1024);
        forward_lines(input, tx, LineEvent::Stdout);
        rx.iter()
            .map(|e| match e {
                LineEvent::Stdout(l) => l,
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn forward_lines_survives_invalid_utf8() {
        assert_eq!(
            forwarded(b"a\xffb\r\nnext\nlast"),
            vec!["a\u{fffd}b", "next", "last"]
        );
    }

    #[test]
    fn forward_lines_splits_an_overlong_line() {
        let mut input = vec![b'x'; MAX_LINE_BYTES * 2 + MAX_LINE_BYTES / 2];
        input.extend_from_slice(b"\nshort\n");
        let lines = forwarded(&input);
        assert_eq!(
            lines.iter().map(String::len).collect::<Vec<_>>(),
            vec![MAX_LINE_BYTES, MAX_LINE_BYTES, MAX_LINE_BYTES / 2, 5]
        );
        assert_eq!(lines[3], "short");

        let mut exact = vec![b'y'; MAX_LINE_BYTES - 1];
        exact.extend_from_slice(b"\nnext\n");
        assert_eq!(forwarded(&exact).len(), 2);
    }

    #[test]
    fn forward_lines_does_not_split_inside_a_character() {
        // 'é' is two bytes; an odd offset puts one of them across the cut.
        let mut input = vec![b'a'];
        input.extend("é".repeat(MAX_LINE_BYTES).bytes());
        let lines = forwarded(&input);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| !l.contains('\u{fffd}')));
        assert_eq!(lines.concat().len(), input.len());
    }

    #[test]
    fn a_full_queue_holds_the_reader_back() {
        let (tx, rx) = mpsc::sync_channel::<LineEvent>(2);
        let input = b"1\n2\n3\n4\n5\n".to_vec();
        let handle = thread::spawn(move || forward_lines(&input[..], tx, LineEvent::Stdout));
        thread::sleep(Duration::from_millis(100));
        assert!(!handle.is_finished());
        assert_eq!(rx.iter().count(), 5);
        handle.join().unwrap();
    }

    #[test]
    fn a_flood_is_trimmed_as_it_is_written() {
        use crate::db::{get_database, temp_db_dir};
        use crate::logs::process_logs::start_run;

        let dir = temp_db_dir("monitor-flood-trim");
        let conn = get_database(Some(&dir)).unwrap();
        let run_id = start_run(&conn, "svc", "/proj").unwrap();
        let writer = OutputWriter {
            conn: &conn,
            run_id: Some(run_id),
            command_name: "svc",
            project_dir: "/proj",
            since_trim: Cell::new((0, 0)),
        };
        let batch: Vec<OutputLine> = (0..MAX_BATCH)
            .map(|i| (ProcessLogType::Stdout, format!("line {i}")))
            .collect();
        let count = |conn: &Connection| -> i64 {
            conn.query_row("select count(*) from log_lines", [], |row| row.get(0))
                .unwrap()
        };

        // Three stretches' worth: never more than about two are kept.
        let mut most = 0;
        for _ in 0..(3 * TRIM_AFTER_LINES / MAX_BATCH) {
            writer.write(&batch);
            most = most.max(count(&conn));
        }
        assert!(most <= 2 * TRIM_AFTER_LINES as i64 + 1, "{most}");
        assert_eq!(count(&conn), TRIM_AFTER_LINES as i64);
        let last: String = conn
            .query_row(
                "select content from log_lines order by id desc limit 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(last, format!("line {}", MAX_BATCH - 1));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
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
                run_id: None,
                transient: false,
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
