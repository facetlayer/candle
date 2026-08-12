//! The log-collector supervision lifecycle.
//!
//! Ports `main()` from `src/main-log-collector.ts` and `startMonitoredService`
//! from `src/log-collector/startMonitoredService.ts`, using std threads (no
//! tokio). The flow:
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

use crate::db::cleanup::maybe_run_cleanup;
use crate::db::open_database_at;
use crate::db::process_table::{create_process_entry, delete_process_entry, CreateProcessEntry};
use crate::db::stdin_messages::{clear_stdin_messages, pop_stdin_message};
use crate::debug::debug_log;
use crate::log_collector::LogCollectorLaunchInfo;
use crate::logs::process_logs::save_process_log;
use crate::logs::ProcessLogType;

const GRACE_PERIOD_MS: u64 = 500;
const STDIN_POLL_INTERVAL_MS: u64 = 500;
const CLEANUP_INTERVAL_MS: u128 = 60 * 1000;

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

/// Human-readable message for a process that died during the startup grace
/// period.
fn start_failed_message(code: Option<i32>) -> String {
    match code {
        Some(c) => format!("Process failed to start: exited with code {c}"),
        None => "Process failed to start: stopped by a signal".to_string(),
    }
}

/// Run the supervision lifecycle to completion, blocking until the child exits
/// (or a startup failure short-circuits). Returns the child's exit code, if any.
pub fn run(launch_info: LogCollectorLaunchInfo) -> Option<i32> {
    let LogCollectorLaunchInfo {
        command_name,
        project_dir,
        shell,
        root,
        enable_stdin,
        database_path,
    } = launch_info;

    debug_log(&format!(
        "[log-collector] starting shell command {shell:?} enableStdin={enable_stdin}"
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

    // launchDir = root ? join(projectDir, root) : projectDir. Per
    // startMonitoredService.ts, the join is unconditional (no absolute-root
    // special-casing).
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
            debug_log(&format!("[log-collector] failed to start: {e}"));
            let _ = save_process_log(
                &conn,
                &command_name,
                &project_dir,
                ProcessLogType::ProcessStartFailed,
                Some(&format!("Process failed to start: {e}")),
            );
            std::process::exit(1);
        }
    };

    let child_pid = child.id() as i64;
    let my_pid = std::process::id() as i64;

    debug_log(&format!("[log-collector] launched subprocess, pid={child_pid}"));

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
                        debug_log(&format!("[log-collector] writing stdin message: {}", msg.data));
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
            Ok(LineEvent::Stdout(line)) => {
                debug_log(&format!("[log-collector] stdout: {line}"));
                let _ = save_process_log(
                    &conn,
                    &command_name,
                    &project_dir,
                    ProcessLogType::Stdout,
                    Some(&line),
                );
            }
            Ok(LineEvent::Stderr(line)) => {
                debug_log(&format!("[log-collector] stderr: {line}"));
                let _ = save_process_log(
                    &conn,
                    &command_name,
                    &project_dir,
                    ProcessLogType::Stderr,
                    Some(&line),
                );
            }
            Ok(LineEvent::Exit(code)) => {
                exited_during_grace = true;
                exit_code = code;
                break;
            }
            Err(_) => break,
        }
    }

    // A nonzero exit within the grace period is a start failure: log it, delete
    // the row, and stop. (Asymmetry vs the spawn-failure branch above, which
    // never created a row.)
    if exited_during_grace && exit_code != Some(0) {
        debug_log(&format!(
            "[log-collector] process failed during grace period, pid={child_pid}, code={exit_code:?}"
        ));
        let _ = save_process_log(
            &conn,
            &command_name,
            &project_dir,
            ProcessLogType::ProcessStartFailed,
            Some(&start_failed_message(exit_code)),
        );
        let _ = delete_process_entry(&conn, &command_name, &project_dir, child_pid);
        done.store(true, Ordering::Relaxed);
        if let Some(handle) = stdin_handle {
            let _ = handle.join();
        }
        return exit_code;
    }

    debug_log(&format!("[log-collector] process started, pid={child_pid}"));
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

    debug_log(&format!(
        "[log-collector] process exited, pid={child_pid}, code={exit_code:?}"
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
