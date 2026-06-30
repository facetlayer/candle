//! MCP (Model Context Protocol) stdio server.
//!
//! Ported from `src/mcp/mcp-main.ts` + `src/mcp/ConsoleLogInterceptor.ts`. Lets an
//! LLM client manage local dev processes over a newline-delimited JSON-RPC stream
//! on stdin/stdout. See `rust/docs/porting/map-mcp.md` for the full spec.
//!
//! Architecture notes:
//! - **Transport is hand-rolled**, not `rmcp`: the candle-core handlers are
//!   synchronous (rusqlite), so a blocking line reader matches exactly and keeps
//!   the protocol under our control.
//! - **stdout purity:** only JSON-RPC frames may reach real stdout. Command
//!   handlers emit their human-readable output through [`crate::output`]; here we
//!   run each handler inside [`crate::output::capture`] so nothing leaks to stdout
//!   and the captured lines are surfaced inside the tool response instead.

use std::io::{BufRead, Write};
use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::config::commands::{add_server_config, AddServerConfigArgs};
use crate::config::file::find_project_dir;
use crate::errors::CandleError;
use crate::start::start_one_service::{start_one_service, RunOptions};

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_INSTRUCTIONS: &str = "Tool for running and managing local dev servers. Use this when launching any local servers, including web servers, APIs, and other services.";
const DEFAULT_LOGS_LIMIT: i64 = 200;

/// JSON-RPC "method not found" (also used for unknown tool names).
const METHOD_NOT_FOUND: i64 = -32601;

/// A tool handler: runs against the DB + cwd with the call's `arguments`,
/// returning an optional structured result. Human-readable output is emitted via
/// [`crate::output`] (captured by [`call_wrapped`]); a returned `Err` becomes an
/// `isError: true` tool response.
type Handler = fn(&Connection, &Path, &Value) -> Result<Option<Value>, CandleError>;

struct ToolDef {
    name: &'static str,
    description: &'static str,
    schema: Value,
    handler: Handler,
}

/// The tool registry, in the exact order the Node `toolDefinitions` array uses.
fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "ListServices",
            description: "List services with structured output",
            schema: json!({
                "type": "object",
                "properties": {
                    "showAll": { "type": "boolean", "description": "Show all services or just current directory (optional)" }
                }
            }),
            handler: tool_list_services,
        },
        ToolDef {
            name: "ListPorts",
            description: "List open ports for running services",
            schema: json!({
                "type": "object",
                "properties": {
                    "showAll": { "type": "boolean" },
                    "serviceName": { "type": "string", "description": "Filter to a specific service name (optional)" }
                }
            }),
            handler: tool_list_ports,
        },
        ToolDef {
            name: "GetLogs",
            description: "Get recent logs for a specific service",
            schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "limit": { "type": "number", "description": "Maximum number of log lines to return (optional)" },
                    "projectDir": { "type": "string", "description": "Project directory where the service is defined (optional - for cross-directory access)" }
                },
                "required": ["name"]
            }),
            handler: tool_get_logs,
        },
        ToolDef {
            name: "StartService",
            description: "Start a config-defined service (use StartTransientService for transient processes)",
            schema: json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            handler: tool_start_service,
        },
        ToolDef {
            name: "StartTransientService",
            description: "Start a transient process with a custom shell command (not defined in config file)",
            schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Name for the transient process" },
                    "shell": { "type": "string", "description": "Shell command to run the service" },
                    "root": { "type": "string", "description": "Root directory for the service (optional, relative to project)" }
                },
                "required": ["name", "shell"]
            }),
            handler: tool_start_transient_service,
        },
        ToolDef {
            name: "KillService",
            description: "Kill a running service",
            schema: json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }),
            handler: tool_kill_service,
        },
        ToolDef {
            name: "RestartService",
            description: "Restart a running service. If no name provided, restarts all running services in the project.",
            schema: json!({
                "type": "object",
                "properties": { "name": { "type": "string", "description": "Name of the service to restart. If not provided, restarts all running services." } }
            }),
            handler: tool_restart_service,
        },
        ToolDef {
            name: "AddServerConfig",
            description: "Add a new server configuration to .candle.json",
            schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "shell": { "type": "string" },
                    "root": { "type": "string", "description": "Root directory for the service (optional)" }
                },
                "required": ["name", "shell"]
            }),
            handler: tool_add_server_config,
        },
        ToolDef {
            name: "OpenBrowser",
            description: "Open a browser window to a running service's port",
            schema: json!({
                "type": "object",
                "properties": {
                    "serviceName": { "type": "string", "description": "Name of the service to open in browser" }
                },
                "required": ["serviceName"]
            }),
            handler: tool_open_browser,
        },
    ]
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn resolve_project_dir(cwd: &Path) -> Result<String, CandleError> {
    Ok(find_project_dir(cwd)?.display().to_string())
}

fn db_err(e: rusqlite::Error) -> CandleError {
    CandleError::Generic(format!("database error: {e}"))
}

// ---- tool handlers ---------------------------------------------------------

