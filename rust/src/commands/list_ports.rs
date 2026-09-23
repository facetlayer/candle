//! `list-ports` / `list-ports-all` command.
//!
//! Walks the process tree of each managed process, asks the platform for the
//! listening TCP sockets of those PIDs (see [`crate::listening_ports`]), and
//! maps each socket back to the service that owns the PID.
//!
//! Note: unlike `list`, this uses the non-running query
//! (`find_processes_by_project_dir` includes killed rows) and does NOT prune dead PIDs — correctness comes from
//! dead PIDs simply owning no sockets.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::config::{find_config_file, find_service_by_name};
use crate::db::process_table::{find_all_processes, find_processes_by_project_dir};
use crate::errors::CandleError;
use crate::listening_ports::listening_sockets_for_pids;
use crate::process_tree::get_process_tree;

/// One listening socket attributed to a service. Field names match the JSON the
/// MCP `ListPorts` tool serializes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortInfo {
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub pid: i64,
    pub port: i64,
    pub address: String,
    pub protocol: String,
    #[serde(rename = "isChildProcess")]
    pub is_child_process: bool,
}

/// Result of [`handle_list_ports`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListPortsOutput {
    pub ports: Vec<PortInfo>,
}

/// Build a `list-ports` / `list-ports-all` result.
///
/// - `show_all`: consider every process row system-wide, with no project needed;
///   otherwise scope to the project resolved from `cwd` (returns
///   `MissingSetupFile` if no config).
/// - `command_names`: when non-empty, restrict to processes with those names.
///   In project scope each name must be a configured service or have a process
///   row in the project (a transient service), else `MissingServiceWithName`.
pub fn handle_list_ports(
    conn: &Connection,
    cwd: &Path,
    show_all: bool,
    command_names: &[String],
) -> Result<ListPortsOutput, CandleError> {
    let mut process_entries = if show_all {
        find_all_processes(conn)?
    } else {
        let found = find_config_file(cwd)?;
        let project_dir = found.project_dir.display().to_string();
        let entries = find_processes_by_project_dir(conn, &project_dir)?;
        for name in command_names {
            let configured = find_service_by_name(&found.config, name).is_some();
            let has_row = entries.iter().any(|e| &e.command_name == name);
            if !configured && !has_row {
                return Err(CandleError::unknown_service(name, &project_dir));
            }
        }
        entries
    };

    if !command_names.is_empty() {
        process_entries.retain(|entry| command_names.contains(&entry.command_name));
    }

    // Compute each process's full tree, collect all PIDs for one socket lookup,
    // and build a PID → service map (later trees overwrite earlier on collision).
    let trees: Vec<(String, i64, Vec<i64>)> = process_entries
        .iter()
        .map(|entry| {
            (
                entry.command_name.clone(),
                entry.pid,
                get_process_tree(entry.pid),
            )
        })
        .collect();

    let all_pids: HashSet<i64> = trees
        .iter()
        .flat_map(|(_, _, pids)| pids.iter().copied())
        .collect();

    if all_pids.is_empty() {
        return Ok(ListPortsOutput { ports: vec![] });
    }

    let raw_ports =
        listening_sockets_for_pids(&all_pids).map_err(|e| CandleError::Generic(e.to_string()))?;

    let mut pid_to_service: HashMap<i64, (String, i64)> = HashMap::new();
    for (service_name, root_pid, pids) in &trees {
        for pid in pids {
            pid_to_service.insert(*pid, (service_name.clone(), *root_pid));
        }
    }

    let mut ports: Vec<PortInfo> = Vec::new();
    for raw in raw_ports {
        if let Some((service_name, root_pid)) = pid_to_service.get(&raw.pid) {
            ports.push(PortInfo {
                service_name: service_name.clone(),
                pid: raw.pid,
                port: raw.port,
                address: raw.address,
                protocol: raw.protocol,
                is_child_process: raw.pid != *root_pid,
            });
        }
    }

    Ok(ListPortsOutput { ports })
}

/// Serialize the output as the MCP-facing JSON (`{ports:[...]}`).
pub fn list_ports_output_to_json(output: &ListPortsOutput) -> String {
    serde_json::to_string_pretty(output).unwrap_or_else(|_| "{\"ports\":[]}".to_string())
}

/// Render a [`ListPortsOutput`] as the pretty table.
///
/// Empty prints `No open ports found for running services.`; otherwise a
/// `SERVICE PID PORT ADDRESS PROTOCOL` table with ` (child)` appended to the
/// PROTOCOL cell for child-process ports.
pub fn format_list_ports_output(output: &ListPortsOutput) -> String {
    if output.ports.is_empty() {
        return "No open ports found for running services.".to_string();
    }

    let headers = ["SERVICE", "PID", "PORT", "ADDRESS", "PROTOCOL"];
    let rows: Vec<[String; 5]> = output
        .ports
        .iter()
        .map(|p| {
            let suffix = if p.is_child_process { " (child)" } else { "" };
            [
                p.service_name.clone(),
                p.pid.to_string(),
                p.port.to_string(),
                p.address.clone(),
                format!("{}{}", p.protocol, suffix),
            ]
        })
        .collect();

    let widths: Vec<usize> = (0..headers.len())
        .map(|i| {
            rows.iter()
                .map(|r| r[i].len())
                .max()
                .unwrap_or(0)
                .max(headers[i].len())
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
    fn empty_output_message() {
        let out = ListPortsOutput { ports: vec![] };
        assert_eq!(
            format_list_ports_output(&out),
            "No open ports found for running services."
        );
    }

    #[test]
    fn table_marks_child_and_headers() {
        let out = ListPortsOutput {
            ports: vec![PortInfo {
                service_name: "web".to_string(),
                pid: 42,
                port: 3000,
                address: "127.0.0.1".to_string(),
                protocol: "TCP".to_string(),
                is_child_process: true,
            }],
        };
        let text = format_list_ports_output(&out);
        let header = text.lines().next().unwrap();
        assert!(header.starts_with("SERVICE"));
        assert!(header.contains("PROTOCOL"));
        assert!(text.contains("(child)"));
        assert!(text.contains("3000"));
    }

    #[test]
    fn json_shape_is_ports_wrapper() {
        let out = ListPortsOutput {
            ports: vec![PortInfo {
                service_name: "web".to_string(),
                pid: 42,
                port: 3000,
                address: "127.0.0.1".to_string(),
                protocol: "TCP".to_string(),
                is_child_process: false,
            }],
        };
        let json = serde_json::to_string(&out).unwrap();
        assert_eq!(
            json,
            r#"{"ports":[{"serviceName":"web","pid":42,"port":3000,"address":"127.0.0.1","protocol":"TCP","isChildProcess":false}]}"#
        );
    }
}
