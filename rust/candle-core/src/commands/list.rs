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

fn running_row(
    service_name: &str,
    working_dir: &str,
    start_time: i64,
    pid: i64,
    config_changed: bool,
) -> ListProcess {
    ListProcess {
        service_name: service_name.to_string(),
        command: service_name.to_string(),
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
                &project_dir,
                entry.start_time,
                entry.pid,
                has_config_drift(entry, Some(service)),
            )),
            None => processes.push(ListProcess {
                service_name: service.name.clone(),
                command: service.name.clone(),
                working_dir: project_dir.clone(),
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
            &entry.project_dir,
            entry.start_time,
            entry.pid,
            has_config_drift(entry, config_service),
        ));
    }

    Ok(ListOutput { processes })
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
    if output.processes.is_empty() {
        return "No services configured.".to_string();
    }

    let headers = ["NAME", "STATUS", "PID", "UPTIME", "COMMAND", "DIRECTORY"];

    let rows: Vec<[String; 6]> = output
        .processes
        .iter()
        .map(|p| {
            let mut status = p.status.clone();
            if p.config_changed == Some(true) {
                status = format!("{status} [config changed]");
            }
            [
                p.service_name.clone(),
                status,
                if p.pid > 0 {
                    p.pid.to_string()
                } else {
                    "-".to_string()
                },
                p.uptime.clone(),
                p.command.clone(),
                p.working_dir.clone(),
            ]
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
