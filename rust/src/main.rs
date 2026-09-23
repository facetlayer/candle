// candle — the only binary Candle ships.
//
// Three modes, decided here:
//   - `--monitor`: supervise one service subprocess (see cli/monitor_mode.rs). The CLI
//     re-invokes itself this way for every service it launches.
//   - `mcp`: stdio MCP server.
//   - anything else: the normal CLI. Hand-rolled command dispatch; see cli/help.rs and
//     cli/parser.rs for the help text and argument parsing.

use std::path::PathBuf;
use std::process::exit;

use candle::cli::help;
use candle::cli::monitor_mode::run_monitor_mode;
use candle::cli::parser::{canonical_command, parse_command_args, CommandArgs};
use candle::commands::clear_logs::handle_clear_logs_command;
use candle::commands::find_orphans::{format_find_orphans, handle_find_orphans};
use candle::commands::list::{
    filter_by_service_names, format_list_detail, format_list_output, format_ps_output, handle_list,
    list_output_to_json,
};
use candle::commands::list_ports::{
    format_list_ports_output, handle_list_ports, list_ports_output_to_json,
};
use candle::commands::logs::{handle_logs_command, LogsCommandOptions};
use candle::commands::open_browser::{format_open_browser_output, handle_open_browser};
use candle::commands::restart::handle_restart;
use candle::commands::wait_for_log::handle_wait_for_log;
use candle::commands::watch::{handle_watch, watch_started_services};
use candle::commands::{assert_known_service_names_in_scope, assert_valid_command_names};
use candle::config::commands::{
    add_server_config, handle_set_config, handle_setup_project, remove_server_config,
    AddServerConfigArgs,
};
use candle::db::cleanup::maybe_run_cleanup;
use candle::db::get_database;
use candle::doc_files::{self, DocLookupError};
use candle::errors::{error_line, CandleError};
use candle::kill::{handle_kill_all, handle_kill_command};
use candle::project_scope::ProjectScope;
use candle::start::{handle_start_command, StartCommandOptions};
use rusqlite::Connection;

fn main() {
    // Rust ignores SIGPIPE by default, which turns `candle watch | head` into a
    // "failed printing to stdout: Broken pipe" panic. Restore the conventional
    // Unix behavior: exit quietly when the reader goes away.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();

    // --monitor short-circuits everything: this process becomes a service monitor
    // and never returns to the CLI path.
    if argv.iter().any(|a| a == "--monitor") {
        run_monitor_mode(&argv);
    }

    // --version / -v short-circuits everything.
    if argv.iter().any(|a| a == "--version" || a == "-v") {
        println!("{}", help::version());
        return;
    }

    // No arguments at all → grouped help on stdout.
    if argv.is_empty() {
        println!("{}", help::grouped_help());
        return;
    }

    let help_flag = argv.iter().any(|a| a == "--help" || a == "-h");
    let command_token = argv.iter().find(|a| !a.starts_with('-')).cloned();

    // --help / -h → grouped help, or command-specific help when a command is named.
    if help_flag {
        match command_token.as_deref().and_then(canonical_command) {
            Some(cmd) => println!("{}", help::command_help(cmd)),
            None => println!("{}", help::grouped_help()),
        }
        return;
    }

    let command_token = match command_token {
        Some(c) => c,
        None => {
            // Only unrecognized flags, no command — fall back to help.
            println!("{}", help::grouped_help());
            return;
        }
    };

    let canonical = match canonical_command(&command_token) {
        Some(c) => c,
        None => {
            fatal(format!(
                "Unrecognized command '{command_token}'\nRun \"candle help\" for available commands."
            ));
        }
    };

    let cmd_index = argv.iter().position(|a| a == &command_token).unwrap();
    let rest = &argv[cmd_index + 1..];

    if canonical == "help" {
        // `candle help <command>` is the same as `candle <command> --help`.
        if let Some(topic) = rest.iter().find(|a| !a.starts_with('-')) {
            match canonical_command(topic) {
                Some(cmd) => println!("{}", help::command_help(cmd)),
                None => {
                    fatal(format!(
                        "Unknown help topic: {topic}\nRun \"candle help\" for available commands."
                    ));
                }
            }
            return;
        }
        println!("{}", help::grouped_help());
        return;
    }

    // `mcp` → MCP server mode (never returns).
    if canonical == "mcp" {
        run_mcp();
    }

    let args = match parse_command_args(canonical, rest) {
        Ok(a) => a,
        Err(msg) => fatal(msg),
    };

    dispatch(canonical, &args);
}

