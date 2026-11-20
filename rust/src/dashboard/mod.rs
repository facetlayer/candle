//! `candle dashboard run`: a localhost web UI listing every running service, with
//! buttons that open a service's ports in the browser. The React UI is built
//! ahead of time from `../dashboard` into `assets/dashboard/` and embedded
//! here; the JSON API reads the database directly.
//!
//! `candle dashboard open` runs the same server as a Candle-managed service in
//! the global namespace (see [`crate::dirs::global_project_dir`]).

pub mod http;
pub mod service;

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::thread;

use rusqlite::Connection;
use serde::Serialize;

use crate::commands::list::format_entry_uptime;
use crate::commands::list_ports::handle_list_ports;
use crate::commands::open_browser::open_service_port;
use crate::db::cleanup::maybe_run_cleanup;
use crate::db::get_database;
use crate::db::process_table::find_all_running_processes;
use crate::dirs::{global_project_dir, resolve_launch_dir};
use crate::errors::CandleError;
use crate::kill::handle_kill_command;
use crate::process_alive::{filter_alive_processes, find_running_service};
use http::{Request, Response};

/// Port the dashboard tries first. When it is taken and no port was asked
/// for, the dashboard falls back to any free port.
pub const DEFAULT_PORT: u16 = 7700;

const INDEX_HTML: &str = include_str!("../../assets/dashboard/index.html");
const APP_JS: &str = include_str!("../../assets/dashboard/app.js");
const STYLE_CSS: &str = include_str!("../../assets/dashboard/style.css");

/// Bind the loopback interface only. Without an explicit port, fall back to an
/// OS-assigned one when [`DEFAULT_PORT`] is taken.
pub fn bind(port: Option<u16>) -> Result<TcpListener, CandleError> {
    let addr = |p: u16| SocketAddr::from(([127, 0, 0, 1], p));
    let result = match port {
        Some(p) => TcpListener::bind(addr(p)),
        None => match TcpListener::bind(addr(DEFAULT_PORT)) {
            Err(e) if e.kind() == ErrorKind::AddrInUse => TcpListener::bind(addr(0)),
            other => other,
        },
    };
    result.map_err(|e| {
        CandleError::Generic(format!(
            "Could not start the dashboard on port {}: {e}",
            port.unwrap_or(DEFAULT_PORT)
        ))
    })
}

/// Serve the dashboard until the process is killed.
pub fn serve(listener: TcpListener) -> ! {
    for stream in listener.incoming().flatten() {
        thread::spawn(move || http::handle_connection(stream, route));
    }
    // `incoming()` never yields None.
    unreachable!("TcpListener::incoming ended")
}

/// Accept only loopback Host headers, so a page on another origin cannot
/// reach the API through DNS rebinding.
fn is_allowed_host(host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    let name = if let Some(rest) = host.strip_prefix('[') {
        // [::1]:port
        match rest.split_once(']') {
            Some((name, _)) => name,
            None => return false,
        }
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// Dispatch one request.
pub fn route(req: &Request) -> Response {
    if !is_allowed_host(req.header("host")) {
        return Response::error(
            403,
            "the dashboard only answers requests addressed to localhost",
        );
    }

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            Response::new(200, "text/html; charset=utf-8", INDEX_HTML)
        }
        ("GET", "/app.js") => Response::new(200, "text/javascript; charset=utf-8", APP_JS),
        ("GET", "/style.css") => Response::new(200, "text/css; charset=utf-8", STYLE_CSS),
        ("GET", "/api/processes") => {
            with_db(|conn| api_processes(conn).map(|out| Response::json(200, &out)))
        }
        ("POST", "/api/open-browser") => api_open_browser(req),
        ("POST", "/api/kill") => api_kill(req),
        (_, "/" | "/index.html" | "/app.js" | "/style.css" | "/api/processes") => {
            Response::error(405, "method not allowed")
        }
        (_, "/api/open-browser" | "/api/kill") => Response::error(405, "method not allowed"),
        _ => Response::error(404, "not found"),
    }
}

/// Run a handler with a fresh connection; connections are not shared across
/// request threads.
fn with_db(f: impl FnOnce(&Connection) -> Result<Response, CandleError>) -> Response {
    let conn = match get_database(None) {
        Ok(conn) => conn,
        Err(e) => return Response::error(500, &format!("Failed to open database: {e}")),
    };
    match f(&conn) {
        Ok(response) => response,
        Err(e @ CandleError::UsageError(_)) => Response::error(400, &e.to_string()),
        Err(e) => Response::error(500, &e.to_string()),
    }
}

/// A listening port in the API response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardPort {
    pub port: i64,
    pub address: String,
    pub protocol: String,
    pub is_child_process: bool,
}

