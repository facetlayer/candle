//! The monitor-mode supervision lifecycle.
//!
//! Uses std threads (no tokio). The flow:
//!
//! 1. open the DB and spawn `sh -c <shell>` (cwd = projectDir[/root]);
//! 2. register a `processes` row (pid = shell, log_collector_pid = self);
//! 3. stream stdout/stderr lines into `log_lines`;
//! 4. a 500ms grace period distinguishes a fast failure from a real start;
//! 5. poll the stdin queue (when enabled) and run periodic cleanup;
//! 6. when the shell exits, keep going while its process group still has
//!    members (something it started in the background);
//! 7. then log `process_exited` and delete the `processes` row.

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
/// After the child exits, how long to keep collecting output the reader threads
/// haven't forwarded yet. Normally both pipes close right after the exit and
/// the drain ends at once; the timeout only matters when a background
/// grandchild inherited the pipes and holds them open, possibly forever. Losing
/// a few lines written after the timeout in that case is acceptable, and
/// keeping it short means the exit (and `ps` / `kill`) isn't held up.
const POST_EXIT_DRAIN_MS: u64 = 500;
/// Once the service's shell has exited, how often to check whether its
/// process group has emptied.
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

/// Human-readable message for a process exit after a successful start. A
/// signal Candle sent (`kill` / `restart`) is a deliberate stop; any other
/// signal (a segfault, the OOM killer) is a crash that `ps` shows as `FAILED`.
fn exit_message(exit: ChildExit, stopped_by_candle: bool) -> String {
    match (exit.code, exit.signal) {
        (Some(c), _) => format!("Process exited with code {c}"),
        (None, Some(sig)) if !stopped_by_candle => killed_by_signal_message(sig),
        _ => "Process was stopped".to_string(),
    }
}

/// Variables added to a service's environment unless it already has them
/// (even set to an empty value, which is how to turn one off).
///
/// `PYTHONUNBUFFERED`: the service's stdout is a pipe, so Python would hold
/// its output in a block buffer and `logs` / `wait-for-log` would see nothing
/// until the buffer filled or the program exited.
const DEFAULT_SERVICE_ENV: [(&str, &str); 1] = [("PYTHONUNBUFFERED", "1")];

/// The entries of [`DEFAULT_SERVICE_ENV`] that `is_set` doesn't report as set.
fn default_service_env(
    is_set: impl Fn(&str) -> bool,
) -> impl Iterator<Item = (&'static str, &'static str)> {
    DEFAULT_SERVICE_ENV
        .into_iter()
        .filter(move |(name, _)| !is_set(name))
}

/// Longest line stored as one log row. Output with no newline for longer than
/// this is split into rows of this size, so a service that never ends its line
/// can't grow the monitor without limit.
const MAX_LINE_BYTES: usize = 64 * 1024;

/// How many output lines can wait to be written to the database. When the
/// queue is full the reader threads stop reading, the pipe fills, and the
/// service blocks on its next write: a service that prints faster than the
/// database can take it is slowed to that rate instead of the backlog piling
/// up in the monitor's memory.
const OUTPUT_QUEUE_LINES: usize = 2 * MAX_BATCH;

/// Where to cut `buf` so that a UTF-8 sequence left incomplete at its end
/// stays with the bytes that follow.
fn utf8_split_point(buf: &[u8]) -> usize {
    match std::str::from_utf8(buf) {
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        _ => buf.len(),
    }
}