fn dispatch(command: &str, args: &CommandArgs) {
    match command {
        "setup-project" => print_or_exit(handle_setup_project(&cwd())),
        "add-service" => cmd_add_service(args),
        "remove-service" => cmd_remove_service(args),
        "set-config" => cmd_set_config(args),
        "list-docs" => cmd_list_docs(),
        "get-doc" => cmd_get_doc(args),
        "kill" => cmd_kill(args),
        "kill-all" => cmd_kill_all(),
        "find-orphans" => cmd_find_orphans(args),
        "start" => cmd_start(args, false),
        "check-start" => cmd_start(args, true),
        "list" => cmd_list(args, false, ListView::Detail),
        "ps" => cmd_list(args, false, ListView::PsTable),
        "list-all" => cmd_list(args, true, ListView::FullTable),
        "wait-for-log" => cmd_wait_for_log(args),
        "logs" => cmd_logs(args),
        "clear-logs" => cmd_clear_logs(args),
        "restart" => cmd_restart(args),
        "watch" => cmd_watch(args),
        "list-ports" => cmd_list_ports(args, false),
        "list-ports-all" => cmd_list_ports(args, true),
        "open-browser" => cmd_open_browser(args),
        "erase-database" => cmd_erase_database(args),
        _ => not_implemented(command),
    }
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// The project a command acts on: the CWD, or whatever `--project-dir` names.
fn scope_of(args: &CommandArgs) -> ProjectScope {
    ProjectScope::new(cwd(), args.value("project-dir"))
}

/// Resolve a command's project directory, exiting with the usage error if the
/// CWD is not inside a project.
fn project_dir_or_exit(scope: &ProjectScope) -> String {
    match scope.resolve() {
        Ok(dir) => dir,
        Err(e) => fail_with(&e),
    }
}

/// Same, for commands that need the project's service definitions: an explicit
/// `--project-dir` must be a project in its own right. See
/// [`ProjectScope::require_own_config`].
fn configured_project_dir_or_exit(scope: &ProjectScope) -> String {
    if let Err(e) = scope.require_own_config() {
        fail_with(&e);
    }
    project_dir_or_exit(scope)
}

/// Print a handler's success message, or print its error to stderr and exit 1.
fn print_or_exit<E: std::fmt::Display>(result: Result<String, E>) {
    match result {
        Ok(msg) => println!("{msg}"),
        Err(e) => fatal(e),
    }
}

fn cmd_add_service(args: &CommandArgs) {
    let name = match args.positionals.first() {
        Some(n) => n.clone(),
        None => {
            fatal("Service name is required");
        }
    };
    if args.positionals.len() > 1 {
        fatal("Cannot use multiple command names for add-service");
    }
    let shell = match args.value("shell") {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            fatal("--shell <command> is required");
        }
    };
    let config_args = AddServerConfigArgs {
        name,
        shell,
        root: args.value("root").map(str::to_string),
        enable_stdin: args.has("enable-stdin"),
    };
    print_or_exit(add_server_config(&config_args, &cwd()));
}

fn cmd_remove_service(args: &CommandArgs) {
    let name = match args.positionals.first() {
        Some(n) => n.clone(),
        None => {
            fatal("Service name is required");
        }
    };
    if args.positionals.len() > 1 {
        fatal("Cannot use multiple command names for remove-service");
    }
    print_or_exit(remove_server_config(&name, &cwd()));
}

fn cmd_set_config(args: &CommandArgs) {
    let (key, value) = match (args.positionals.first(), args.positionals.get(1)) {
        (Some(k), Some(v)) => (k.clone(), v.clone()),
        _ => {
            fatal("set-config requires a <key> and a <value>");
        }
    };
    print_or_exit(handle_set_config(&key, &value, &cwd()));
}

fn cmd_list_docs() {
    let docs = doc_files::list_docs();
    let width = docs.iter().map(|d| d.name.len()).max().unwrap_or(0);
    println!("Available docs (show one with 'candle get-doc <name>'):\n");
    for doc in docs {
        if doc.description.is_empty() {
            println!("  {}", doc.name);
        } else {
            println!("  {:width$}  {}", doc.name, doc.description);
        }
    }
}