fn tool_list_services(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let show_all = args.get("showAll").and_then(|v| v.as_bool()).unwrap_or(false);
    let output = crate::commands::list::handle_list(conn, cwd, show_all)?;
    Ok(Some(serde_json::to_value(&output).unwrap_or_else(|_| json!({ "processes": [] }))))
}

fn tool_list_ports(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let show_all = args.get("showAll").and_then(|v| v.as_bool()).unwrap_or(false);
    let command_names: Vec<String> = arg_str(args, "serviceName")
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();
    let output = crate::commands::list_ports::handle_list_ports(conn, cwd, show_all, &command_names)?;
    Ok(Some(serde_json::to_value(&output).unwrap_or_else(|_| json!({ "ports": [] }))))
}

fn tool_get_logs(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let name = arg_str(args, "name")
        .ok_or_else(|| CandleError::Generic("Service name is required".to_string()))?
        .to_string();
    // `limit` is nullish-defaulted to 200: an explicit 0 passes through.
    let limit = match args.get("limit") {
        Some(v) if !v.is_null() => v.as_i64().unwrap_or(DEFAULT_LOGS_LIMIT),
        _ => DEFAULT_LOGS_LIMIT,
    };
    let project_dir = match arg_str(args, "projectDir") {
        Some(p) => p.to_string(),
        None => resolve_project_dir(cwd)?,
    };
    crate::commands::logs::handle_logs_command(conn, &project_dir, &[name], limit, None);
    Ok(None)
}

fn tool_start_service(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let name = arg_str(args, "name")
        .ok_or_else(|| CandleError::Generic("Service name is required".to_string()))?
        .to_string();
    let project_dir = resolve_project_dir(cwd)?;
    let result = start_one_service(
        conn,
        RunOptions {
            command_name: name,
            project_dir,
            shell: None,
            root: None,
            enable_stdin: false,
            check_start: false,
        },
    )?;
    Ok(Some(json!({
        "projectDir": result.project_dir,
        "serviceName": result.service_name,
    })))
}

fn tool_start_transient_service(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let name = arg_str(args, "name");
    let shell = arg_str(args, "shell");
    let (name, shell) = match (name, shell) {
        (Some(n), Some(s)) => (n.to_string(), s.to_string()),
        _ => {
            return Err(CandleError::Generic(
                "Service name and shell command are required".to_string(),
            ))
        }
    };
    let project_dir = resolve_project_dir(cwd)?;
    let result = start_one_service(
        conn,
        RunOptions {
            command_name: name,
            project_dir,
            shell: Some(shell),
            root: arg_str(args, "root").map(str::to_string),
            enable_stdin: false,
            check_start: false,
        },
    )?;
    Ok(Some(json!({
        "projectDir": result.project_dir,
        "serviceName": result.service_name,
    })))
}

fn tool_kill_service(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let name = arg_str(args, "name")
        .ok_or_else(|| CandleError::Generic("Service name is required".to_string()))?
        .to_string();
    let project_dir = resolve_project_dir(cwd)?;
    crate::kill::handle_kill_command(conn, &project_dir, &[name], false, false).map_err(db_err)?;
    Ok(None)
}

fn tool_restart_service(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let project_dir = resolve_project_dir(cwd)?;
    let names: Vec<String> = arg_str(args, "name").map(|s| vec![s.to_string()]).unwrap_or_default();
    crate::commands::restart::handle_restart(conn, &project_dir, &names)?;
    Ok(None)
}

fn tool_add_server_config(_conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let name = arg_str(args, "name");
    let shell = arg_str(args, "shell");
    let (name, shell) = match (name, shell) {
        (Some(n), Some(s)) => (n.to_string(), s.to_string()),
        _ => {
            return Err(CandleError::Generic(
                "Service name and shell command are required".to_string(),
            ))
        }
    };
    let msg = add_server_config(
        &AddServerConfigArgs {
            name,
            shell,
            root: arg_str(args, "root").map(str::to_string),
            enable_stdin: false,
        },
        cwd,
    )?;
    crate::output::out(&msg);
    Ok(None)
}

fn tool_open_browser(conn: &Connection, cwd: &Path, args: &Value) -> Result<Option<Value>, CandleError> {
    let service_name = arg_str(args, "serviceName")
        .ok_or_else(|| CandleError::Generic("Service name is required".to_string()))?;
    let project_dir = resolve_project_dir(cwd)?;
    let output = crate::commands::open_browser::handle_open_browser(
        conn,
        cwd,
        &project_dir,
        Some(service_name),
    )?;
    Ok(Some(serde_json::to_value(&output).unwrap_or(Value::Null)))
}

// ---- call wrapping ---------------------------------------------------------

struct CallOutcome {
    result: Option<Value>,
    error: Option<String>,
    logs: Vec<String>,
}

/// Run a handler with output capture, mirroring `callWrapped`: the captured
/// stdout/stderr lines become `logs`; a thrown error becomes `error`.
fn call_wrapped(handler: Handler, conn: &Connection, cwd: &Path, args: &Value) -> CallOutcome {
    let (res, captured) = crate::output::capture(|| handler(conn, cwd, args));
    let logs = captured.mcp_log_lines();
    match res {
        Ok(result) => CallOutcome { result, error: None, logs },
        Err(e) => CallOutcome { result: None, error: Some(e.to_string()), logs },
    }
}

