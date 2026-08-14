//! `watch` command handler.
//!
//! Streams live process logs to the console, polling the `process_output` table
//! until interrupted (Ctrl+C / SIGTERM) or an optional `exit_after_ms` deadline
//! is reached. `watch` never launches processes — it only observes. It is also
//! reused by `start`/`restart` in interactive mode to follow a fresh launch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::config::file::find_project_dir;
use crate::db::process_table::find_processes_by_command_name_and_project_dir;
use crate::errors::CandleError;
use crate::log_filters::{ExecutionStatusTracker, LatestExecutionLogFilter, ShowPastLogsBehavior};
use crate::logs::console_log::{
    console_log_row, console_log_system_message, ConsoleLogOptions, OutputFormat,
};
use crate::logs::process_logs::ProcessLog;
use crate::logs::LogIterator;
use crate::output;
use crate::process_alive::filter_alive_processes;

const INITIAL_LOG_COUNT: i64 = 100;
const POLL_INTERVAL: u64 = 200;
const RECENT_LOG_WINDOW_MS: u64 = 10_000;

/// Set by SIGINT/SIGTERM handlers to break the poll loop. Reset at the top of
/// each [`watch_process`] call.
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Apply a batch of logs to the status tracker, then filter and render it.
///
/// Factored out because both the initial batch and each poll batch need the same
/// `tracker.apply` -> `filter` -> render sequence, and borrowing `tracker` +
/// `filter` mutably as closures alongside `conn` would be awkward.
fn print_batch(
    logs: &[ProcessLog],
    is_blended: bool,
    tracker: &mut ExecutionStatusTracker,
    filter: &mut LatestExecutionLogFilter,
) {
    tracker.apply(logs);
    let filtered = filter.filter(logs);
    for log in &filtered {
        let opts = ConsoleLogOptions {
            format: Some(OutputFormat::Pretty),
            prefix: if is_blended {
                Some(format!("[{}] ", log.command_name))
            } else {
                None
            },
            enable_app_name_prefix: false,
        };
        console_log_row(log, &opts);
    }
}

/// Stream logs for the given command(s) to the console until interrupted or the
/// optional deadline is reached. An empty `command_names` watches every process
/// in the project.
///
/// `show_past_logs` / `recent_window_ms` control how much history is replayed
/// before live streaming begins: the `watch` command replays recent logs, while
/// `start` in interactive mode only shows logs from the launch it just made.
pub fn watch_process(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    exit_after_ms: Option<u64>,
    show_past_logs: ShowPastLogsBehavior,
    recent_window_ms: Option<u64>,
) -> rusqlite::Result<()> {
    // With one explicit name there's no ambiguity; anything else (multiple
    // names, or the watch-everything case) prefixes each line with its name.
    let is_blended = command_names.len() != 1;

    let mut iterator = LogIterator::new(project_dir.to_string(), command_names.to_vec());

    // Filter to only show logs from the most recent process launch for each
    // command, optionally pruning to a recent time window so we don't spam
    // history from long-running services when `watch` is invoked.
    let mut filter = LatestExecutionLogFilter::new(show_past_logs, recent_window_ms);

    let initial_logs = iterator.get_next_logs(conn, Some(INITIAL_LOG_COUNT))?;
    filter.check_latest_launch_status(&initial_logs);

    let mut tracker = ExecutionStatusTracker::new();

    // Install signal handlers and reset the stop flag.
    STOP.store(false, Ordering::SeqCst);
    unsafe {
        libc::signal(libc::SIGINT, handle_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handle_signal as *const () as libc::sighandler_t);
    }

    let deadline = exit_after_ms
        .filter(|ms| *ms > 0)
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    // Print the initial batch (already fetched for the status check).
    print_batch(&initial_logs, is_blended, &mut tracker, &mut filter);

    loop {
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                console_log_system_message(
                    OutputFormat::Pretty,
                    &format!("Exiting watch mode after {}ms timeout", exit_after_ms.unwrap()),
                    "",
                );
                break;
            }
        }

        let batch = iterator.get_next_logs(conn, None)?;
        print_batch(&batch, is_blended, &mut tracker, &mut filter);

        sleep(Duration::from_millis(POLL_INTERVAL));
    }

    let running = tracker.count_running_processes();
    if running == 1 {
        console_log_system_message(
            OutputFormat::Pretty,
            "Stopped watching. Process is still running in the background.",
            "",
        );
    } else if running > 1 {
        console_log_system_message(
            OutputFormat::Pretty,
            &format!("Stopped watching. {running} processes are still running in the background."),
            "",
        );
    }

    // Restore default signal handlers.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }

    Ok(())
}

/// Handle the `watch` command.
///
/// `watch` only observes — it never launches processes.
/// - With no names, it watches everything in the project (including processes
///   that haven't launched yet) and always succeeds.
/// - With names, every named process must currently be running.
pub fn handle_watch(
    conn: &Connection,
    cwd: &std::path::Path,
    command_names: &[String],
    exit_after_ms: Option<u64>,
) -> Result<(), CandleError> {
    let project_dir_path = find_project_dir(cwd)?;
    let project_dir = project_dir_path.display().to_string();

    if command_names.is_empty() {
        output::out("Watching all processes in this project.");
    } else {
        // Each named process must be running.
        for name in command_names {
            let existing =
                find_processes_by_command_name_and_project_dir(conn, name, &project_dir)
                    .map_err(|e| CandleError::Generic(format!("database error: {e}")))?;
            let not_killed: Vec<_> =
                existing.into_iter().filter(|p| p.killed_at.is_none()).collect();
            let running = filter_alive_processes(conn, not_killed)
                .map_err(|e| CandleError::Generic(format!("database error: {e}")))?;
            if running.is_empty() {
                return Err(CandleError::UsageError(format!(
                    "Process '{name}' is not running. Start it with: candle start {name}"
                )));
            }
        }

        if command_names.len() == 1 {
            output::out(&format!("Watching process '{}'", command_names[0]));
        } else {
            output::out(&format!("Watching {} processes:", command_names.len()));
            for name in command_names {
                output::out(&format!("  - '{name}'"));
            }
        }
    }
    output::out("Press Ctrl+C to stop watching.");
    output::out("");

    watch_process(
        conn,
        &project_dir,
        command_names,
        exit_after_ms,
        ShowPastLogsBehavior::ShowLogsFromPreviousLaunch,
        Some(RECENT_LOG_WINDOW_MS),
    )
    .map_err(|e| CandleError::Generic(format!("database error: {e}")))?;

    Ok(())
}

/// Follow the logs of service(s) that were just launched by `start`/`restart`
/// in interactive mode. Shows only logs from the fresh launch (no stale
/// history), streaming until Ctrl+C — which detaches and leaves the processes
/// running — or the optional deadline.
pub fn watch_started_services(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    exit_after_ms: Option<u64>,
) -> Result<(), CandleError> {
    output::out("[Now watching console logs. Press Ctrl+C to stop watching.]");
    output::out("");

    watch_process(
        conn,
        project_dir,
        command_names,
        exit_after_ms,
        ShowPastLogsBehavior::OnlyShowAfterRecentLaunch,
        None,
    )
    .map_err(|e| CandleError::Generic(format!("database error: {e}")))?;

    Ok(())
}
