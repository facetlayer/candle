//! `logs` command handler.
//!
//! Ported from `src/logs-command.ts`. Fetches stored process output for the
//! given command(s) (or all commands in the project when none are named),
//! filters to the most recent launch (showing logs from a previous launch when
//! there is no launch marker), and renders each row through the output sink.

use rusqlite::Connection;

use crate::log_filters::{LatestExecutionLogFilter, ShowPastLogsBehavior};
use crate::logs::console_log::{console_log_row, ConsoleLogOptions, OutputFormat};
use crate::logs::process_logs::{get_process_logs_with_eviction_info, LogSearchOptions};
use crate::output;

/// Display logs for the given command(s) in the project.
///
/// When `command_names` is empty (or has more than one entry) the output runs in
/// "blended" mode, which prefixes each line with `[<command>] `.
pub fn handle_logs_command(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    limit: i64,
    start_at_id: Option<i64>,
) {
    let is_blended_mode = command_names.len() != 1;

    // Get logs and filter to only show logs from the most recent process run.
    let result = get_process_logs_with_eviction_info(
        conn,
        &LogSearchOptions {
            project_dir: Some(project_dir.to_string()),
            command_names: command_names.to_vec(),
            limit: Some(limit),
            after_log_id: start_at_id,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let all_logs = result.logs;
    let logs_were_evicted = result.logs_were_evicted;

    let mut log_filter =
        LatestExecutionLogFilter::new(ShowPastLogsBehavior::ShowLogsFromPreviousLaunch, None);
    log_filter.check_latest_launch_status(&all_logs);

    let logs = log_filter.filter(&all_logs);

    if logs.is_empty() {
        if command_names.len() == 1 {
            output::out(&format!(
                "No logs found for command '{}' in project '{project_dir}'.",
                command_names[0]
            ));
        } else {
            output::out(&format!(
                "No logs found for commands in project '{project_dir}'."
            ));
        }
        return;
    }

    // Show eviction indicator if older logs were removed.
    if logs_were_evicted {
        output::out("-- older logs have been removed --");
    }

    // Display logs with prefix in blended mode.
    for log in &logs {
        console_log_row(
            log,
            &ConsoleLogOptions {
                format: Some(OutputFormat::Pretty),
                prefix: None,
                enable_app_name_prefix: is_blended_mode,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};
    use crate::logs::process_logs::save_process_log;
    use crate::logs::ProcessLogType;

    #[test]
    fn empty_db_single_name() {
        let dir = temp_db_dir("logs-empty-single");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &["svc".to_string()], 100, None);
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "No logs found for command 'svc' in project '/proj'."));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_db_zero_names() {
        let dir = temp_db_dir("logs-empty-zero");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &[], 100, None);
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "No logs found for commands in project '/proj'."));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_command_shows_stdout_rows() {
        let dir = temp_db_dir("logs-single-rows");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "svc", "/proj", ProcessLogType::ProcessStartInitiated, None)
            .unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("alpha")).unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("beta")).unwrap();

        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &["svc".to_string()], 100, None);
        });

        // Start lines are hidden; no eviction line.
        assert_eq!(captured.stdout, vec!["alpha".to_string(), "beta".to_string()]);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blended_mode_prefixes_command_name() {
        let dir = temp_db_dir("logs-blended");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "a", "/proj", ProcessLogType::Stdout, Some("x")).unwrap();
        save_process_log(&conn, "b", "/proj", ProcessLogType::Stdout, Some("y")).unwrap();

        let (_, captured) = output::capture(|| {
            handle_logs_command(
                &conn,
                "/proj",
                &["a".to_string(), "b".to_string()],
                100,
                None,
            );
        });

        assert!(captured.stdout.iter().any(|l| l == "[a] x"));
        assert!(captured.stdout.iter().any(|l| l == "[b] y"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
