//! Structured service listings and text/JSON rendering.
//! RUNNING requires an unmarked row with a live service or monitor.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::config::{find_config_file, find_service_by_name, CandleSetupConfig, ServiceConfig};
use crate::db::process_table::{
    find_all_processes, find_running_processes_by_project_dir, ProcessEntry,
};
use crate::dirs::resolve_launch_dir;
use crate::errors::CandleError;
use crate::logs::log_type::{KILLED_BY_SIGNAL, STOPPED_WHILE_STARTING_MESSAGE};
use crate::logs::ProcessLogType;
use crate::process_alive::filter_alive_processes;

/// Listing row; field order and camelCase names define the JSON shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListProcess {
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub command: String,
    #[serde(rename = "workingDir")]
    pub working_dir: String,
    /// Project key for --project-dir, distinct from the service working directory.
    #[serde(rename = "projectDir")]
    pub project_dir: String,
    pub uptime: String,
    /// The service's PID, or `None` (JSON `null`) when it is not running.
    pub pid: Option<i64>,
    pub status: String,
    /// Running command differs from config; false for stopped services.
    #[serde(rename = "configChanged")]
    pub config_changed: bool,
    /// Non-zero exit code for EXITED status; None otherwise, including FAILED.
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i64>,
}

/// Result of [`handle_list`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOutput {
    pub processes: Vec<ListProcess>,
}

const STATUS_RUNNING: &str = "RUNNING";
const STATUS_NOT_RUNNING: &str = "not running";
/// Status for a stopped service whose latest run failed to start without an
/// exit code (spawn failure, missing root, signal during startup).
const STATUS_FAILED: &str = "FAILED";

/// Status for a stopped service whose latest run exited with a non-zero code.
fn exited_status(code: i64) -> String {
    format!("EXITED ({code})")
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Whether a running process's stored command differs from its config entry.
/// Compares `shell`, and `root` with empty/null/None normalized to "unset".
pub fn has_config_drift(entry: &ProcessEntry, service: Option<&ServiceConfig>) -> bool {
    let service = match service {
        Some(s) => s,
        None => return false,
    };

    if entry.shell.as_deref() != Some(service.shell.as_str()) {
        return true;
    }

    let db_root = entry.root.as_deref().filter(|s| !s.is_empty());
    let config_root = service.root.as_deref().filter(|s| !s.is_empty());
    db_root != config_root
}

/// The shell string to report for a running process: prefer the shell recorded
/// on the process row, fall back to the configured service's shell, then "".
fn resolve_shell(entry: &ProcessEntry, service: Option<&ServiceConfig>) -> String {
    entry
        .shell
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| service.map(|s| s.shell.clone()))
        .unwrap_or_default()
}

/// Format uptime with non-zero units, or 0s.
pub fn format_uptime(milliseconds: i64) -> String {
    let total_seconds = (milliseconds / 1000).max(0);
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs}s"));
    }

    parts.join(" ")
}

/// A running process's uptime, formatted with [`format_uptime`].
pub fn format_entry_uptime(entry: &ProcessEntry) -> String {
    format_uptime(now_millis() - entry.start_time * 1000)
}

fn running_row(
    service_name: &str,
    command: &str,
    working_dir: &str,
    project_dir: &str,
    start_time: i64,
    pid: i64,
    config_changed: bool,
) -> ListProcess {
    ListProcess {
        service_name: service_name.to_string(),
        command: command.to_string(),
        working_dir: working_dir.to_string(),
        project_dir: project_dir.to_string(),
        uptime: format_uptime(now_millis() - start_time * 1000),
        pid: Some(pid),
        status: STATUS_RUNNING.to_string(),
        config_changed,
        exit_code: None,
    }
}

/// Parse the exit code out of a lifecycle log line written by the monitor:
/// `Process exited with code N` or `Process failed to start: exited with code N`.
fn parse_exit_code(content: &str) -> Option<i64> {
    let (_, code) = content.rsplit_once("exited with code ")?;
    code.trim().parse().ok()
}

