//! `open-browser` command.
//!
//! Ported from `src/open-browser-command.ts`. Resolves a service name (explicit
//! or the sole running one), finds its lowest listening port via
//! [`handle_list_ports`], opens `http://localhost:<port>` in the platform
//! browser, and returns the chosen port/url.

use std::path::Path;
use std::process::{Command, Stdio};

use rusqlite::Connection;
use serde::Serialize;

use crate::commands::list_ports::handle_list_ports;
use crate::db::process_table::{
    find_processes_by_command_name_and_project_dir, find_processes_by_project_dir,
};
use crate::errors::CandleError;

/// Result of [`handle_open_browser`]. Field names match the MCP JSON shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenBrowserOutput {
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub port: i64,
    pub url: String,
}

fn db_err(e: rusqlite::Error) -> CandleError {
    CandleError::ConfigFileError(format!("database error: {e}"))
}

/// Resolve which service to open. An explicit name wins; otherwise there must be
/// exactly one process row (running or killed — `findProcessesByProjectDir`
/// includes killed) for the project.
fn resolve_service_name(
    conn: &Connection,
    project_dir: &str,
    provided_name: Option<&str>,
) -> Result<String, CandleError> {
    if let Some(name) = provided_name {
        if !name.is_empty() {
            return Ok(name.to_string());
        }
    }

    let processes = find_processes_by_project_dir(conn, project_dir).map_err(db_err)?;

    if processes.is_empty() {
        return Err(CandleError::UsageError(
            "No service name provided and no running processes found in this project.".to_string(),
        ));
    }

    if processes.len() > 1 {
        let names = processes
            .iter()
            .map(|p| p.command_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CandleError::UsageError(format!(
            "No service name provided and multiple processes are running: {names}. Please specify which service to open."
        )));
    }

    Ok(processes[0].command_name.clone())
}

/// Open a browser to the lowest listening port of a project service.
///
/// `cwd` is needed because port detection re-resolves the project config; the
/// caller passes the already-resolved `project_dir` for service-name resolution
/// (matching the Node split between `findProjectDir()` and the cwd-based
/// `handleListPorts`).
pub fn handle_open_browser(
    conn: &Connection,
    cwd: &Path,
    project_dir: &str,
    service_name: Option<&str>,
) -> Result<OpenBrowserOutput, CandleError> {
    let service_name = resolve_service_name(conn, project_dir, service_name)?;

    let ports_output =
        handle_list_ports(conn, cwd, false, std::slice::from_ref(&service_name))?;

    if ports_output.ports.is_empty() {
        let processes =
            find_processes_by_command_name_and_project_dir(conn, &service_name, project_dir)
                .map_err(db_err)?;
        let is_running = processes.iter().any(|p| p.killed_at.is_none());
        if is_running {
            return Err(CandleError::UsageError(format!(
                "No open ports found for service '{service_name}'."
            )));
        }
        return Err(CandleError::UsageError(format!(
            "No open ports found for service '{service_name}'. Start the service with: candle start"
        )));
    }

    // Pick the numerically lowest port.
    let port = ports_output
        .ports
        .iter()
        .map(|p| p.port)
        .min()
        .expect("non-empty");
    let url = format!("http://localhost:{port}");

    open_url(&url)?;

    Ok(OpenBrowserOutput {
        service_name,
        port,
        url,
    })
}

/// Launch the platform browser, fully detached so candle can exit without
/// killing it. Mirrors `openUrl`'s per-platform command table.
fn open_url(url: &str) -> Result<(), CandleError> {
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<&str>) = ("open", vec![url]);
    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<&str>) = ("cmd", vec!["/c", "start", "", url]);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let (program, args): (&str, Vec<&str>) = ("xdg-open", vec![url]);

    let spawn_result = Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match spawn_result {
        Ok(_child) => Ok(()),
        Err(e) => Err(CandleError::Generic(format!(
            "Failed to open browser: {e}"
        ))),
    }
}

/// The user-facing line printed after a successful open.
pub fn format_open_browser_output(output: &OpenBrowserOutput) -> String {
    format!("Opened {} in browser", output.url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::process_table::{create_process_entry, CreateProcessEntry};
    use crate::db::{get_database, temp_db_dir};

    fn entry(name: &str) -> CreateProcessEntry {
        CreateProcessEntry {
            command_name: name.to_string(),
            project_dir: "/proj".to_string(),
            pid: 1234,
            log_collector_pid: None,
            shell: None,
            root: None,
        }
    }

    #[test]
    fn resolve_uses_explicit_name() {
        let dir = temp_db_dir("open-browser-explicit");
        let conn = get_database(Some(&dir)).unwrap();
        assert_eq!(
            resolve_service_name(&conn, "/proj", Some("web")).unwrap(),
            "web"
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_errors_when_none() {
        let dir = temp_db_dir("open-browser-none");
        let conn = get_database(Some(&dir)).unwrap();
        let err = resolve_service_name(&conn, "/proj", None).unwrap_err();
        assert!(err.to_string().contains("no running processes"));
        assert!(err.is_usage_error());
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_single_running() {
        let dir = temp_db_dir("open-browser-single");
        let conn = get_database(Some(&dir)).unwrap();
        create_process_entry(&conn, &entry("solo")).unwrap();
        assert_eq!(
            resolve_service_name(&conn, "/proj", None).unwrap(),
            "solo"
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_errors_when_multiple() {
        let dir = temp_db_dir("open-browser-multi");
        let conn = get_database(Some(&dir)).unwrap();
        create_process_entry(&conn, &entry("a")).unwrap();
        create_process_entry(&conn, &entry("b")).unwrap();
        let err = resolve_service_name(&conn, "/proj", None).unwrap_err();
        assert!(err.to_string().contains("multiple processes are running"));
        assert!(err.to_string().contains("a, b"));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn output_line() {
        let out = OpenBrowserOutput {
            service_name: "web".to_string(),
            port: 3000,
            url: "http://localhost:3000".to_string(),
        };
        assert_eq!(format_open_browser_output(&out), "Opened http://localhost:3000 in browser");
    }
}