fn cmd_get_doc(args: &CommandArgs) {
    let name = match args.positionals.first() {
        Some(n) if !n.trim().is_empty() => n.as_str(),
        _ => fatal("get-doc requires a <name>\nRun \"candle list-docs\" to see available docs."),
    };
    match doc_files::get_doc(name) {
        Ok(doc) => {
            println!("{}", doc.content);
            println!("\n(File source: {})", doc.source_path);
        }
        Err(DocLookupError::NotFound) => {
            fatal(format!(
                "Doc file not found: {name}\nRun with \"list-docs\" command to see available docs."
            ));
        }
    }
}

/// Open the candle database (resolving the state dir from the environment), or
/// print an error and exit.
fn open_db() -> Connection {
    match get_database(None) {
        Ok(conn) => conn,
        Err(e) => fatal(format!("Failed to open database: {e}")),
    }
}

/// Print a fatal error to stderr as `Error: <message>` and exit 1. Every fatal
/// user-facing error in the CLI goes through here, so the prefix is added in
/// exactly one place (see [`error_line`]).
fn fatal(message: impl std::fmt::Display) -> ! {
    eprintln!("{}", error_line(&message.to_string()));
    exit(1);
}

/// Print a CandleError to stderr and exit 1.
fn fail_with(err: &CandleError) -> ! {
    fatal(err)
}

/// `kill` / `stop`: resolve the project dir, validate names, then mark/kill.
///
/// With an explicit `--project-dir`, name validation is skipped: that form
/// exists to clean up after a project that is gone, so there is no config left
/// to validate against. An unrecognized name simply reports that nothing by
/// that name is running.
fn cmd_kill(args: &CommandArgs) {
    let scope = scope_of(args);
    let project_dir = project_dir_or_exit(&scope);

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    if !scope.is_explicit() {
        if let Err(e) = assert_valid_command_names(&conn, scope.base_dir(), &args.positionals) {
            fail_with(&e);
        }
    }

    if let Err(e) = handle_kill_command(&conn, &project_dir, &args.positionals, false, false) {
        fatal(format!("Database error: {e}"));
    }
}

/// `kill-all`: kill every process across every project. No validation, no config.
fn cmd_kill_all() {
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);
    if let Err(e) = handle_kill_all(&conn, false) {
        fatal(format!("Database error: {e}"));
    }
}

/// Decide whether a launch-style command (`start` / `restart`) should stay
/// attached and watch logs. `--watch` forces interactive, `--bg` forces
/// non-interactive; otherwise auto-detect (human at a TTY → watch; agent,
/// script, or pipe → return immediately).
fn should_watch_after_launch(args: &CommandArgs) -> bool {
    let force_bg = args.has("bg");
    let force_watch = args.has("watch");
    if force_bg && force_watch {
        fatal("Cannot use --bg and --watch together");
    }
    if force_watch {
        true
    } else if force_bg {
        false
    } else {
        candle::run_context::is_interactive()
    }
}

/// Print the follow-up hint after a non-interactive launch.
fn print_logs_hint(started: &[String]) {
    let hint = if started.len() == 1 {
        format!("Run 'candle logs {}' to see logs.", started[0])
    } else {
        "Run 'candle logs' to see logs.".to_string()
    };
    println!("{hint}");
}

/// `start` / `run` (`check_start = false`) and `check-start` (`check_start =
/// true`): resolve the project dir, then launch the requested service(s).
///
/// In interactive mode, `start` stays attached and streams the new process's
/// logs until Ctrl+C (the process keeps running). In non-interactive mode (and
/// always for `check-start`), it exits as soon as the launch is confirmed.
fn cmd_start(args: &CommandArgs, check_start: bool) {
    let project_dir = configured_project_dir_or_exit(&scope_of(args));

    let watch_after = !check_start && should_watch_after_launch(args);

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    let opts = StartCommandOptions {
        project_dir: project_dir.clone(),
        command_names: args.positionals.clone(),
        shell: args.value("shell").map(str::to_string),
        root: args.value("root").map(str::to_string),
        enable_stdin: args.has("enable-stdin"),
        check_start,
    };

    match handle_start_command(&conn, opts) {
        Ok(started) => {
            if watch_after {
                let exit_after_ms: Option<u64> =
                    args.value("exit-after-ms").and_then(|s| s.parse().ok());
                if let Err(e) = watch_started_services(&conn, &project_dir, &started, exit_after_ms)
                {
                    fail_with(&e);
                }
            } else {
                print_logs_hint(&started);
            }
            exit(0)
        }
        Err(e) => fail_with(&e),
    }
}