/// Forward each line of a child's output pipe as an event until EOF.
///
/// Reads bytes rather than `String`s so output that isn't valid UTF-8 is
/// decoded lossily instead of ending the read: closing the pipe early would
/// kill the child with SIGPIPE on its next write. A line longer than
/// [`MAX_LINE_BYTES`] is forwarded in pieces.
fn forward_lines(pipe: impl Read, tx: mpsc::SyncSender<LineEvent>, event: fn(String) -> LineEvent) {
    let mut reader = BufReader::new(pipe);
    // Holds the start of a line across iterations only when a split left an
    // incomplete UTF-8 sequence to carry over.
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
                // If the supervisor is gone this fails at once; keep draining
                // so the child never blocks or gets SIGPIPE.
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

/// `first` plus the events already queued behind it, up to [`MAX_BATCH`].
/// Never waits: a lone line on a quiet service is written at once. Stops at
/// an `Exit` so the caller sees it right after the lines that preceded it.
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

/// While a service floods its output, the monitor trims that service's logs
/// each time it has written this many lines...
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

    /// Bound what a flooding service can put in the database.
    ///
    /// `maxLogsPerService` is applied by the periodic cleanup, which runs at
    /// most every ten minutes; a service in a print loop can write gigabytes in
    /// that time. So after every [`TRIM_AFTER_LINES`] lines (or
    /// [`TRIM_AFTER_BYTES`]) the service's older rows are deleted, keeping what
    /// was written since the previous trim (and never fewer rows than
    /// `maxLogsPerService`). The database then holds at most about two such
    /// stretches per service. Keeping the latest stretch rather than cutting
    /// straight down to the limit gives `wait-for-log` and `watch`, which poll,
    /// time to see every line before it goes.
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

    /// Receive and write output until the `Exit` event, which the wait thread
    /// sends after every line queued before the exit. Used once the exit is
    /// already known, so this doesn't wait on a live process.
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

/// Keep supervising after the service's shell has exited, for as long as its
/// process group (`pgid`, the shell's PID) still has members.
///
/// A command that starts something in the background and returns
/// (`server & echo started`) leaves that process running in the service's
/// group. Reporting the service as exited at that point would put the process
/// out of reach of `ps` and `kill`. So the row stays, marked `leader_exited` so
/// that `kill` signals the group instead of the shell's PID, and output keeps
/// being collected until the group is empty. A process that has left the group
/// (`setsid`) isn't covered. Returns whether the group outlived the shell.
///
/// A group id isn't reused while the group has members, so the check can't
/// mistake another group for this one while the service is still around.
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

/// The `process_exited` message for a service that had started. `outlived` is
/// set when the process group outlived the shell (see
/// [`supervise_remaining_group`]): the shell's own exit status then says
/// nothing about how the rest of the service ended, so a stop by Candle is
/// reported as one.
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
fn start_failed_message(exit: ChildExit, stopped_by_candle: bool) -> String {
    match (exit.code, exit.signal) {
        (Some(c), _) => format!("Process failed to start: exited with code {c}"),
        _ if stopped_by_candle => STOPPED_WHILE_STARTING_MESSAGE.to_string(),
        (None, Some(sig)) => format!("Process failed to start: {KILLED_BY_SIGNAL} {sig}"),
        (None, None) => "Process failed to start: stopped by a signal".to_string(),
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

    // launchDir = root ? join(projectDir, root) : projectDir. `Path::join`
    // replaces the base when `root` is absolute, so this is the same directory
    // `resolve_launch_dir` reports in the start banner and `list`.
    let launch_dir = match &root {
        Some(r) => Path::new(&project_dir).join(r),
        None => Path::new(&project_dir).to_path_buf(),
    };

    // Spawn the monitored service. On spawn failure: log process_start_failed,
    // exit, do NOT create (or delete) a process row.
    //
    // The service leads its own process group (pgid == its pid), separate from
    // the monitor's. Descendants stay in that group even after they're
    // reparented (a double-forked `(daemon &)`), so `kill` can reach them with
    // one group signal without also stopping the monitor.
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

    // Reader + wait threads forward events over a bounded channel (see
    // `OUTPUT_QUEUE_LINES`).
    let (tx, rx) = mpsc::sync_channel::<LineEvent>(OUTPUT_QUEUE_LINES);

    let stdout = child.stdout.take().expect("stdout piped");
    let tx_out = tx.clone();
    thread::spawn(move || forward_lines(stdout, tx_out, LineEvent::Stdout));

    let stderr = child.stderr.take().expect("stderr piped");
    let tx_err = tx.clone();
    thread::spawn(move || forward_lines(stderr, tx_err, LineEvent::Stderr));

    // Stdin polling thread (own DB connection; pop needs &mut). The `done` flag
    // lets us stop it once the child exits.
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

    // Wait thread: reports the exit once the child terminates. The exit also
    // goes into `exit_slot`, because on the channel it can sit behind a large
    // backlog of output; the start decision reads the slot so it never waits
    // for that backlog to be written.
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

    // Grace period: collect output until the deadline or an early exit.
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

    // The child may have exited within the window with its `Exit` event still
    // queued behind output (a process that dies instantly still prints its
    // error first). Ask the wait thread directly rather than writing out the
    // queue first: a service that prints a lot at startup would otherwise hold
    // up the start decision until its whole backlog was in the database.
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

    // A nonzero exit within the grace period is a start failure: log it, delete
    // the row, and stop. (Asymmetry vs the spawn-failure branch above, which
    // never created a row.)
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
        // A failed start leaves nothing running: stop whatever the shell had
        // already put in the background.
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

    // Exited cleanly (code 0) within the grace period.
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

    // Main loop: stream output until the process exits, running cleanup ~60s.
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
        // The line after the exit is left for the post-exit drain.
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
        // Two and a half rows' worth with no newline, then a normal line.
        let mut input = vec![b'x'; MAX_LINE_BYTES * 2 + MAX_LINE_BYTES / 2];
        input.extend_from_slice(b"\nshort\n");
        let lines = forwarded(&input);
        assert_eq!(
            lines.iter().map(String::len).collect::<Vec<_>>(),
            vec![MAX_LINE_BYTES, MAX_LINE_BYTES, MAX_LINE_BYTES / 2, 5]
        );
        assert_eq!(lines[3], "short");

        // A line of exactly the maximum isn't followed by an empty row.
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
        // Nothing has been received, so the reader is parked with the queue full.
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
        // The newest line is still there.
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
