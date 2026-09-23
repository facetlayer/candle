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
use crate::db::process_table::find_running_processes_by_project_dir;
use crate::errors::CandleError;
use crate::log_filters::LatestRunFilter;
use crate::logs::console_log::{
    console_log_row, console_log_system_message, ConsoleLogOptions, OutputFormat,
};
use crate::logs::process_logs::ProcessLog;
use crate::logs::LogIterator;
use crate::output;
use crate::process_alive::{filter_alive_processes, is_service_running};

const INITIAL_LOG_COUNT: i64 = 100;
const POLL_INTERVAL: u64 = 200;
const RECENT_LOG_WINDOW_MS: u64 = 10_000;

/// Set by SIGINT/SIGTERM handlers to break the poll loop. Reset at the top of
/// each [`watch_process`] call.
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Filter a batch of logs to the latest runs and render it.
fn print_batch(logs: &[ProcessLog], is_blended: bool, filter: &mut LatestRunFilter) {
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

/// How many of the watched services (every service when `command_names` is
/// empty) have a live process.
fn count_running_services(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
) -> rusqlite::Result<usize> {
    let mut names: Vec<String> = filter_alive_processes(
        conn,
        find_running_processes_by_project_dir(conn, project_dir)?,
    )?
    .into_iter()
    .map(|p| p.command_name)
    .filter(|name| command_names.is_empty() || command_names.contains(name))
    .collect();
    names.sort();
    names.dedup();
    Ok(names.len())
}

/// Stream logs for the given command(s) to the console until interrupted or the
/// optional deadline is reached. An empty `command_names` watches every process
/// in the project.
///
/// Only each command's latest run is shown. `recent_window_ms` limits how much
/// of it is replayed before live streaming begins: the `watch` command replays
/// only recent logs, while `start` in interactive mode shows the whole launch it
/// just made.
pub fn watch_process(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    exit_after_ms: Option<u64>,
    recent_window_ms: Option<u64>,
) -> rusqlite::Result<()> {
    // With one explicit name there's no ambiguity; anything else (multiple
    // names, or the watch-everything case) prefixes each line with its name.
    let is_blended = command_names.len() != 1;

    let mut iterator = LogIterator::new(project_dir.to_string(), command_names.to_vec());

    let mut filter = LatestRunFilter::new(recent_window_ms);
    filter.seed_latest_runs(conn, project_dir, command_names)?;

    let initial_logs = iterator.get_next_logs(conn, Some(INITIAL_LOG_COUNT))?;

    // Install signal handlers and reset the stop flag.
    STOP.store(false, Ordering::SeqCst);
    unsafe {
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
    }

    let deadline = exit_after_ms
        .filter(|ms| *ms > 0)
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    // Print the initial batch (already fetched for the status check).
    print_batch(&initial_logs, is_blended, &mut filter);

    loop {
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                console_log_system_message(
                    OutputFormat::Pretty,
                    &format!(
                        "Exiting watch mode after {}ms timeout",
                        exit_after_ms.unwrap()
                    ),
                    "",
                );
                break;
            }
        }

        let batch = iterator.get_next_logs(conn, None)?;
        print_batch(&batch, is_blended, &mut filter);

        sleep(Duration::from_millis(POLL_INTERVAL));
    }

    let running = count_running_services(conn, project_dir, command_names)?;
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
            if !is_service_running(conn, &project_dir, name)? {
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
        Some(RECENT_LOG_WINDOW_MS),
    )?;

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

    watch_process(conn, project_dir, command_names, exit_after_ms, None)?;

    Ok(())
}