/// Which renderer a listing command uses for its non-JSON output.
enum ListView {
    /// The multiline detail view (`candle list`).
    Detail,
    /// The compact NAME/STATUS/PID/UPTIME table (`candle ps`).
    PsTable,
    /// The full table, including COMMAND and DIRECTORY (`candle list-all`).
    FullTable,
}

/// `list` / `ls`, `ps` / `status` (`show_all = false`) and `list-all`
/// (`show_all = true`).
fn cmd_list(args: &CommandArgs, show_all: bool, view: ListView) {
    let scope = scope_of(args);
    if let Err(e) = scope.require_own_config() {
        fail_with(&e);
    }
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    // `list-all` is system-wide, so an unknown name there has no project to name.
    let project_dir = if show_all || args.positionals.is_empty() {
        None
    } else {
        Some(project_dir_or_exit(&scope))
    };
    let output = match handle_list(&conn, scope.base_dir(), show_all).and_then(|output| {
        filter_by_service_names(output, &args.positionals, project_dir.as_deref())
    }) {
        Ok(output) => output,
        Err(e) => fail_with(&e),
    };

    if args.has("json") {
        println!("{}", list_output_to_json(&output));
    } else {
        match view {
            ListView::Detail => println!("{}", format_list_detail(&output)),
            ListView::PsTable => println!("{}", format_ps_output(&output)),
            ListView::FullTable => println!("{}", format_list_output(&output)),
        }
    }
}

/// `wait-for-log`: poll the named command's logs for a substring until it
/// appears, the process exits, or the timeout elapses.
fn cmd_wait_for_log(args: &CommandArgs) {
    let scope = scope_of(args);
    let project_dir = project_dir_or_exit(&scope);

    // --message is required (yargs demandOption).
    let message = match args.value("message") {
        Some(m) => m,
        None => {
            fatal("--message <text> is required");
        }
    };

    // --timeout is in seconds, default 30; convert to ms like TS `timeout * 1000`.
    let timeout_secs: f64 = args
        .value("timeout")
        .and_then(|s| s.parse().ok())
        .unwrap_or(30.0);
    let timeout_ms = (timeout_secs * 1000.0) as u64;

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    exit_on_unknown_service_names(&conn, &scope, &project_dir, &args.positionals);

    let result = handle_wait_for_log(&conn, &project_dir, &args.positionals, message, timeout_ms);
    if !result.success {
        exit(1);
    }
}

/// Exit with `No service '<name>' configured` if a name has neither stored
/// logs, a process row, nor a config entry. See
/// [`candle::commands::assert_known_service_names`].
fn exit_on_unknown_service_names(
    conn: &Connection,
    scope: &ProjectScope,
    project_dir: &str,
    names: &[String],
) {
    if let Err(e) = assert_known_service_names_in_scope(conn, scope, project_dir, names) {
        fail_with(&e);
    }
}

