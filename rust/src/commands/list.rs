//! `list` / `list-all` command.
//!
//! Ported from `src/list-command.ts`. Produces a structured listing of services
//! and running processes, plus a pretty-table formatter and the JSON shape the
//! `--json` flag and MCP consume.
//!
//! RUNNING is determined by liveness: the `list` query is already restricted to
//! `killed_at is null`, and [`filter_alive_processes`] drops (and deletes) rows
//! whose PIDs are dead, so killed/stale entries never show as RUNNING.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::Serialize;

use crate::config::{
    find_config_file, find_service_by_name, CandleSetupConfig, ServiceConfig,
};
use crate::db::process_table::{
    find_all_processes, find_running_processes_by_project_dir, ProcessEntry,
};
use crate::errors::CandleError;
use crate::process_alive::filter_alive_processes;

/// One row in a `list` result. Field order and (camelCase) names match the JSON
/// objects `handleList` emits, which `--json` serializes directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListProcess {
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub command: String,
    #[serde(rename = "workingDir")]
    pub working_dir: String,
    pub uptime: String,
    pub pid: i64,
    pub status: String,
    #[serde(rename = "configChanged", skip_serializing_if = "Option::is_none")]
    pub config_changed: Option<bool>,
}

/// Result of [`handle_list`]. Mirrors `ListOutput`; only `processes` is ever set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOutput {
    pub processes: Vec<ListProcess>,
}

const STATUS_RUNNING: &str = "RUNNING";
const STATUS_NOT_RUNNING: &str = "not running";

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Wrap a database error as a (non-usage) config error so the single `CandleError`
/// return type can carry it. These should not occur in practice.
fn db_err(e: rusqlite::Error) -> CandleError {
    CandleError::ConfigFileError(format!("database error: {e}"))
}

/// Whether a running process's stored command differs from its config entry.
/// Mirrors `hasConfigDrift`: compares `shell`, and `root` with empty/null/None
/// normalized to "unset".
fn has_config_drift(entry: &ProcessEntry, service: Option<&ServiceConfig>) -> bool {
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
/// Mirrors `formatUptime`: only non-zero components are shown, and an all-zero
/// duration renders as `"0s"`.
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

/// Resolve the directory a service actually runs in: `project_dir`, or the
/// service's `root` joined onto it (an absolute `root` replaces it outright).
/// Mirrors the `launch_dir` computation in `start::start_one_service`, so the
/// directory `list` reports matches the one the launch banner printed.
fn resolve_working_dir(project_dir: &str, root: Option<&str>) -> String {
    match root.filter(|r| !r.is_empty()) {
        Some(root) if Path::new(root).is_absolute() => root.to_string(),
        Some(root) => Path::new(project_dir).join(root).to_string_lossy().into_owned(),
        None => project_dir.to_string(),
    }
}

fn running_row(
    service_name: &str,
    command: &str,
    working_dir: &str,
    start_time: i64,
    pid: i64,
    config_changed: bool,
) -> ListProcess {
    ListProcess {
        service_name: service_name.to_string(),
        command: command.to_string(),
        working_dir: working_dir.to_string(),
        uptime: format_uptime(now_millis() - start_time * 1000),
        pid,
        status: STATUS_RUNNING.to_string(),
        config_changed: Some(config_changed),
    }
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
        let entries =
            filter_alive_processes(conn, find_all_processes(conn).map_err(db_err)?).map_err(db_err)?;
        let processes = entries
            .into_iter()
            .map(|entry| {
                running_row(
                    &entry.command_name,
                    &resolve_shell(&entry, None),
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

    let running =
        filter_alive_processes(conn, find_running_processes_by_project_dir(conn, &project_dir).map_err(db_err)?)
            .map_err(db_err)?;

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
                &resolve_working_dir(
                    &project_dir,
                    entry.root.as_deref().or(service.root.as_deref()),
                ),
                entry.start_time,
                entry.pid,
                has_config_drift(entry, Some(service)),
            )),
            None => processes.push(ListProcess {
                service_name: service.name.clone(),
                command: service.shell.clone(),
                working_dir: resolve_working_dir(&project_dir, service.root.as_deref()),
                uptime: "-".to_string(),
                pid: 0,
                status: STATUS_NOT_RUNNING.to_string(),
                config_changed: None,
            }),
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
            &entry.project_dir,
            entry.start_time,
            entry.pid,
            has_config_drift(entry, config_service),
        ));
    }

    Ok(ListOutput { processes })
}

