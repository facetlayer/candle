//! `logs` command handler.
//!
//! Fetches stored process output for the given service(s) (or all services in
//! the project when none are named), filters to the most recent launch
//! (showing logs from a previous launch when there is no launch marker), and
//! renders each row through the output sink, either as text or (with `--json`)
//! as a JSON array that carries each row's ID.

use rusqlite::Connection;
use serde_json::json;

use crate::logs::console_log::{console_log_row, ConsoleLogOptions, OutputFormat};
use crate::logs::process_logs::{
    command_names_with_logs, get_log_tail, LogSearchOptions, LogTail, ProcessLog,
};
use crate::logs::ProcessLogType;
use crate::output;

/// How the CLI's truncation hint tells the reader to ask for more lines.
pub const CLI_MORE_HINT: &str = "use --count to see more";

/// Options for [`handle_logs_command`].
#[derive(Debug, Clone)]
pub struct LogsCommandOptions {
    /// Lines to show per service.
    pub limit: i64,
    /// Only rows with an ID greater than this.
    pub start_at_id: Option<i64>,
    /// Print a JSON array instead of text.
    pub json: bool,
    /// Tail of the truncation hint, e.g. [`CLI_MORE_HINT`]. MCP callers pass a
    /// hint that names the tool's `limit` parameter instead.
    pub more_hint: String,
}

impl LogsCommandOptions {
    /// Plain-text CLI output with the given limit.
    pub fn cli(limit: i64) -> Self {
        LogsCommandOptions {
            limit,
            start_at_id: None,
            json: false,
            more_hint: CLI_MORE_HINT.to_string(),
        }
    }
}

/// The newest `limit` printable rows of each named command's latest run.
fn fetch_latest_run_tail(
    conn: &Connection,
    project_dir: &str,
    command_names: Vec<String>,
    options: &LogsCommandOptions,
) -> LogTail {
    get_log_tail(
        conn,
        &LogSearchOptions {
            project_dir: Some(project_dir.to_string()),
            command_names,
            after_log_id: options.start_at_id,
            ..Default::default()
        },
        options.limit,
    )
    .unwrap_or_default()
}

/// The name used for a row's `type` in `--json` output, or None for rows that
/// `logs` never prints (the launch markers).
fn json_log_type(log_type: i64) -> Option<&'static str> {
    match ProcessLogType::try_from(log_type) {
        Ok(ProcessLogType::Stdout) => Some("stdout"),
        Ok(ProcessLogType::Stderr) => Some("stderr"),
        Ok(ProcessLogType::ProcessStartFailed) => Some("start_failed"),
        Ok(ProcessLogType::ProcessExited) => Some("exited"),
        _ => None,
    }
}

fn logs_to_json(logs: &[ProcessLog]) -> String {
    let entries: Vec<serde_json::Value> = logs
        .iter()
        .filter_map(|log| {
            let log_type = json_log_type(log.log_type)?;
            Some(json!({
                "id": log.id,
                "service": log.command_name,
                "type": log_type,
                "content": log.content.as_deref().unwrap_or_default(),
                "timestamp": log.timestamp,
            }))
        })
        .collect();
    serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
}

fn lines_phrase(limit: i64) -> String {
    if limit == 1 {
        "line".to_string()
    } else {
        format!("{limit} lines")
    }
}

