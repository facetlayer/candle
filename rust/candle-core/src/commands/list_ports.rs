//! `list-ports` / `list-ports-all` command.
//!
//! Ported from `src/list-ports-command.ts`. Walks the process tree of each
//! managed process, runs a single system-wide `lsof` for listening TCP sockets,
//! and maps each socket back to the service that owns the PID.
//!
//! Note: unlike `list`, this uses the non-running query (`findProcessesByProjectDir`
//! includes killed rows) and does NOT prune dead PIDs — correctness comes from
//! `lsof` simply not matching dead PIDs.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::{Command, Stdio};

use rusqlite::Connection;
use serde::Serialize;

use crate::config::find_config_file;
use crate::db::process_table::{find_all_processes, find_processes_by_project_dir};
use crate::errors::CandleError;
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

/// Result of [`handle_list_ports`]. Mirrors `ListPortsOutput`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListPortsOutput {
    pub ports: Vec<PortInfo>,
}

/// A raw listening socket parsed from `lsof`, before service attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawPortInfo {
    pid: i64,
    port: i64,
    address: String,
    protocol: String,
}

fn db_err(e: rusqlite::Error) -> CandleError {
    CandleError::ConfigFileError(format!("database error: {e}"))
}

/// Build a `list-ports` / `list-ports-all` result.
///
/// - `show_all`: consider every process row system-wide; otherwise scope to the
///   project resolved from `cwd` (throws `MissingSetupFile` if no config).
/// - `command_names`: when non-empty, restrict to processes with those names.
pub fn handle_list_ports(
    conn: &Connection,
    cwd: &Path,
    show_all: bool,
    command_names: &[String],
) -> Result<ListPortsOutput, CandleError> {
    let project_dir = find_config_file(cwd)?.project_dir.display().to_string();

    let mut process_entries = if show_all {
        find_all_processes(conn).map_err(db_err)?
    } else {
        find_processes_by_project_dir(conn, &project_dir).map_err(db_err)?
    };

    if !command_names.is_empty() {
        process_entries.retain(|entry| command_names.contains(&entry.command_name));
    }

    // Compute each process's full tree, collect all PIDs for one lsof call, and
    // build a PID → service map (later trees overwrite earlier on collision, as
    // in the Node `Map.set` loop).
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

    let mut all_pids: Vec<i64> = Vec::new();
    for (_, _, pids) in &trees {
        all_pids.extend(pids.iter().copied());
    }

    if all_pids.is_empty() {
        return Ok(ListPortsOutput { ports: vec![] });
    }

    let raw_ports = get_listening_ports(&all_pids);

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

/// Run a single `lsof -iTCP -sTCP:LISTEN -n -P`, parse all listening sockets, and
/// filter to the requested PID set. On any spawn failure (e.g. lsof missing),
/// returns an empty list — matching the Node `error` handler.
fn get_listening_ports(pids: &[i64]) -> Vec<RawPortInfo> {
    if pids.is_empty() {
        return Vec::new();
    }
    let pid_set: HashSet<i64> = pids.iter().copied().collect();

    let output = Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let output = match output {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_lsof_output(&stdout)
        .into_iter()
        .filter(|p| pid_set.contains(&p.pid))
        .collect()
}

/// Parse `lsof` output into raw listening sockets.
///
/// Brittle positional parsing matching `parseLsofOutput`: only `LISTEN` lines,
/// ≥9 whitespace fields, PID from field 1, protocol from the first `TCP`/`UDP`
/// token (default `TCP`), name from the second-to-last token split at its last
/// `:`, `*` → `0.0.0.0`, deduped by `pid:port`.
fn parse_lsof_output(output: &str) -> Vec<RawPortInfo> {
    let mut port_infos: Vec<RawPortInfo> = Vec::new();

    for line in output.split('\n') {
        if !line.contains("LISTEN") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let pid: i64 = match parts[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let protocol = parts
            .iter()
            .find(|p| **p == "TCP" || **p == "UDP")
            .copied()
            .unwrap_or("TCP")
            .to_string();

        // Second-to-last token (the last is "(LISTEN)").
        let name_column = parts[parts.len() - 2];
        if !name_column.contains(':') {
            continue;
        }

        let last_colon = name_column.rfind(':').unwrap();
        let address = &name_column[..last_colon];
        let port_str = &name_column[last_colon + 1..];
        let port: i64 = match port_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let normalized_address = if address == "*" { "0.0.0.0" } else { address };

        port_infos.push(RawPortInfo {
            pid,
            port,
            address: normalized_address.to_string(),
            protocol,
        });
    }

    // Deduplicate by pid+port (lsof prints separate IPv4/IPv6 lines per listener).
    let mut seen: HashSet<String> = HashSet::new();
    port_infos
        .into_iter()
        .filter(|info| seen.insert(format!("{}:{}", info.pid, info.port)))
        .collect()
}

/// Serialize the output as the MCP-facing JSON (`{ports:[...]}`).
pub fn list_ports_output_to_json(output: &ListPortsOutput) -> String {
    serde_json::to_string_pretty(output).unwrap_or_else(|_| "{\"ports\":[]}".to_string())
}

/// Render a [`ListPortsOutput`] as the pretty table.
///
/// Mirrors `printListPortsOutput`: empty prints
/// `No open ports found for running services.`; otherwise a
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
    fn parse_ipv4_and_star() {
        let out = "\
COMMAND   PID   USER   FD   TYPE DEVICE SIZE/OFF NODE NAME
node    12345   user   45u  IPv4 0x1234    0t0  TCP 127.0.0.1:3000 (LISTEN)
node    12345   user   46u  IPv4 0x1235    0t0  TCP *:8080 (LISTEN)
";
        let parsed = parse_lsof_output(out);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].pid, 12345);
        assert_eq!(parsed[0].port, 3000);
        assert_eq!(parsed[0].address, "127.0.0.1");
        assert_eq!(parsed[0].protocol, "TCP");
        assert_eq!(parsed[1].address, "0.0.0.0");
        assert_eq!(parsed[1].port, 8080);
    }

    #[test]
    fn parse_ipv6_splits_on_last_colon() {
        let out =
            "node    222   user   7u  IPv6 0xabc    0t0  TCP [::1]:5173 (LISTEN)\n";
        let parsed = parse_lsof_output(out);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].address, "[::1]");
        assert_eq!(parsed[0].port, 5173);
    }

    #[test]
    fn dedup_by_pid_port() {
        let out = "\
node    1   u   7u  IPv4 0x1 0t0 TCP 127.0.0.1:3000 (LISTEN)
node    1   u   8u  IPv6 0x2 0t0 TCP [::1]:3000 (LISTEN)
";
        let parsed = parse_lsof_output(out);
        // Same pid:port → second dropped.
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn non_listen_and_short_lines_skipped() {
        let out = "\
node    1   u   7u  IPv4 0x1 0t0 TCP 127.0.0.1:3000 (ESTABLISHED)
short line LISTEN
";
        assert!(parse_lsof_output(out).is_empty());
    }

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