/// Restrict a listing to the named services (matched on service name), keeping
/// the original listing order. An empty `names` slice is a no-op. A name that
/// matches nothing is a usage error.
pub fn filter_by_service_names(
    output: ListOutput,
    names: &[String],
) -> Result<ListOutput, CandleError> {
    if names.is_empty() {
        return Ok(output);
    }

    for name in names {
        if !output.processes.iter().any(|p| &p.service_name == name) {
            return Err(CandleError::UsageError(format!(
                "No service found with name: {name}"
            )));
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
/// Each entry is a header line (`name  STATUS  pid N  uptime T`) followed by
/// two-space-indented `command:` and `directory:` lines carrying the full,
/// untruncated values. Entries are separated by a blank line. `pid` and
/// `uptime` are omitted for services that are not running, and
/// ` [config changed]` is appended to the status on config drift.
pub fn format_list_detail(output: &ListOutput) -> String {
    if output.processes.is_empty() {
        return "No services configured.".to_string();
    }

    let mut entries: Vec<String> = Vec::new();
    for p in &output.processes {
        let mut header = format!("{}  {}", p.service_name, p.status);
        if p.config_changed == Some(true) {
            header.push_str(" [config changed]");
        }
        if p.status == STATUS_RUNNING {
            if p.pid > 0 {
                header.push_str(&format!("  pid {}", p.pid));
            }
            if !p.uptime.is_empty() && p.uptime != "-" {
                header.push_str(&format!("  uptime {}", p.uptime));
            }
        }
        entries.push(format!(
            "{header}\n  command:   {}\n  directory: {}",
            p.command, p.working_dir
        ));
    }

    entries.join("\n\n")
}

/// Serialize the processes array as pretty JSON (2-space indent), matching the
/// Node CLI's `JSON.stringify(output.processes, null, 2)`. This is the shape the
/// `--json` flag and MCP consume.
pub fn list_output_to_json(output: &ListOutput) -> String {
    serde_json::to_string_pretty(&output.processes).unwrap_or_else(|_| "[]".to_string())
}

/// Render a [`ListOutput`] as the pretty table.
///
/// Mirrors `printListOutput`: an empty result prints `No services configured.`;
/// otherwise a `NAME STATUS PID UPTIME COMMAND DIRECTORY` table with two-space
/// column separators and a dashed separator row. ` [config changed]` is appended
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
            if p.config_changed == Some(true) {
                status = format!("{status} [config changed]");
            }
            let mut cells = vec![
                p.service_name.clone(),
                status,
                if p.pid > 0 {
                    p.pid.to_string()
                } else {
                    "-".to_string()
                },
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
            rows.iter().map(|r| r[i].len()).max().unwrap_or(0).max(header_len)
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
    fn working_dir_resolves_root() {
        assert_eq!(resolve_working_dir("/proj", None), "/proj");
        assert_eq!(resolve_working_dir("/proj", Some("")), "/proj");
        assert_eq!(resolve_working_dir("/proj", Some("./sub")), "/proj/./sub");
        assert_eq!(resolve_working_dir("/proj", Some("sub")), "/proj/sub");
        assert_eq!(resolve_working_dir("/proj", Some("/elsewhere")), "/elsewhere");
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
                uptime: "5s".to_string(),
                pid: 42,
                status: "RUNNING".to_string(),
                config_changed: Some(true),
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
        assert!(name < status && status < pid && pid < uptime && uptime < command && command < directory);
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
                    uptime: "3m 5s".to_string(),
                    pid: 12345,
                    status: STATUS_RUNNING.to_string(),
                    config_changed: Some(false),
                },
                ListProcess {
                    service_name: "api".to_string(),
                    command: "npm run api".to_string(),
                    working_dir: "/proj".to_string(),
                    uptime: "-".to_string(),
                    pid: 0,
                    status: STATUS_NOT_RUNNING.to_string(),
                    config_changed: None,
                },
            ],
        }
    }

    #[test]
    fn detail_view_is_multiline_and_untruncated() {
        assert_eq!(
            format_list_detail(&sample()),
            "web  RUNNING  pid 12345  uptime 3m 5s\n  command:   npm run dev\n  directory: /proj/web\n\napi  not running\n  command:   npm run api\n  directory: /proj"
        );
    }

    #[test]
    fn detail_view_marks_config_changed_and_handles_empty() {
        let mut out = sample();
        out.processes[0].config_changed = Some(true);
        let text = format_list_detail(&out);
        assert!(text.starts_with("web  RUNNING [config changed]  pid 12345"));
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
        let filtered = filter_by_service_names(sample(), &["api".to_string()]).unwrap();
        assert_eq!(filtered.processes.len(), 1);
        assert_eq!(filtered.processes[0].service_name, "api");

        // Empty filter is a no-op.
        assert_eq!(
            filter_by_service_names(sample(), &[]).unwrap().processes.len(),
            2
        );

        let err = filter_by_service_names(sample(), &["nope".to_string()]).unwrap_err();
        assert!(format!("{err}").contains("nope"), "got: {err}");
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
        assert!(text.contains("command:   npm run dev"));
        assert!(!text.contains("command:   web"));
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
        }
    }

    #[test]
    fn json_shape_matches_node() {
        // Running row: all keys incl. configChanged, in declaration order.
        let running = ListProcess {
            service_name: "echo".to_string(),
            command: "echo".to_string(),
            working_dir: "/proj".to_string(),
            uptime: "5s".to_string(),
            pid: 42,
            status: "RUNNING".to_string(),
            config_changed: Some(false),
        };
        let json = serde_json::to_string(&running).unwrap();
        assert_eq!(
            json,
            r#"{"serviceName":"echo","command":"echo","workingDir":"/proj","uptime":"5s","pid":42,"status":"RUNNING","configChanged":false}"#
        );

        // Not-running row: configChanged omitted.
        let stopped = ListProcess {
            service_name: "web".to_string(),
            command: "web".to_string(),
            working_dir: "/proj".to_string(),
            uptime: "-".to_string(),
            pid: 0,
            status: "not running".to_string(),
            config_changed: None,
        };
        let json = serde_json::to_string(&stopped).unwrap();
        assert_eq!(
            json,
            r#"{"serviceName":"web","command":"web","workingDir":"/proj","uptime":"-","pid":0,"status":"not running"}"#
        );
    }
}
