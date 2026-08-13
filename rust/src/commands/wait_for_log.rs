//! `wait-for-log` command handler.
//!
//! Ported from `src/wait-for-log-command.ts`. Polls the `process_output` table
//! for a given substring, scoped to the most recent launch of the named
//! command(s), until the message appears, the process exits, or a timeout is hit.

use std::thread::sleep;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::log_filters::{LatestExecutionLogFilter, ShowPastLogsBehavior};
use crate::logs::console_log::{console_log_row, ConsoleLogOptions};
use crate::logs::process_logs::{get_process_logs, LogSearchOptions};
use crate::logs::{LogIterator, ProcessLogType};
use crate::output;

const POLL_INTERVAL: u64 = 200;
const LOG_COUNT_SEARCH_LIMIT: i64 = 1000;

/// Result of [`handle_wait_for_log`]. The TS `message` field on failure is never
/// read by the caller, so a bare success flag is sufficient.
pub struct WaitForLogResult {
    pub success: bool,
}

fn content_contains(content: &Option<String>, message: &str) -> bool {
    content.as_deref().is_some_and(|c| c.contains(message))
}

fn print_recent_logs(conn: &Connection, project_dir: &str, command_names: &[String]) {
    output::out(&format!("Recent logs for '{}':", command_names.join(", ")));
    let mut filter = LatestExecutionLogFilter::new(ShowPastLogsBehavior::OnlyShowAfterRecentLaunch, None);
    let all_logs = get_process_logs(
        conn,
        &LogSearchOptions {
            project_dir: Some(project_dir.to_string()),
            command_names: command_names.to_vec(),
            limit: Some(100),
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let recent_logs = filter.filter(&all_logs);
    for log in &recent_logs {
        console_log_row(log, &ConsoleLogOptions::pretty());
    }
}

/// Wait for `message` to appear in the logs of the given command(s).
pub fn handle_wait_for_log(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    message: &str,
    timeout_ms: u64,
) -> WaitForLogResult {
    // Get recent logs
    let mut log_iterator = LogIterator::with_limit(
        project_dir.to_string(),
        command_names.to_vec(),
        Some(LOG_COUNT_SEARCH_LIMIT),
    );
    let all_initial_logs = log_iterator.get_next_logs(conn, None).unwrap_or_default();

    // Use filter to only show logs from the most recent process run
    let mut log_filter =
        LatestExecutionLogFilter::new(ShowPastLogsBehavior::OnlyShowAfterRecentLaunch, None);
    log_filter.check_latest_launch_status(&all_initial_logs);
    let initial_logs = log_filter.filter(&all_initial_logs);

    // Check if we have any logs at all for this process
    if initial_logs.is_empty() {
        return WaitForLogResult { success: false };
    }

    // Check if we have any process_has_started events
    let has_process_started = initial_logs
        .iter()
        .any(|log| log.log_type == ProcessLogType::ProcessStartInitiated.as_i64());

    if !has_process_started {
        output::err("Process has not started yet");
        return WaitForLogResult { success: false };
    }

    // Look for the message in existing logs
    for log_event in &initial_logs {
        if content_contains(&log_event.content, message) {
            output::out(&format!("Found message \"{message}\" in existing logs."));
            return WaitForLogResult { success: true };
        }
    }

    // Poll for logs until we find the message or timeout
    let time_started = Instant::now();
    loop {
        if time_started.elapsed().as_millis() > timeout_ms as u128 {
            output::out(&format!(
                "wait-for-log failed: Timed out after {timeout_ms}ms and message \"{message}\" not found."
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

            if log.log_type == ProcessLogType::ProcessExited.as_i64() {
                output::out(&format!(
                    "wait-for-log failed: Process exited before finding message \"{message}\""
                ));
                print_recent_logs(conn, project_dir, command_names);
                return WaitForLogResult { success: false };
            }
        }

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

        save_process_log(&conn, "echo", "/proj", ProcessLogType::ProcessStartInitiated, None)
            .unwrap();
        save_process_log(&conn, "echo", "/proj", ProcessLogType::Stdout, Some("hello world"))
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

        let (result, _captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "hello", 30000)
        });

        assert!(!result.success);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn started_but_message_absent_times_out() {
        let dir = temp_db_dir("wait-for-log-timeout");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "echo", "/proj", ProcessLogType::ProcessStartInitiated, None)
            .unwrap();

        let timeout_ms = 200;
        let (result, captured) = output::capture(|| {
            handle_wait_for_log(&conn, "/proj", &["echo".to_string()], "never-appears", timeout_ms)
        });

        assert!(!result.success);
        assert!(captured.stdout.iter().any(|l| l
            == &format!(
                "wait-for-log failed: Timed out after {timeout_ms}ms and message \"never-appears\" not found."
            )));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
