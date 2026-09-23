//! `wait-for-log` command handler.
//!
//! Ported from `src/wait-for-log-command.ts`. Polls the `process_output` table
//! for a given substring, scoped to the most recent launch of the named
//! service(s), until the message appears, the process exits, or a timeout is hit.
//!
//! A service that isn't running fails at once rather than waiting out the
//! timeout: nothing will ever write the message.

use std::thread::sleep;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::db::process_table::find_running_processes_by_project_dir;
use crate::log_filters::LatestRunFilter;
use crate::logs::console_log::{console_log_row, ConsoleLogOptions, OutputFormat};
use crate::logs::process_logs::{
    get_log_tail, get_process_logs, latest_run_ids, LogSearchOptions, ProcessLog,
};
use crate::logs::{LogIterator, ProcessLogType};
use crate::output;
use crate::process_alive::filter_alive_processes;

const POLL_INTERVAL: u64 = 200;
const LOG_COUNT_SEARCH_LIMIT: i64 = 1000;
/// Lines of the latest run shown when waiting fails.
const RECENT_LOG_LINES: i64 = 20;
/// How often (in polls) to re-check that the service is still running.
const LIVENESS_CHECK_EVERY: u32 = 5;

/// Result of [`handle_wait_for_log`]. The TS `message` field on failure is never
/// read by the caller, so a bare success flag is sufficient.
pub struct WaitForLogResult {
    pub success: bool,
}

fn content_contains(content: &Option<String>, message: &str) -> bool {
    content.as_deref().is_some_and(|c| c.contains(message))
}

fn is_type(log: &ProcessLog, log_type: ProcessLogType) -> bool {
    log.log_type == log_type.as_i64()
}

/// A log row that ends a run: the process exited or never started.
fn ends_run(log: &ProcessLog) -> bool {
    is_type(log, ProcessLogType::ProcessExited) || is_type(log, ProcessLogType::ProcessStartFailed)
}

/// The "not running" phrase for one service, several, or the whole project.
fn describe_services(command_names: &[String]) -> String {
    match command_names.len() {
        0 => "No service in this project is running".to_string(),
        1 => format!("Service '{}' is not running", command_names[0]),
        _ => format!("Services '{}' are not running", command_names.join(", ")),
    }
}

/// Whether any of the named services (every service when `command_names` is
/// empty) has a live process in the project.
fn is_any_running(conn: &Connection, project_dir: &str, command_names: &[String]) -> bool {
    let entries = find_running_processes_by_project_dir(conn, project_dir)
        .and_then(|entries| filter_alive_processes(conn, entries))
        .unwrap_or_default();
    entries
        .iter()
        .any(|e| command_names.is_empty() || command_names.contains(&e.command_name))
}

/// Print the last [`RECENT_LOG_LINES`] lines of the latest run, then a pointer
/// to `candle logs` for the rest.
fn print_recent_logs(conn: &Connection, project_dir: &str, command_names: &[String]) {
    let tail = get_log_tail(
        conn,
        &LogSearchOptions {
            project_dir: Some(project_dir.to_string()),
            command_names: command_names.to_vec(),
            ..Default::default()
        },
        RECENT_LOG_LINES,
    )
    .unwrap_or_default();

    let label = command_names.join(", ");
    let header = if tail.truncated {
        format!("Last {RECENT_LOG_LINES} lines of the latest run of '{label}':")
    } else {
        format!("Logs from the latest run of '{label}':")
    };
    output::out(&header);
    let options = ConsoleLogOptions {
        format: Some(OutputFormat::Pretty),
        prefix: None,
        enable_app_name_prefix: command_names.len() != 1,
    };
    for log in &tail.logs {
        console_log_row(log, &options);
    }
    let logs_command = if command_names.len() == 1 {
        format!("candle logs {}", command_names[0])
    } else {
        "candle logs".to_string()
    };
    output::out(&format!("Run '{logs_command}' to see more."));
}

fn fail_not_running(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    message: &str,
    has_run: bool,
) -> WaitForLogResult {
    output::error(&format!(
        "{} and message \"{message}\" was not found.",
        describe_services(command_names)
    ));
    if has_run {
        print_recent_logs(conn, project_dir, command_names);
    }
    WaitForLogResult { success: false }
}

