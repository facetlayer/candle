//! `list` / `list-all` command.
//!
//! Produces a structured listing of services and running processes, plus a
//! pretty-table formatter and the JSON shape the `--json` flag and MCP consume.
//!
//! RUNNING is determined by liveness: the `list` query is already restricted to
//! `killed_at is null`, and [`filter_alive_processes`] drops (and deletes) rows
//! whose PIDs are dead, so killed/stale entries never show as RUNNING.

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

/// One row in a `list` result. Serialized directly by `--json`, so field order
/// and (camelCase) names define the JSON output shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListProcess {
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub command: String,
    #[serde(rename = "workingDir")]
    pub working_dir: String,
    /// The project directory the service belongs to (where its `.candle.json`
    /// lives). Differs from `working_dir` when the service has a `root`; this is
    /// the value to pass as `--project-dir` to target the service.
    #[serde(rename = "projectDir")]
    pub project_dir: String,
    pub uptime: String,
    /// The service's PID, or `None` (JSON `null`) when it is not running.
    pub pid: Option<i64>,
    pub status: String,
    /// Whether the running process was launched with a different `shell` /
    /// `root` than the config now has. Always `false` for a stopped service.
    #[serde(rename = "configChanged")]
    pub config_changed: bool,
    /// Exit code of the service's latest run, when that run exited non-zero
    /// (status `EXITED (<code>)`). `None` (JSON `null`) otherwise, including
    /// for `FAILED`, which has no exit code.
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

/// Format a duration in milliseconds as `"1d 2h"`, `"3m 5s"`, `"0s"`, etc.
/// Only non-zero components are shown, and an all-zero duration renders as
/// `"0s"`.
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
enum LatestRun {
    /// Still going, stopped deliberately, exited cleanly, or never ran.
    Unremarkable,
    /// Ended with a non-zero exit code (a crash, or a start that exited
    /// non-zero): status `EXITED (<code>)`.
    Exited(i64),
    /// Ended without an exit code for a reason other than a deliberate stop:
    /// the shell couldn't be spawned, the root directory was missing, or it was
    /// killed by a signal Candle didn't send (a crash). Status `FAILED`.
    Failed,
}

/// Classify a service's latest run from that run's newest lifecycle row (start
/// initiated / failed / started / exited). A previous instance's late exit row
/// belongs to an older run and is ignored.
fn latest_run(
    conn: &Connection,
    project_dir: &str,
    command_name: &str,
) -> Result<LatestRun, CandleError> {
    let row: Option<(i64, Option<String>)> = conn
        .query_row(
            "select log_type, content from process_output \
             where project_dir = ?1 and command_name = ?2 and log_type in (?3, ?4, ?5, ?6) \
             and run_id is (select max(run_id) from process_output \
                            where project_dir = ?1 and command_name = ?2) \
             order by id desc limit 1",
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

/// Build a `list` / `list-all` result.
///
/// - `show_all`: list every alive process system-wide (no config required).
/// - otherwise: resolve the project config from `cwd`, list configured services
///   (config order) first — running or not — then append any running processes
///   not present in the config.
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
                    // The directory the service runs in, same as `list` reports.
                    &resolve_launch_dir(&entry.project_dir, entry.root.as_deref()),
                    &entry.project_dir,
                    entry.start_time,
                    entry.pid,
                    // No project context for drift detection in list-all.
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

    // Configured services first, in file order.
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

    // Then running processes not present in the config (transient / orphaned).
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

/// Restrict a listing to the named services (matched on service name), keeping
/// the original listing order. An empty `names` slice is a no-op. A name that
/// matches nothing is a usage error: in a project (`project_dir` set) the shared
/// `No service '<name>' configured for directory: <dir>`; for the system-wide
/// `list-all` (`None`) `No running service named '<name>'`.
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

/// Render a [`ListOutput`] as the multiline detail view used by `candle list`.
///
/// Each entry is a `[name]` header line followed by two-space-indented
/// `status:`, `command:` and `directory:` lines carrying the full,
/// untruncated values. Entries are separated by a blank line. The status line
/// reads `STATUS - pid N - uptime T`; `pid` and `uptime` are omitted for
/// services that are not running, and ` [config changed]` is appended to the
/// status on config drift.
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

/// Serialize the processes array as pretty JSON (2-space indent). This is the
/// shape the `--json` flag and MCP consume.
pub fn list_output_to_json(output: &ListOutput) -> String {
    serde_json::to_string_pretty(&output.processes).unwrap_or_else(|_| "[]".to_string())
}

/// Render a [`ListOutput`] as the pretty table.
///
/// An empty result prints `No services configured.`; otherwise a
/// `NAME STATUS PID UPTIME COMMAND DIRECTORY` table with two-space column
/// separators and a dashed separator row. ` [config changed]` is appended
/// to STATUS where the process drifted from config; PID 0 renders as `-`.
pub fn format_list_output(output: &ListOutput) -> String {
    format_table(output, true)
}

/// Render a [`ListOutput`] as the compact `candle ps` table: the same style as
/// [`format_list_output`] but with only `NAME STATUS PID UPTIME`, dropping the
/// two widest columns so the table fits in a narrow terminal.
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
        // Exact column order; old headers absent.
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

        // Empty filter is a no-op.
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

        // Falls back to the config service's shell when the row has none.
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

        // Nothing known at all.
        assert_eq!(resolve_shell(&no_shell, None), "");

        // The rendered row shows the shell string, not the service name.
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
        }
    }

    #[test]
    fn json_shape_matches_node() {
        // Running row: all keys incl. configChanged, in declaration order.
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

        // Not-running row: same keys, pid null.
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

        // Crashed row: status and exitCode carry the code.
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

        // A newer run that is still going (or stopped cleanly) clears it.
        log(ProcessLogType::ProcessStartInitiated, None);
        assert_eq!(latest(), LatestRun::Unremarkable);
        log(ProcessLogType::ProcessStarted, None);
        log(ProcessLogType::ProcessExited, Some("Process was stopped"));
        assert_eq!(latest(), LatestRun::Unremarkable);

        // A crash by a signal Candle didn't send is FAILED.
        log(ProcessLogType::ProcessStartInitiated, None);
        log(ProcessLogType::ProcessStarted, None);
        log(
            ProcessLogType::ProcessExited,
            Some(&crate::logs::log_type::killed_by_signal_message(11)),
        );
        assert_eq!(latest(), LatestRun::Failed);

        // A start that exited non-zero keeps its code.
        log(ProcessLogType::ProcessStartInitiated, None);
        log(
            ProcessLogType::ProcessStartFailed,
            Some("Process failed to start: exited with code 127"),
        );
        assert_eq!(latest(), LatestRun::Exited(127));

        // Start failures without an exit code are FAILED...
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

        // ...but a deliberate kill during startup is not.
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