/// Build the `tools/call` result `{ content, isError }` per map-mcp.md §4.
fn build_call_result(outcome: CallOutcome) -> Value {
    let mut content: Vec<Value> = Vec::new();
    if !outcome.logs.is_empty() {
        content.push(json!({ "type": "text", "text": outcome.logs.join("\n") }));
    }

    if let Some(error) = outcome.error {
        content.push(json!({ "type": "text", "text": format!("Error: {error}") }));
        return json!({ "content": content, "isError": true });
    }

    if let Some(result) = outcome.result {
        let text = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "null".to_string());
        content.push(json!({ "type": "text", "text": text }));
    }
    json!({ "content": content, "isError": false })
}

// ---- server loop -----------------------------------------------------------

/// Serve the MCP protocol over stdio, blocking until stdin closes. Exits the
/// process with code 0 on EOF (the transport has no auto-shutdown).
pub fn serve_mcp() -> ! {
    let conn = match crate::db::get_database(None) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("candle: failed to open database: {e}");
            std::process::exit(1);
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // ignore non-JSON lines
        };

        // Notifications have no `id` and never get a response.
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        let response = match handle_message(&conn, &cwd, method, &params, id.clone()) {
            Some(resp) => resp,
            None => continue, // notification or id-less message
        };

        if writeln!(out, "{}", serde_json::to_string(&response).unwrap_or_default()).is_err() {
            break;
        }
        let _ = out.flush();
    }

    std::process::exit(0);
}

/// Dispatch one request, returning the JSON-RPC response envelope, or `None` for
/// notifications / messages without an `id`.
fn handle_message(
    conn: &Connection,
    cwd: &Path,
    method: &str,
    params: &Value,
    id: Option<Value>,
) -> Option<Value> {
    // No id ⇒ notification (e.g. `notifications/initialized`): handle silently.
    let id = id?;

    let result_or_error = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION") },
            "instructions": SERVER_INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let tools: Vec<Value> = tool_definitions()
                .into_iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema }))
                .collect();
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match tool_definitions().into_iter().find(|t| t.name == name) {
                Some(tool) => {
                    let outcome = call_wrapped(tool.handler, conn, cwd, &arguments);
                    Ok(build_call_result(outcome))
                }
                None => Err((METHOD_NOT_FOUND, format!("Unknown tool: {name}"))),
            }
        }
        _ => Err((METHOD_NOT_FOUND, "Method not found".to_string())),
    };

    Some(match result_or_error {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_has_expected_names_in_order() {
        let names: Vec<&str> = tool_definitions().iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "ListServices",
                "ListPorts",
                "GetLogs",
                "StartService",
                "StartTransientService",
                "KillService",
                "RestartService",
                "AddServerConfig",
                "OpenBrowser",
            ]
        );
    }

    #[test]
    fn initialize_response_shape() {
        let resp = handle_message(
            &crate::db::get_database(Some(&crate::db::temp_db_dir("mcp-init"))).unwrap(),
            Path::new("."),
            "initialize",
            &Value::Null,
            Some(json!(1)),
        )
        .unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["capabilities"]["tools"], json!({}));
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], json!(1));
    }

    #[test]
    fn unknown_tool_is_method_not_found() {
        let conn = crate::db::get_database(Some(&crate::db::temp_db_dir("mcp-unknown"))).unwrap();
        let resp = handle_message(
            &conn,
            Path::new("."),
            "tools/call",
            &json!({ "name": "Nope", "arguments": {} }),
            Some(json!(2)),
        )
        .unwrap();
        assert_eq!(resp["error"]["code"], json!(METHOD_NOT_FOUND));
        assert!(resp["error"]["message"].as_str().unwrap().contains("Nope"));
    }

    #[test]
    fn notification_without_id_has_no_response() {
        let conn = crate::db::get_database(Some(&crate::db::temp_db_dir("mcp-notif"))).unwrap();
        assert!(handle_message(&conn, Path::new("."), "notifications/initialized", &Value::Null, None).is_none());
    }

    #[test]
    fn build_call_result_orders_logs_before_result() {
        let outcome = CallOutcome {
            result: Some(json!({ "ok": true })),
            error: None,
            logs: vec!["line one".to_string()],
        };
        let res = build_call_result(outcome);
        assert_eq!(res["isError"], json!(false));
        assert_eq!(res["content"][0]["text"], "line one");
        assert!(res["content"][1]["text"].as_str().unwrap().contains("\"ok\": true"));
    }

    #[test]
    fn build_call_result_error_sets_is_error() {
        let outcome = CallOutcome {
            result: None,
            error: Some("boom".to_string()),
            logs: vec![],
        };
        let res = build_call_result(outcome);
        assert_eq!(res["isError"], json!(true));
        assert_eq!(res["content"][0]["text"], "Error: boom");
    }
}