fn cmd_logs(args: &CommandArgs) {
    let scope = scope_of(args);
    let project_dir = project_dir_or_exit(&scope);

    let limit: i64 = args
        .value("count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let start_at_id: Option<i64> = args.value("start-at").and_then(|s| s.parse().ok());

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    exit_on_unknown_service_names(&conn, &scope, &project_dir, &args.positionals);

    let options = LogsCommandOptions {
        start_at_id,
        json: args.has("json"),
        ..LogsCommandOptions::cli(limit)
    };
    handle_logs_command(&conn, &project_dir, &args.positionals, &options);
}

/// `clear-logs`: delete stored output for the named command(s) in the project.
fn cmd_clear_logs(args: &CommandArgs) {
    let scope = scope_of(args);
    let project_dir = project_dir_or_exit(&scope);

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    // Same rule as `logs`: stored logs, a process row, or a config entry.
    exit_on_unknown_service_names(&conn, &scope, &project_dir, &args.positionals);

    match handle_clear_logs_command(&conn, &project_dir, &args.positionals) {
        Ok(()) => {}
        Err(e) => fatal(format!("Could not clear logs: {e}")),
    }
}

/// `restart`: kill the named (or all running) services in the project, then
/// start them again. An unknown service name fails validation (stderr + exit 1);
/// an empty project with nothing running yields the "No running processes" usage
/// error from the handler. Follows the same interactive/non-interactive behavior
/// as `start` (see [`cmd_start`]).
fn cmd_restart(args: &CommandArgs) {
    let scope = scope_of(args);
    let project_dir = configured_project_dir_or_exit(&scope);

    let watch_after = should_watch_after_launch(args);

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    if let Err(e) = assert_valid_command_names(&conn, scope.base_dir(), &args.positionals) {
        fail_with(&e);
    }

    match handle_restart(&conn, &project_dir, &args.positionals) {
        Ok(restarted) => {
            if watch_after {
                let exit_after_ms: Option<u64> =
                    args.value("exit-after-ms").and_then(|s| s.parse().ok());
                if let Err(e) =
                    watch_started_services(&conn, &project_dir, &restarted, exit_after_ms)
                {
                    fail_with(&e);
                }
            } else {
                print_logs_hint(&restarted);
            }
            exit(0)
        }
        Err(e) => fail_with(&e),
    }
}

fn cmd_watch(args: &CommandArgs) {
    if candle::run_context::is_run_by_agent() {
        fatal(
            "'watch' blocks and is not available in agent mode. Use 'candle logs' to view process output.",
        );
    }
    let scope = scope_of(args);
    if let Err(e) = scope.require_own_config() {
        fail_with(&e);
    }
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);
    if let Err(e) = assert_valid_command_names(&conn, scope.base_dir(), &args.positionals) {
        fail_with(&e);
    }
    let exit_after_ms: Option<u64> = args.value("exit-after-ms").and_then(|s| s.parse().ok());
    match handle_watch(&conn, scope.base_dir(), &args.positionals, exit_after_ms) {
        Ok(()) => {}
        Err(e) => fail_with(&e),
    }
}

/// `list-ports [names...]` / `list-ports-all`: detect open listening ports for
/// project (or all) processes via lsof and print them as a table, or as JSON
/// with `--json`. Names restrict the output to those services; in a project an
/// unknown name is an error. `list-ports-all` is system-wide and needs no
/// project.
fn cmd_list_ports(args: &CommandArgs, show_all: bool) {
    let scope = scope_of(args);
    if !show_all {
        if let Err(e) = scope.require_own_config() {
            fail_with(&e);
        }
    }
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    let output = match handle_list_ports(&conn, scope.base_dir(), show_all, &args.positionals) {
        Ok(output) => output,
        Err(e) => fail_with(&e),
    };
    if args.has("json") {
        println!("{}", list_ports_output_to_json(&output));
    } else {
        println!("{}", format_list_ports_output(&output));
    }
}

/// `open-browser`: resolve a service (explicit or sole running), open a browser
/// to its lowest listening port.
fn cmd_open_browser(args: &CommandArgs) {
    let scope = scope_of(args);
    let project_dir = configured_project_dir_or_exit(&scope);

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    let service_name = args.positionals.first().map(String::as_str);
    match handle_open_browser(&conn, scope.base_dir(), &project_dir, service_name) {
        Ok(output) => println!("{}", format_open_browser_output(&output)),
        Err(e) => fail_with(&e),
    }
}

/// `find-orphans`: report live tracked processes whose project no longer
/// accounts for them. System-wide, like `kill-all`, so it takes no project.
fn cmd_find_orphans(args: &CommandArgs) {
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    let output = match handle_find_orphans(&conn) {
        Ok(output) => output,
        Err(e) => fail_with(&e),
    };

    if args.has("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("{}", format_find_orphans(&output));
    }
}

/// `erase-database [--force]`: delete candle.db (+ WAL/SHM) from the state dir,
/// refusing while tracked processes are alive unless `--force`.
fn cmd_erase_database(args: &CommandArgs) {
    use candle::commands::erase_database::{
        format_refusal, handle_erase_database_command, EraseOutcome,
    };
    match handle_erase_database_command(args.has("force")) {
        Ok(EraseOutcome::Erased) => {}
        Ok(EraseOutcome::RefusedLiveProcesses(live)) => fatal(format_refusal(&live)),
        Err(e) => fatal(format!("Could not erase database: {e}")),
    }
}

fn run_mcp() -> ! {
    // stdio JSON-RPC server (M8). Blocks until stdin closes, then exits 0.
    candle::mcp::serve_mcp();
}

fn not_implemented(command: &str) -> ! {
    fatal(format!("'{command}' is not implemented"));
}