/// A running service in the API response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardProcess {
    pub service_name: String,
    pub project_dir: String,
    pub command: String,
    pub working_dir: String,
    pub pid: i64,
    pub uptime: String,
    pub transient: bool,
    /// Runs in Candle's global namespace rather than a user project.
    pub is_global: bool,
    /// One entry per port number, lowest first.
    pub ports: Vec<DashboardPort>,
}

/// `GET /api/processes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessesResponse {
    pub version: &'static str,
    pub global_project_dir: String,
    pub processes: Vec<DashboardProcess>,
}

/// Every live service system-wide with its listening ports, sorted by project
/// then name.
pub fn api_processes(conn: &Connection) -> Result<ProcessesResponse, CandleError> {
    let _ = maybe_run_cleanup(conn);
    let global = global_project_dir();
    let entries = filter_alive_processes(conn, find_all_running_processes(conn)?)?;

    let mut ports: HashMap<(String, String), Vec<DashboardPort>> = HashMap::new();
    for p in handle_list_ports(conn, Path::new("/"), true, &[])?.ports {
        let list = ports.entry((p.project_dir, p.service_name)).or_default();
        // A port bound on both IPv4 and IPv6 is still one thing to open.
        if !list.iter().any(|existing| existing.port == p.port) {
            list.push(DashboardPort {
                port: p.port,
                address: p.address,
                protocol: p.protocol,
                is_child_process: p.is_child_process,
            });
        }
    }

    let mut processes: Vec<DashboardProcess> = entries
        .into_iter()
        .map(|entry| {
            let mut service_ports = ports
                .remove(&(entry.project_dir.clone(), entry.command_name.clone()))
                .unwrap_or_default();
            service_ports.sort_by_key(|p| p.port);
            DashboardProcess {
                uptime: format_entry_uptime(&entry),
                command: entry.shell.clone().unwrap_or_default(),
                working_dir: resolve_launch_dir(&entry.project_dir, entry.root.as_deref()),
                is_global: entry.project_dir == global,
                service_name: entry.command_name,
                project_dir: entry.project_dir,
                pid: entry.pid,
                transient: entry.transient,
                ports: service_ports,
            }
        })
        .collect();
    processes
        .sort_by(|a, b| (&a.project_dir, &a.service_name).cmp(&(&b.project_dir, &b.service_name)));

    Ok(ProcessesResponse {
        version: crate::cli::help::version(),
        global_project_dir: global,
        processes,
    })
}

/// A POST body naming one service: `{projectDir, serviceName, ...}`.
struct ServiceRequest {
    project_dir: String,
    service_name: String,
    body: serde_json::Value,
}

impl ServiceRequest {
    /// Parse the body. Requiring a JSON content type makes cross-origin form
    /// posts impossible without a CORS preflight, which this server never
    /// approves.
    fn parse(req: &Request) -> Result<Self, Response> {
        let is_json = req
            .header("content-type")
            .is_some_and(|ct| ct.split(';').next().unwrap_or("").trim() == "application/json");
        if !is_json {
            return Err(Response::error(
                415,
                "expected Content-Type: application/json",
            ));
        }
        let body: serde_json::Value = serde_json::from_slice(&req.body)
            .map_err(|_| Response::error(400, "request body is not valid JSON"))?;
        let field = |name: &str| body.get(name).and_then(|v| v.as_str()).map(str::to_string);
        let (Some(project_dir), Some(service_name)) = (field("projectDir"), field("serviceName"))
        else {
            return Err(Response::error(
                400,
                "projectDir and serviceName are required",
            ));
        };
        Ok(ServiceRequest {
            project_dir,
            service_name,
            body,
        })
    }

    /// An optional integer field.
    fn optional_i64(&self, name: &str) -> Result<Option<i64>, Response> {
        match self.body.get(name) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(v) => v
                .as_i64()
                .map(Some)
                .ok_or_else(|| Response::error(400, &format!("{name} must be a number"))),
        }
    }
}

/// `POST /api/open-browser` with `{projectDir, serviceName, port?}`.
fn api_open_browser(req: &Request) -> Response {
    let parsed = ServiceRequest::parse(req).and_then(|r| Ok((r.optional_i64("port")?, r)));
    let (port, r) = match parsed {
        Ok(v) => v,
        Err(response) => return response,
    };
    with_db(|conn| {
        open_service_port(conn, &r.project_dir, &r.service_name, port)
            .map(|out| Response::json(200, &out))
    })
}