/// Wait for `message` to appear in the logs of the given service(s).
pub fn handle_wait_for_log(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    message: &str,
    timeout_ms: u64,
) -> WaitForLogResult {
    // Recent rows of each service's latest run. Every row carries its run, so
    // this works even when the launch itself is older than the search window.
    let mut log_filter = LatestRunFilter::new(None);
    let _ = log_filter.seed_latest_runs(conn, project_dir, command_names);
    let mut log_iterator = LogIterator::with_limit(
        project_dir.to_string(),
        command_names.to_vec(),
        Some(LOG_COUNT_SEARCH_LIMIT),
    );
    let initial_logs =
        log_filter.filter(&log_iterator.get_next_logs(conn, None).unwrap_or_default());

    // Look for the message in existing logs. A run that has already finished
    // still counts if it printed the message.
    for log_event in &initial_logs {
        if content_contains(&log_event.content, message) {
            output::out(&format!("Found message \"{message}\" in existing logs."));
            return WaitForLogResult { success: true };
        }
    }

    let has_run = !latest_run_ids(conn, project_dir, command_names)
        .unwrap_or_default()
        .is_empty();
    let latest_run_lifecycle = get_process_logs(
        conn,
        &LogSearchOptions {
            project_dir: Some(project_dir.to_string()),
            command_names: command_names.to_vec(),
            log_types: vec![
                ProcessLogType::ProcessStarted.as_i64(),
                ProcessLogType::ProcessStartFailed.as_i64(),
                ProcessLogType::ProcessExited.as_i64(),
            ],
            latest_launch_only: true,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let running = is_any_running(conn, project_dir, command_names);

    // Never launched and nothing running: nothing is going to write the message.
    if !has_run && !running {
        return fail_not_running(conn, project_dir, command_names, message, false);
    }
    // The latest run already ended (and, with several services, none of them
    // is still running).
    if latest_run_lifecycle.iter().any(ends_run) && !running {
        return fail_not_running(conn, project_dir, command_names, message, true);
    }

    // Once the monitor has reported the start, the process row must exist for
    // as long as the service runs. Before that, a launch is still in progress.
    let mut start_reported = !has_run
        || latest_run_lifecycle
            .iter()
            .any(|log| is_type(log, ProcessLogType::ProcessStarted));

    // Poll for logs until we find the message or timeout
    let time_started = Instant::now();
    let mut polls: u32 = 0;
    loop {
        if start_reported
            && polls.is_multiple_of(LIVENESS_CHECK_EVERY)
            && !is_any_running(conn, project_dir, command_names)
        {
            return fail_not_running(conn, project_dir, command_names, message, true);
        }

        if time_started.elapsed().as_millis() > timeout_ms as u128 {
            output::error(&format!(
                "Timed out after {timeout_ms}ms and message \"{message}\" not found."
            ));
            print_recent_logs(conn, project_dir, command_names);
            return WaitForLogResult { success: false };
        }

        let raw_logs = log_iterator.get_next_logs(conn, None).unwrap_or_default();
        let logs = log_filter.filter(&raw_logs);
        for log in &logs {
            if content_contains(&log.content, message) {
                output::out(&format!("Found message \"{message}\" in logs."));
                return WaitForLogResult { success: true };
            }

            if is_type(log, ProcessLogType::ProcessStarted) {
                start_reported = true;
            }

            if ends_run(log) {
                output::error(&format!(
                    "Process exited before finding message \"{message}\""
                ));
                print_recent_logs(conn, project_dir, command_names);
                return WaitForLogResult { success: false };
            }
        }

        polls = polls.wrapping_add(1);
        sleep(Duration::from_millis(POLL_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};
    use crate::logs::process_logs::save_process_log;

    #[test]
    fn found_in_existing_logs() {
        let dir = temp_db_dir("wait-for-log-found");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(
            &conn,
            "echo",
            "/proj",
            ProcessLogType::ProcessStartInitiated,
            None,
        )
        .unwrap();
        save_process_log(
            &conn,
            "echo",
            "/proj",
            ProcessLogType::Stdout,
            Some("hello world"),
        )
        .unwrap();

        let (result, captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "hello", 30000)
        });

        assert!(result.success);
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "Found message \"hello\" in existing logs."));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_logs_is_failure() {
        let dir = temp_db_dir("wait-for-log-empty");
        let conn = get_database(Some(&dir)).unwrap();

        let started = Instant::now();
        let (result, captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "hello", 30000)
        });

        assert!(!result.success);
        // Fails at once instead of waiting out the 30s timeout.
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(captured.stdout.is_empty());
        assert_eq!(
            captured.stderr,
            vec![
                "Error: Service 'echo' is not running and message \"hello\" was not found."
                    .to_string()
            ]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn started_but_message_absent_times_out() {
        let dir = temp_db_dir("wait-for-log-timeout");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(
            &conn,
            "echo",
            "/proj",
            ProcessLogType::ProcessStartInitiated,
            None,
        )
        .unwrap();

        let timeout_ms = 200;
        let (result, captured) = output::capture(|| {
            handle_wait_for_log(
                &conn,
                "/proj",
                &["echo".to_string()],
                "never-appears",
                timeout_ms,
            )
        });

        assert!(!result.success);
        assert!(captured.stderr.iter().any(|l| l
            == &format!(
                "Error: Timed out after {timeout_ms}ms and message \"never-appears\" not found."
            )));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn save(conn: &Connection, log_type: ProcessLogType, content: Option<&str>) {
        save_process_log(conn, "echo", "/proj", log_type, content).unwrap();
    }

    #[test]
    fn exited_latest_run_fails_at_once() {
        let dir = temp_db_dir("wait-for-log-exited");
        let conn = get_database(Some(&dir)).unwrap();

        save(&conn, ProcessLogType::ProcessStartInitiated, None);
        save(&conn, ProcessLogType::Stdout, Some("booting"));
        save(&conn, ProcessLogType::ProcessStarted, None);
        save(
            &conn,
            ProcessLogType::ProcessExited,
            Some("Process exited with code 1"),
        );

        let started = Instant::now();
        let (result, captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "ready", 30000)
        });

        assert!(!result.success);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            captured.stderr,
            vec![
                "Error: Service 'echo' is not running and message \"ready\" was not found."
                    .to_string()
            ]
        );
        assert!(captured.stdout.contains(&"booting".to_string()));
        assert_eq!(
            captured.stdout.last().unwrap(),
            "Run 'candle logs echo' to see more."
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn started_without_live_process_fails_at_once() {
        let dir = temp_db_dir("wait-for-log-no-process");
        let conn = get_database(Some(&dir)).unwrap();

        // The monitor reported the start, but no process row is alive.
        save(&conn, ProcessLogType::ProcessStartInitiated, None);
        save(&conn, ProcessLogType::ProcessStarted, None);

        let started = Instant::now();
        let (result, captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "ready", 30000)
        });

        assert!(!result.success);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(captured.stderr[0].contains("Service 'echo' is not running"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timeout_shows_only_the_tail_of_the_latest_run() {
        let dir = temp_db_dir("wait-for-log-tail");
        let conn = get_database(Some(&dir)).unwrap();

        // A previous run, then a launch that is still in progress.
        save(&conn, ProcessLogType::ProcessStartInitiated, None);
        save(&conn, ProcessLogType::Stdout, Some("old run line"));
        save(
            &conn,
            ProcessLogType::ProcessExited,
            Some("Process was stopped"),
        );
        save(&conn, ProcessLogType::ProcessStartInitiated, None);
        for i in 0..50 {
            save(&conn, ProcessLogType::Stdout, Some(&format!("new {i}")));
        }

        let (result, captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "never", 200)
        });

        assert!(!result.success);
        assert!(captured.stderr[0].starts_with("Error: Timed out"));
        let out = &captured.stdout;
        assert_eq!(out[0], "Last 20 lines of the latest run of 'echo':");
        let lines: Vec<&String> = out.iter().filter(|l| l.starts_with("new ")).collect();
        assert_eq!(lines.len(), 20);
        assert_eq!(lines[0], "new 30");
        assert!(!out.iter().any(|l| l.contains("old run line")));
        assert!(!out.iter().any(|l| l.contains("Process was stopped")));
        assert_eq!(out.last().unwrap(), "Run 'candle logs echo' to see more.");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
