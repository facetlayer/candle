//! Poll and render live logs; also used by interactive start/restart.

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

/// Stream latest-run logs until interrupted or the optional deadline.
/// Empty names watch all services. recent_window_ms limits initial replay;
/// interactive starts replay the whole launch.
pub fn watch_process(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    exit_after_ms: Option<u64>,
    recent_window_ms: Option<u64>,
) -> rusqlite::Result<()> {
    let is_blended = command_names.len() != 1;

    let mut iterator = LogIterator::new(project_dir.to_string(), command_names.to_vec());

    let mut filter = LatestRunFilter::new(recent_window_ms);
    filter.seed_latest_runs(conn, project_dir, command_names)?;

    let initial_logs = iterator.get_next_logs(conn, Some(INITIAL_LOG_COUNT))?;

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

    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }

    Ok(())
}

/// Watch all project services, including future launches, or named services
/// that must already be running.
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

/// Follow newly launched services until interruption or deadline.
/// Ctrl+C detaches, leaving services running.
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