/// `POST /api/kill` with `{projectDir, serviceName, pid?}`. With `pid`, only
/// that instance is killed, so a click on a stale row cannot kill a service
/// that has since restarted. The dashboard's own service is refused: killing
/// it would drop this very request.
fn api_kill(req: &Request) -> Response {
    let parsed = ServiceRequest::parse(req).and_then(|r| Ok((r.optional_i64("pid")?, r)));
    let (pid, r) = match parsed {
        Ok(v) => v,
        Err(response) => return response,
    };
    if crate::dirs::is_global_project_dir(&r.project_dir)
        && r.service_name == service::DASHBOARD_SERVICE_NAME
    {
        return Response::error(
            400,
            "The dashboard can't kill itself. Run 'candle dashboard close' instead.",
        );
    }

    with_db(|conn| {
        let Some(entry) = find_running_service(conn, &r.project_dir, &r.service_name)? else {
            return Ok(Response::error(
                404,
                &format!("Service '{}' is not running.", r.service_name),
            ));
        };
        if pid.is_some_and(|pid| pid != entry.pid) {
            return Ok(Response::error(
                409,
                &format!(
                    "Service '{}' has restarted (now pid {}); refresh and try again.",
                    r.service_name, entry.pid
                ),
            ));
        }
        // Keep the kill messages out of the dashboard's own log.
        let (result, _) = crate::output::capture(|| {
            handle_kill_command(
                conn,
                &r.project_dir,
                std::slice::from_ref(&r.service_name),
                true,
                true,
            )
        });
        result?;
        Ok(Response::json(
            200,
            &serde_json::json!({ "serviceName": r.service_name, "pid": entry.pid }),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn host_check_allows_only_loopback_names() {
        for ok in [
            "localhost",
            "localhost:7700",
            "127.0.0.1:1",
            "[::1]:7700",
            "LOCALHOST:7700",
        ] {
            assert!(is_allowed_host(Some(ok)), "{ok}");
        }
        for bad in [
            "evil.com",
            "evil.com:7700",
            "localhost.evil.com",
            "127.0.0.2",
            "[::1",
            "",
        ] {
            assert!(!is_allowed_host(Some(bad)), "{bad}");
        }
        assert!(!is_allowed_host(None));
    }

    #[test]
    fn foreign_host_is_forbidden() {
        let res = route(&request(
            "GET",
            "/",
            &[("host", "attacker.example:7700")],
            "",
        ));
        assert_eq!(res.status, 403);
    }

    #[test]
    fn serves_embedded_assets() {
        let host = [("host", "localhost:7700")];
        let index = route(&request("GET", "/", &host, ""));
        assert_eq!(index.status, 200);
        assert!(String::from_utf8_lossy(&index.body).contains("/app.js"));
        assert_eq!(
            route(&request("GET", "/app.js", &host, "")).content_type,
            "text/javascript; charset=utf-8"
        );
        assert_eq!(route(&request("GET", "/style.css", &host, "")).status, 200);
        assert_eq!(route(&request("GET", "/nope", &host, "")).status, 404);
        assert_eq!(
            route(&request("DELETE", "/api/processes", &host, "")).status,
            405
        );
        assert_eq!(
            route(&request("GET", "/api/open-browser", &host, "")).status,
            405
        );
    }

    #[test]
    fn kill_validates_request_and_refuses_the_dashboard_itself() {
        let host = ("host", "localhost:7700");
        let json = ("content-type", "application/json");
        let post = |headers: &[(&str, &str)], body: &str| {
            route(&request("POST", "/api/kill", headers, body))
        };
        assert_eq!(post(&[host], "{}").status, 415);
        assert_eq!(post(&[host, json], r#"{"serviceName":"web"}"#).status, 400);
        assert_eq!(
            post(
                &[host, json],
                r#"{"projectDir":"/p","serviceName":"web","pid":"x"}"#
            )
            .status,
            400
        );
        let own = serde_json::json!({
            "projectDir": global_project_dir(),
            "serviceName": service::DASHBOARD_SERVICE_NAME,
        });
        let res = post(&[host, json], &own.to_string());
        assert_eq!(res.status, 400);
        assert!(String::from_utf8_lossy(&res.body).contains("candle dashboard close"));
        assert_eq!(route(&request("GET", "/api/kill", &[host], "")).status, 405);
    }

    #[test]
    fn open_browser_validates_request_before_touching_the_database() {
        let host = ("host", "localhost:7700");
        let json = ("content-type", "application/json");
        let post = |headers: &[(&str, &str)], body: &str| {
            route(&request("POST", "/api/open-browser", headers, body)).status
        };
        assert_eq!(post(&[host], "{}"), 415);
        assert_eq!(post(&[host, ("content-type", "text/plain")], "{}"), 415);
        assert_eq!(post(&[host, json], "not json"), 400);
        assert_eq!(post(&[host, json], r#"{"projectDir":"/p"}"#), 400);
        assert_eq!(
            post(
                &[host, json],
                r#"{"projectDir":"/p","serviceName":"web","port":"x"}"#
            ),
            400
        );
    }
}