/// Display logs for the given service(s) in the project.
///
/// When `command_names` is empty (or has more than one entry) the output runs in
/// "blended" mode: each line is prefixed with `[<service>] `, and the limit
/// applies to each service separately, so one chatty service can't push the
/// others out of the output.
pub fn handle_logs_command(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    options: &LogsCommandOptions,
) {
    let is_blended_mode = command_names.len() != 1;

    let mut logs: Vec<ProcessLog> = Vec::new();
    let mut truncated_services: Vec<String> = Vec::new();

    if is_blended_mode {
        let names = if command_names.is_empty() {
            command_names_with_logs(conn, project_dir, options.start_at_id).unwrap_or_default()
        } else {
            command_names.to_vec()
        };
        for name in names {
            let service = fetch_latest_run_tail(conn, project_dir, vec![name.clone()], options);
            if service.truncated && !service.logs.is_empty() {
                truncated_services.push(name);
            }
            logs.extend(service.logs);
        }
        logs.sort_by_key(|l| l.id);
    } else {
        let service = fetch_latest_run_tail(conn, project_dir, command_names.to_vec(), options);
        if service.truncated {
            truncated_services.push(command_names[0].clone());
        }
        logs = service.logs;
    }

    if options.json {
        output::out(&logs_to_json(&logs));
        return;
    }

    if logs.is_empty() {
        if command_names.len() == 1 {
            output::out(&format!(
                "No logs found for service '{}' in project '{project_dir}'.",
                command_names[0]
            ));
        } else {
            output::out(&format!(
                "No logs found for services in project '{project_dir}'."
            ));
        }
        return;
    }

    // Only when the limit cut off lines from this run; a previous run's lines
    // are hidden on purpose and aren't worth a hint.
    if !truncated_services.is_empty() {
        let what = lines_phrase(options.limit);
        let hint = if is_blended_mode {
            format!(
                "-- showing the last {what} per service ({} had more); {} --",
                truncated_services.join(", "),
                options.more_hint
            )
        } else {
            format!("-- showing the last {what}; {} --", options.more_hint)
        };
        output::out(&hint);
    }

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
            handle_logs_command(
                &conn,
                "/proj",
                &["svc".to_string()],
                &LogsCommandOptions::cli(100),
            );
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "No logs found for service 'svc' in project '/proj'."));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_db_zero_names() {
        let dir = temp_db_dir("logs-empty-zero");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &[], &LogsCommandOptions::cli(100));
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "No logs found for services in project '/proj'."));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_command_shows_stdout_rows() {
        let dir = temp_db_dir("logs-single-rows");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(
            &conn,
            "svc",
            "/proj",
            ProcessLogType::ProcessStartInitiated,
            None,
        )
        .unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("alpha")).unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("beta")).unwrap();

        let (_, captured) = output::capture(|| {
            handle_logs_command(
                &conn,
                "/proj",
                &["svc".to_string()],
                &LogsCommandOptions::cli(100),
            );
        });

        // Start lines are hidden; no eviction line.
        assert_eq!(
            captured.stdout,
            vec!["alpha".to_string(), "beta".to_string()]
        );

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
                &LogsCommandOptions::cli(100),
            );
        });

        assert!(captured.stdout.iter().any(|l| l == "[a] x"));
        assert!(captured.stdout.iter().any(|l| l == "[b] y"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blended_mode_limits_each_service_separately() {
        let dir = temp_db_dir("logs-blended-per-service");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "quiet", "/proj", ProcessLogType::Stdout, Some("q1")).unwrap();
        for i in 0..10 {
            save_process_log(
                &conn,
                "chatty",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("c{i}")),
            )
            .unwrap();
        }

        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &[], &LogsCommandOptions::cli(3));
        });

        assert_eq!(
            captured.stdout,
            vec![
                "-- showing the last 3 lines per service (chatty had more); use --count to see more --"
                    .to_string(),
                "[quiet] q1".to_string(),
                "[chatty] c7".to_string(),
                "[chatty] c8".to_string(),
                "[chatty] c9".to_string(),
            ]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_more_hint_is_used() {
        let dir = temp_db_dir("logs-more-hint");
        let conn = get_database(Some(&dir)).unwrap();

        for i in 0..3 {
            save_process_log(
                &conn,
                "svc",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("l{i}")),
            )
            .unwrap();
        }
        let options = LogsCommandOptions {
            more_hint: "raise `limit` to see more".to_string(),
            ..LogsCommandOptions::cli(1)
        };
        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &["svc".to_string()], &options);
        });

        assert_eq!(
            captured.stdout,
            vec![
                "-- showing the last line; raise `limit` to see more --".to_string(),
                "l2".to_string()
            ]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_output_includes_ids_and_skips_markers() {
        let dir = temp_db_dir("logs-json");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(
            &conn,
            "svc",
            "/proj",
            ProcessLogType::ProcessStartInitiated,
            None,
        )
        .unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("alpha")).unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stderr, Some("beta")).unwrap();

        let options = LogsCommandOptions {
            json: true,
            ..LogsCommandOptions::cli(100)
        };
        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &["svc".to_string()], &options);
        });

        let parsed: serde_json::Value = serde_json::from_str(&captured.stdout.join("\n")).unwrap();
        let entries = parsed.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["id"], 2);
        assert_eq!(entries[0]["service"], "svc");
        assert_eq!(entries[0]["type"], "stdout");
        assert_eq!(entries[0]["content"], "alpha");
        assert_eq!(entries[1]["type"], "stderr");

        // --start-at with an ID from the JSON output returns only later rows.
        let options = LogsCommandOptions {
            json: true,
            start_at_id: Some(2),
            ..LogsCommandOptions::cli(100)
        };
        let (_, captured) = output::capture(|| {
            handle_logs_command(&conn, "/proj", &["svc".to_string()], &options);
        });
        let parsed: serde_json::Value = serde_json::from_str(&captured.stdout.join("\n")).unwrap();
        let entries = parsed.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["content"], "beta");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