/// How a stopped service's latest run ended, as far as `ps` / `list` care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LatestRun {
    /// Still going, stopped deliberately, exited cleanly, or never ran.
    Unremarkable,
    /// Non-zero exit: EXITED (<code>).
    Exited(i64),
    /// Spawn failure or unintended signal: FAILED.
    Failed,
}

/// Classify the latest run by its newest lifecycle row; ignore late older rows.
pub(crate) fn latest_run(
    conn: &Connection,
    project_dir: &str,
    command_name: &str,
) -> Result<LatestRun, CandleError> {
    let row: Option<(i64, Option<String>)> = conn
        .query_row(
            &format!(
                "select l.log_type, l.content from services s join log_lines l on l.service_id = s.id \
                 where s.project_dir = ?1 and s.command_name = ?2 and l.log_type in (?3, ?4, ?5, ?6) \
                 and l.run_id is {} order by l.id desc limit 1",
                crate::db::latest_run_of("s.id")
            ),
            rusqlite::params![
                project_dir,
                command_name,
                ProcessLogType::ProcessStartInitiated.as_i64(),
                ProcessLogType::ProcessStartFailed.as_i64(),
                ProcessLogType::ProcessStarted.as_i64(),
                ProcessLogType::ProcessExited.as_i64(),
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let Some((log_type, content)) = row else {
        return Ok(LatestRun::Unremarkable);
    };
    let content = content.unwrap_or_default();
    let nonzero_code = parse_exit_code(&content).filter(|code| *code != 0);

    if log_type == ProcessLogType::ProcessExited.as_i64() {
        return Ok(match nonzero_code {
            Some(code) => LatestRun::Exited(code),
            None if content.contains(KILLED_BY_SIGNAL) => LatestRun::Failed,
            None => LatestRun::Unremarkable,
        });
    }
    if log_type == ProcessLogType::ProcessStartFailed.as_i64() {
        return Ok(match nonzero_code {
            Some(code) => LatestRun::Exited(code),
            // Candle itself stopped it mid-start (kill / restart): not a failure.
            None if content == STOPPED_WHILE_STARTING_MESSAGE => LatestRun::Unremarkable,
            None => LatestRun::Failed,
        });
    }
    Ok(LatestRun::Unremarkable)
}

/// List live processes system-wide, or configured services in file order
/// followed by unconfigured running services in the project.
pub fn handle_list(
    conn: &Connection,
    cwd: &Path,
    show_all: bool,
) -> Result<ListOutput, CandleError> {
    if show_all {
        let entries = filter_alive_processes(conn, find_all_processes(conn)?)?;
        let processes = entries
            .into_iter()
            .map(|entry| {
                running_row(
                    &entry.command_name,
                    &resolve_shell(&entry, None),
                    &resolve_launch_dir(&entry.project_dir, entry.root.as_deref()),
                    &entry.project_dir,
                    entry.start_time,
                    entry.pid,
                    false,
                )
            })
            .collect();
        return Ok(ListOutput { processes });
    }

    let found = find_config_file(cwd)?;
    let config: CandleSetupConfig = found.config;
    let project_dir = found.project_dir.display().to_string();

    let running = filter_alive_processes(
        conn,
        find_running_processes_by_project_dir(conn, &project_dir)?,
    )?;

    let mut processes: Vec<ListProcess> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for service in &config.services {
        seen.push(service.name.as_str());
        let running_process = running.iter().find(|p| p.command_name == service.name);

        match running_process {
            Some(entry) => processes.push(running_row(
                &service.name,
                &resolve_shell(entry, Some(service)),
                &resolve_launch_dir(
                    &project_dir,
                    entry.root.as_deref().or(service.root.as_deref()),
                ),
                &project_dir,
                entry.start_time,
                entry.pid,
                has_config_drift(entry, Some(service)),
            )),
            None => {
                let (status, exit_code) = match latest_run(conn, &project_dir, &service.name)? {
                    LatestRun::Exited(code) => (exited_status(code), Some(code)),
                    LatestRun::Failed => (STATUS_FAILED.to_string(), None),
                    LatestRun::Unremarkable => (STATUS_NOT_RUNNING.to_string(), None),
                };
                processes.push(ListProcess {
                    service_name: service.name.clone(),
                    command: service.shell.clone(),
                    working_dir: resolve_launch_dir(&project_dir, service.root.as_deref()),
                    project_dir: project_dir.clone(),
                    uptime: "-".to_string(),
                    pid: None,
                    status,
                    config_changed: false,
                    exit_code,
                })
            }
        }
    }

    for entry in &running {
        if seen.contains(&entry.command_name.as_str()) {
            continue;
        }
        let config_service = find_service_by_name(&config, &entry.command_name);
        processes.push(running_row(
            &entry.command_name,
            &resolve_shell(entry, config_service),
            &resolve_launch_dir(&project_dir, entry.root.as_deref()),
            &project_dir,
            entry.start_time,
            entry.pid,
            has_config_drift(entry, config_service),
        ));
    }

    Ok(ListOutput { processes })
}

/// Filter names without changing order; empty names leave the listing intact.
/// Reject unknown names with a project-specific or system-wide error.
pub fn filter_by_service_names(
    output: ListOutput,
    names: &[String],
    project_dir: Option<&str>,
) -> Result<ListOutput, CandleError> {
    if names.is_empty() {
        return Ok(output);
    }

    for name in names {
        if !output.processes.iter().any(|p| &p.service_name == name) {
            return Err(match project_dir {
                Some(dir) => CandleError::unknown_service(name, dir),
                None => CandleError::UsageError(format!("No running service named '{name}'")),
            });
        }
    }

    let processes = output
        .processes
        .into_iter()
        .filter(|p| names.contains(&p.service_name))
        .collect();
    Ok(ListOutput { processes })
}

/// Render full service details, omitting PID/uptime for stopped services.
pub fn format_list_detail(output: &ListOutput) -> String {
    if output.processes.is_empty() {
        return "No services configured.".to_string();
    }

    let mut entries: Vec<String> = Vec::new();
    for p in &output.processes {
        let mut status = p.status.clone();
        if p.config_changed {
            status.push_str(" [config changed]");
        }
        if p.status == STATUS_RUNNING {
            if let Some(pid) = p.pid {
                status.push_str(&format!(" - pid {pid}"));
            }
            if !p.uptime.is_empty() && p.uptime != "-" {
                status.push_str(&format!(" - uptime {}", p.uptime));
            }
        }
        entries.push(format!(
            "[{}]\n  status: {status}\n  command: {}\n  directory: {}",
            p.service_name, p.command, p.working_dir
        ));
    }

    entries.join("\n\n")
}

/// Serialize the process array for CLI/MCP JSON output.
pub fn list_output_to_json(output: &ListOutput) -> String {
    serde_json::to_string_pretty(&output.processes).unwrap_or_else(|_| "[]".to_string())
}

/// Render the full service table.
pub fn format_list_output(output: &ListOutput) -> String {
    format_table(output, true)
}

/// Render the compact NAME/STATUS/PID/UPTIME table for candle ps.
pub fn format_ps_output(output: &ListOutput) -> String {
    format_table(output, false)
}

fn format_table(output: &ListOutput, with_command_and_dir: bool) -> String {
    if output.processes.is_empty() {
        return "No services configured.".to_string();
    }

    let mut headers: Vec<&str> = vec!["NAME", "STATUS", "PID", "UPTIME"];
    if with_command_and_dir {
        headers.push("COMMAND");
        headers.push("DIRECTORY");
    }

    let rows: Vec<Vec<String>> = output
        .processes
        .iter()
        .map(|p| {
            let mut status = p.status.clone();
            if p.config_changed {
                status = format!("{status} [config changed]");
            }
            let mut cells = vec![
                p.service_name.clone(),
                status,
                p.pid.map_or_else(|| "-".to_string(), |pid| pid.to_string()),
                p.uptime.clone(),
            ];
            if with_command_and_dir {
                cells.push(p.command.clone());
                cells.push(p.working_dir.clone());
            }
            cells
        })
        .collect();

    let widths: Vec<usize> = (0..headers.len())
        .map(|i| {
            let header_len = headers[i].len();
            rows.iter()
                .map(|r| r[i].len())
                .max()
                .unwrap_or(0)
                .max(header_len)
        })
        .collect();

    let pad = |cell: &str, width: usize| -> String {
        let mut s = cell.to_string();
        while s.len() < width {
            s.push(' ');
        }
        s
    };

    let format_row = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| pad(c, widths[i]))
            .collect::<Vec<_>>()
            .join("  ")
    };

    let mut lines: Vec<String> = Vec::new();
    let header_cells: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    lines.push(format_row(&header_cells));
    lines.push(
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  "),
    );
    for row in &rows {
        lines.push(format_row(row));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_cases() {
        assert_eq!(format_uptime(0), "0s");
        assert_eq!(format_uptime(5_000), "5s");
        assert_eq!(format_uptime(185_000), "3m 5s");
        assert_eq!(format_uptime((86400 + 2 * 3600) * 1000), "1d 2h");
    }

    #[test]
    fn empty_output_message() {
        let out = ListOutput { processes: vec![] };
        assert_eq!(format_list_output(&out), "No services configured.");
    }

    #[test]
    fn header_order_and_config_changed() {
        let out = ListOutput {
            processes: vec![ListProcess {
                service_name: "echo".to_string(),
                command: "echo".to_string(),
                working_dir: "/proj".to_string(),
                project_dir: "/proj".to_string(),
                uptime: "5s".to_string(),
                pid: Some(42),
                status: "RUNNING".to_string(),
                config_changed: true,
                exit_code: None,
            }],
        };
        let text = format_list_output(&out);
        let header = text.lines().next().unwrap();
        let name = header.find("NAME").unwrap();
        let status = header.find("STATUS").unwrap();
        let pid = header.find("PID").unwrap();
        let uptime = header.find("UPTIME").unwrap();
        let command = header.find("COMMAND").unwrap();
        let directory = header.find("DIRECTORY").unwrap();
        assert!(
            name < status
                && status < pid
                && pid < uptime
                && uptime < command
                && command < directory
        );
        assert!(!text.contains("LAUNCH_ID"));
        assert!(!text.contains("WRAPPER_PID"));
        assert!(text.contains("[config changed]"));
        assert!(text.contains("42"));
    }

    fn sample() -> ListOutput {
        ListOutput {
            processes: vec![
                ListProcess {
                    service_name: "web".to_string(),
                    command: "npm run dev".to_string(),
                    working_dir: "/proj/web".to_string(),
                    project_dir: "/proj".to_string(),
                    uptime: "3m 5s".to_string(),
                    pid: Some(12345),
                    status: STATUS_RUNNING.to_string(),
                    config_changed: false,
                    exit_code: None,
                },
                ListProcess {
                    service_name: "api".to_string(),
                    command: "npm run api".to_string(),
                    working_dir: "/proj".to_string(),
                    project_dir: "/proj".to_string(),
                    uptime: "-".to_string(),
                    pid: None,
                    status: STATUS_NOT_RUNNING.to_string(),
                    config_changed: false,
                    exit_code: None,
                },
            ],
        }
    }

    #[test]
    fn detail_view_is_multiline_and_untruncated() {
        assert_eq!(
            format_list_detail(&sample()),
            "[web]\n  status: RUNNING - pid 12345 - uptime 3m 5s\n  command: npm run dev\n  directory: /proj/web\n\n[api]\n  status: not running\n  command: npm run api\n  directory: /proj"
        );
    }

    #[test]
    fn detail_view_marks_config_changed_and_handles_empty() {
        let mut out = sample();
        out.processes[0].config_changed = true;
        let text = format_list_detail(&out);
        assert!(text.starts_with("[web]\n  status: RUNNING [config changed] - pid 12345"));
        assert_eq!(
            format_list_detail(&ListOutput { processes: vec![] }),
            "No services configured."
        );
    }

    #[test]
    fn ps_table_omits_command_and_directory() {
        let text = format_ps_output(&sample());
        let header = text.lines().next().unwrap();
        assert!(!header.contains("COMMAND"));
        assert!(!header.contains("DIRECTORY"));
        assert!(!text.contains("npm run dev"));
        assert!(!text.contains("/proj"));
        let name = header.find("NAME").unwrap();
        let status = header.find("STATUS").unwrap();
        let pid = header.find("PID").unwrap();
        let uptime = header.find("UPTIME").unwrap();
        assert!(name < status && status < pid && pid < uptime);
        assert!(text.contains("12345"));
        assert_eq!(
            format_ps_output(&ListOutput { processes: vec![] }),
            "No services configured."
        );
    }

    #[test]
    fn name_filter_selects_and_rejects() {
        let filtered =
            filter_by_service_names(sample(), &["api".to_string()], Some("/proj")).unwrap();
        assert_eq!(filtered.processes.len(), 1);
        assert_eq!(filtered.processes[0].service_name, "api");

        assert_eq!(
            filter_by_service_names(sample(), &[], Some("/proj"))
                .unwrap()
                .processes
                .len(),
            2
        );

        let err =
            filter_by_service_names(sample(), &["nope".to_string()], Some("/proj")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "No service 'nope' configured for directory: /proj"
        );

        let err = filter_by_service_names(sample(), &["nope".to_string()], None).unwrap_err();
        assert_eq!(err.to_string(), "No running service named 'nope'");
    }

    #[test]
    fn command_field_carries_the_shell_string() {
        let entry = ProcessEntry {
            shell: Some("npm run dev".to_string()),
            ..blank_entry()
        };
        assert_eq!(resolve_shell(&entry, None), "npm run dev");

        let service = ServiceConfig {
            name: "web".to_string(),
            shell: "npm run fallback".to_string(),
            root: None,
            enable_stdin: None,
        };
        let no_shell = ProcessEntry {
            shell: None,
            ..blank_entry()
        };
        assert_eq!(resolve_shell(&no_shell, Some(&service)), "npm run fallback");

        assert_eq!(resolve_shell(&no_shell, None), "");

        let text = format_list_detail(&sample());
        assert!(text.contains("command: npm run dev"));
        assert!(!text.contains("command: web"));
    }

    fn blank_entry() -> ProcessEntry {
        ProcessEntry {
            id: 1,
            command_name: "web".to_string(),
            project_dir: "/proj".to_string(),
            pid: 1,
            log_collector_pid: None,
            start_time: 0,
            created_at: 0,
            killed_at: None,
            shell: None,
            root: None,
            run_id: None,
            transient: false,
            pid_identity: None,
            monitor_identity: None,
            leader_exited: false,
        }
    }

    #[test]
    fn json_shape_matches_node() {
        let running = ListProcess {
            service_name: "echo".to_string(),
            command: "echo".to_string(),
            working_dir: "/proj".to_string(),
            project_dir: "/proj".to_string(),
            uptime: "5s".to_string(),
            pid: Some(42),
            status: "RUNNING".to_string(),
            config_changed: false,
            exit_code: None,
        };
        let json = serde_json::to_string(&running).unwrap();
        assert_eq!(
            json,
            r#"{"serviceName":"echo","command":"echo","workingDir":"/proj","projectDir":"/proj","uptime":"5s","pid":42,"status":"RUNNING","configChanged":false,"exitCode":null}"#
        );

        let stopped = ListProcess {
            service_name: "web".to_string(),
            command: "web".to_string(),
            working_dir: "/proj".to_string(),
            project_dir: "/proj".to_string(),
            uptime: "-".to_string(),
            pid: None,
            status: "not running".to_string(),
            config_changed: false,
            exit_code: None,
        };
        let json = serde_json::to_string(&stopped).unwrap();
        assert_eq!(
            json,
            r#"{"serviceName":"web","command":"web","workingDir":"/proj","projectDir":"/proj","uptime":"-","pid":null,"status":"not running","configChanged":false,"exitCode":null}"#
        );

        let crashed = ListProcess {
            status: exited_status(1),
            exit_code: Some(1),
            ..stopped
        };
        let json = serde_json::to_string(&crashed).unwrap();
        assert!(json.contains(r#""status":"EXITED (1)","configChanged":false,"exitCode":1"#));
    }

    #[test]
    fn exit_code_parsing() {
        assert_eq!(parse_exit_code("Process exited with code 1"), Some(1));
        assert_eq!(
            parse_exit_code("Process failed to start: exited with code 127"),
            Some(127)
        );
        assert_eq!(parse_exit_code("Process was stopped"), None);
    }

    #[test]
    fn latest_run_from_logs() {
        use crate::db::{get_database, temp_db_dir};
        use crate::logs::process_logs::save_process_log;
        let dir = temp_db_dir("list-exit-code");
        let conn = get_database(Some(&dir)).unwrap();
        let log = |t: ProcessLogType, c: Option<&str>| {
            save_process_log(&conn, "svc", "/proj", t, c).unwrap();
        };
        let latest = || latest_run(&conn, "/proj", "svc").unwrap();

        assert_eq!(latest(), LatestRun::Unremarkable);

        log(ProcessLogType::ProcessStartInitiated, None);
        log(ProcessLogType::ProcessStarted, None);
        log(ProcessLogType::Stdout, Some("boom"));
        log(
            ProcessLogType::ProcessExited,
            Some("Process exited with code 3"),
        );
        assert_eq!(latest(), LatestRun::Exited(3));

        log(ProcessLogType::ProcessStartInitiated, None);
        assert_eq!(latest(), LatestRun::Unremarkable);
        log(ProcessLogType::ProcessStarted, None);
        log(ProcessLogType::ProcessExited, Some("Process was stopped"));
        assert_eq!(latest(), LatestRun::Unremarkable);

        log(ProcessLogType::ProcessStartInitiated, None);
        log(ProcessLogType::ProcessStarted, None);
        log(
            ProcessLogType::ProcessExited,
            Some(&crate::logs::log_type::killed_by_signal_message(11)),
        );
        assert_eq!(latest(), LatestRun::Failed);

        log(ProcessLogType::ProcessStartInitiated, None);
        log(
            ProcessLogType::ProcessStartFailed,
            Some("Process failed to start: exited with code 127"),
        );
        assert_eq!(latest(), LatestRun::Exited(127));

        for reason in [
            "Process failed to start: root directory does not exist: /proj/sub",
            "Process failed to start: could not run 'sh': boom",
            "Process failed to start: stopped by a signal",
            "Process failed to start: killed by signal 11",
        ] {
            log(ProcessLogType::ProcessStartInitiated, None);
            log(ProcessLogType::ProcessStartFailed, Some(reason));
            assert_eq!(latest(), LatestRun::Failed, "{reason}");
        }

        log(ProcessLogType::ProcessStartInitiated, None);
        log(
            ProcessLogType::ProcessStartFailed,
            Some(STOPPED_WHILE_STARTING_MESSAGE),
        );
        assert_eq!(latest(), LatestRun::Unremarkable);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_row_json_has_null_exit_code() {
        let failed = ListProcess {
            service_name: "web".to_string(),
            command: "web".to_string(),
            working_dir: "/proj".to_string(),
            project_dir: "/proj".to_string(),
            uptime: "-".to_string(),
            pid: None,
            status: STATUS_FAILED.to_string(),
            config_changed: false,
            exit_code: None,
        };
        let json = serde_json::to_string(&failed).unwrap();
        assert!(json.contains(r#""status":"FAILED","configChanged":false,"exitCode":null"#));
    }
}
