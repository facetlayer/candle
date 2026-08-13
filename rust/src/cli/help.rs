// Help text rendering for the candle CLI.
//
// Ported verbatim from `printGroupedHelp()` in src/main-cli.ts. The Vitest help suite asserts on
// section headers and command names via substring checks, so the grouped layout is reproduced
// exactly. The `watch` line is hidden when running under an agent (see src/main-cli.ts), matching
// `isRunByAgent`.

use crate::run_context::is_run_by_agent;

/// The candle version, injected at build time by Cargo from the workspace `version`
/// field in Cargo.toml (`CARGO_PKG_VERSION`). This is the single source of truth for
/// both `--version` and the header line of the grouped help output.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn grouped_help() -> String {
    let version = version();
    let watch_line = if is_run_by_agent() {
        String::new()
    } else {
        "\n  watch [name...]           Watch live output from running process(es)".to_string()
    };

    format!(
        "candle {version}

Usage: candle <command> [options]

Process Management:
  list, ls, status          List processes for this project directory
  start, run [names...]     Start process(es); watches logs when run interactively
  check-start [names...]    Start process(es) only if not already running
  restart [names...]        Restart running process(es)
  kill [names...]           Kill running process(es)

Port Detection:
  list-ports [names...]     Uses the OS to detect and list the active open ports
  open-browser [name]       Open browser to service (auto-detects if one running)

Logs:
  logs [name...]            Show recent logs for process(es){watch_line}
  wait-for-log [name]       Wait for a specific log message

Configuration:
  setup-project             Create a new .candle.json in the current directory
  add-service [name] ...    Add a new service to .candle.json
  remove-service [name]     Remove a service from .candle.json
  set-config <key> <value>  Set a configuration option in .candle.json

Documentation:
  list-docs                 List available documentation
  get-doc <name>            Display a documentation file

Troubleshooting & Maintenance:
  list-all                  List all managed processes on this system
  kill-all                  Kill all managed processes on this system
  list-ports-all            List currently active ports for all managed processes
  clear-logs [name]         Clear logs for process(es)
  erase-database            Erase the Candle database

Options:
  help                      Show help
  mcp                       Enter MCP server mode
  --version                 Show version number

Run 'candle <command> --help' for more information on a command."
    )
}

/// Per-command help. Kept simple: the help tests assert that the output contains the command name
/// and its option flags, not an exact layout.
pub fn command_help(command: &str) -> String {
    match command {
        "start" | "run" => {
            "candle start [name...]   Start process(es)\n\nWhen run interactively, start watches the new process's logs after launching;\npress Ctrl+C to stop watching (the process keeps running in the background).\nWhen run non-interactively (agents, scripts, pipes), start exits as soon as\nthe launch is confirmed.\n\nOptions:\n  --watch            Force interactive mode: watch logs after starting\n  --bg               Force non-interactive mode: exit once started\n  --shell <cmd>      Shell command for a transient process\n  --root <dir>       Root directory for a transient process\n  --enable-stdin     Enable stdin message polling from database".to_string()
        }
        "check-start" => {
            "candle check-start [name...]   Start process(es) only if not already running\n\nOptions:\n  --shell <cmd>      Shell command for a transient process\n  --root <dir>       Root directory for a transient process\n  --enable-stdin     Enable stdin message polling from database".to_string()
        }
        "restart" => {
            "candle restart [name...]   Restart running process(es)\n\nFollows the same interactive behavior as start: when run interactively,\nrestart watches the restarted process's logs; press Ctrl+C to stop watching.\n\nOptions:\n  --watch            Force interactive mode: watch logs after restarting\n  --bg               Force non-interactive mode: exit once restarted".to_string()
        }
        "kill" | "stop" => "candle kill [name...]   Kill process(es) in the current directory".to_string(),
        "kill-all" => "candle kill-all   Kill all running processes".to_string(),
        "list" | "ls" | "status" => "candle list   List processes for the current directory\n\nOptions:\n  --json   Output as JSON".to_string(),
        "list-all" => "candle list-all   List all processes\n\nOptions:\n  --json   Output as JSON".to_string(),
        "logs" => {
            "candle logs [name...]   Show recent logs for process(es)\n\nOptions:\n  --count <n>      Number of log lines to show (default: 100)\n  --start-at <id>  Only show logs after this log ID".to_string()
        }
        "watch" => {
            "candle watch [name...]   Watch live output from running process(es)\n\nWatch never launches processes. With no name, it watches every process in\nthe project (including ones that haven't launched yet). With a name, the\nnamed process must already be running. Press Ctrl+C to stop watching.".to_string()
        }
        "wait-for-log" => {
            "candle wait-for-log [name]   Wait for a specific log message\n\nOptions:\n  --message <text>   The log message to wait for (required)\n  --timeout <secs>   Timeout in seconds (default: 30)".to_string()
        }
        "list-ports" => "candle list-ports [names...]   List open ports for running services".to_string(),
        "list-ports-all" => "candle list-ports-all   List open ports for all services".to_string(),
        "open-browser" => "candle open-browser [name]   Open a browser to a running service".to_string(),
        "setup-project" => "candle setup-project   Create a new .candle.json in the current directory".to_string(),
        "add-service" => {
            "candle add-service <name>   Add a new service to .candle.json\n\nOptions:\n  --shell <cmd>      Shell command to run the service (required)\n  --root <dir>       Root directory for the service\n  --enable-stdin     Enable stdin message polling from database".to_string()
        }
        "remove-service" => "candle remove-service <name>   Remove a service from .candle.json".to_string(),
        "set-config" => "candle set-config <key> <value>   Set a configuration option in .candle.json".to_string(),
        "clear-logs" => "candle clear-logs [name]   Clear logs for process(es)".to_string(),
        "erase-database" => "candle erase-database   Erase the Candle database".to_string(),
        "list-docs" => "candle list-docs   List available documentation".to_string(),
        "get-doc" => "candle get-doc <name>   Display a documentation file".to_string(),
        "mcp" => "candle mcp   Enter MCP server mode".to_string(),
        _ => grouped_help(),
    }
}
