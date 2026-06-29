// candle — primary CLI binary.
//
// Hand-rolled command dispatch ported from src/main-cli.ts. See help.rs / parser.rs for the help
// text and argument parsing. Command handlers are wired in incrementally per milestone; commands
// not yet ported print a not-implemented notice and exit non-zero.

mod help;
mod parser;

use std::path::PathBuf;
use std::process::exit;

use candle_core::commands::assert_valid_command_names;
use candle_core::commands::list::{format_list_output, handle_list, list_output_to_json};
use candle_core::config::commands::{
    add_server_config, handle_set_config, handle_setup_project, remove_server_config,
    AddServerConfigArgs,
};
use candle_core::config::find_project_dir;
use candle_core::db::cleanup::maybe_run_cleanup;
use candle_core::db::get_database;
use candle_core::doc_files::{self, DocLookupError};
use candle_core::errors::CandleError;
use candle_core::kill::{handle_kill_all, handle_kill_command};
use candle_core::start::{handle_start_command, StartCommandOptions};
use parser::{canonical_command, parse_command_args, CommandArgs};
use rusqlite::Connection;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // --version / -v short-circuits everything.
    if argv.iter().any(|a| a == "--version" || a == "-v") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // No arguments at all → grouped help on stdout.
    if argv.is_empty() {
        println!("{}", help::grouped_help());
        return;
    }

    let help_flag = argv.iter().any(|a| a == "--help" || a == "-h");
    let mcp_flag = argv.iter().any(|a| a == "--mcp");
    let command_token = argv.iter().find(|a| !a.starts_with('-')).cloned();

    // --help / -h → grouped help, or command-specific help when a command is named.
    if help_flag {
        match command_token.as_deref().and_then(canonical_command) {
            Some(cmd) => println!("{}", help::command_help(cmd)),
            None => println!("{}", help::grouped_help()),
        }
        return;
    }

    // `mcp` command or `--mcp` flag → MCP server mode.
    if mcp_flag {
        run_mcp();
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
            eprintln!("Error: Unrecognized command '{command_token}'");
            eprintln!("Run \"candle help\" for available commands.");
            exit(1);
        }
    };

    let cmd_index = argv.iter().position(|a| a == &command_token).unwrap();
    let rest = &argv[cmd_index + 1..];

    if canonical == "help" {
        if let Some(topic) = rest.iter().find(|a| !a.starts_with('-')) {
            eprintln!("Unknown help topic: {topic}");
            exit(1);
        }
        println!("{}", help::grouped_help());
        return;
    }

    if canonical == "mcp" {
        run_mcp();
        return;
    }

    let args = match parse_command_args(canonical, rest) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            exit(1);
        }
    };

    dispatch(canonical, &args);
}

fn dispatch(command: &str, args: &CommandArgs) {
    match command {
        "setup-project" => print_or_exit(handle_setup_project(&cwd()), |e| format!("{e}")),
        "add-service" => cmd_add_service(args),
        "remove-service" => cmd_remove_service(args),
        "set-config" => cmd_set_config(args),
        "list-docs" => cmd_list_docs(),
        "get-doc" => cmd_get_doc(args),
        "kill" => cmd_kill(args),
        "kill-all" => cmd_kill_all(),
        "start" => cmd_start(args, false),
        "check-start" => cmd_start(args, true),
        "list" => cmd_list(args, false),
        "list-all" => cmd_list(args, true),
        // Remaining process-management commands are wired in later milestones.
        _ => not_implemented(command),
    }
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Print a handler's success message, or render its error to stderr and exit 1.
fn print_or_exit<E>(result: Result<String, E>, render: impl FnOnce(&E) -> String) {
    match result {
        Ok(msg) => println!("{msg}"),
        Err(e) => {
            eprintln!("{}", render(&e));
            exit(1);
        }
    }
}

fn cmd_add_service(args: &CommandArgs) {
    let name = match args.positionals.first() {
        Some(n) => n.clone(),
        None => {
            eprintln!("Error: Service name is required");
            exit(1);
        }
    };
    if args.positionals.len() > 1 {
        eprintln!("Error: Cannot use multiple command names for add-service");
        exit(1);
    }
    let shell = match args.value("shell") {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            eprintln!("Missing required argument: shell");
            exit(1);
        }
    };
    let config_args = AddServerConfigArgs {
        name,
        shell,
        root: args.value("root").map(str::to_string),
        enable_stdin: args.has("enable-stdin"),
    };
    print_or_exit(add_server_config(&config_args, &cwd()), |e| {
        format!("Error adding service: {e}")
    });
}

fn cmd_remove_service(args: &CommandArgs) {
    let name = match args.positionals.first() {
        Some(n) => n.clone(),
        None => {
            eprintln!("Error: Service name is required");
            exit(1);
        }
    };
    if args.positionals.len() > 1 {
        eprintln!("Error: Cannot use multiple command names for remove-service");
        exit(1);
    }
    print_or_exit(remove_server_config(&name, &cwd()), |e| {
        format!("Error removing service: {e}")
    });
}

fn cmd_set_config(args: &CommandArgs) {
    let (key, value) = match (args.positionals.first(), args.positionals.get(1)) {
        (Some(k), Some(v)) => (k.clone(), v.clone()),
        _ => {
            eprintln!("Error: set-config requires a <key> and a <value>");
            exit(1);
        }
    };
    print_or_exit(handle_set_config(&key, &value, &cwd()), |e| format!("Error: {e}"));
}

fn cmd_list_docs() {
    println!("Available doc files:\n");
    for doc in doc_files::list_docs() {
        let hint = format!("candle get-doc {}", doc.filename);
        if doc.description.is_empty() {
            println!("  {} ({hint})\n", doc.name);
        } else {
            println!("  {} ({hint}):", doc.name);
            println!("    {}\n", doc.description);
        }
    }
}

fn cmd_get_doc(args: &CommandArgs) {
    let name = args.positionals.first().map(String::as_str).unwrap_or("");
    match doc_files::get_doc(name) {
        Ok(doc) => {
            println!("{}", doc.raw_content);
            println!("\n(File source: docs/{})", doc.filename);
        }
        Err(DocLookupError::NotFound) => {
            eprintln!("Doc file not found: {name}");
            eprintln!("Run with \"list-docs\" command to see available docs.");
            exit(1);
        }
        Err(DocLookupError::Ambiguous(matches)) => {
            eprintln!(
                "Multiple docs match \"{name}\": {}. Please be more specific.",
                matches.join(", ")
            );
            exit(1);
        }
    }
}

/// Open the candle database (resolving the state dir from the environment), or
/// print an error and exit.
fn open_db() -> Connection {
    match get_database(None) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("candle: failed to open database: {e}");
            exit(1);
        }
    }
}

/// Print a CandleError to stderr and exit 1. Mirrors the Node top-level handler,
/// which prints `error.message` for usage errors.
fn fail_with(err: &CandleError) -> ! {
    eprintln!("{err}");
    exit(1);
}

/// `kill` / `stop`: resolve the project dir, validate names, then mark/kill.
fn cmd_kill(args: &CommandArgs) {
    let cwd = cwd();
    let project_dir = match find_project_dir(&cwd) {
        Ok(dir) => dir.display().to_string(),
        Err(e) => fail_with(&e),
    };

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    if let Err(e) = assert_valid_command_names(&conn, &cwd, &args.positionals) {
        fail_with(&e);
    }

    if let Err(e) = handle_kill_command(&conn, &project_dir, &args.positionals, false, false) {
        eprintln!("candle: database error: {e}");
        exit(1);
    }
}

/// `kill-all`: kill every process across every project. No validation, no config.
fn cmd_kill_all() {
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);
    if let Err(e) = handle_kill_all(&conn, false) {
        eprintln!("candle: database error: {e}");
        exit(1);
    }
}

/// `start` / `run` (`check_start = false`) and `check-start` (`check_start =
/// true`): resolve the project dir, then launch the requested service(s).
fn cmd_start(args: &CommandArgs, check_start: bool) {
    let cwd = cwd();
    let project_dir = match find_project_dir(&cwd) {
        Ok(dir) => dir.display().to_string(),
        Err(e) => fail_with(&e),
    };

    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    let opts = StartCommandOptions {
        project_dir,
        command_names: args.positionals.clone(),
        shell: args.value("shell").map(str::to_string),
        root: args.value("root").map(str::to_string),
        enable_stdin: args.has("enable-stdin"),
        check_start,
    };

    match handle_start_command(&conn, opts) {
        // main-cli.ts calls process.exit(0) after start; match that.
        Ok(()) => exit(0),
        Err(e) => fail_with(&e),
    }
}

/// `list` / `ls` (`show_all = false`) and `list-all` (`show_all = true`).
fn cmd_list(args: &CommandArgs, show_all: bool) {
    let cwd = cwd();
    let conn = open_db();
    let _ = maybe_run_cleanup(&conn);

    let output = match handle_list(&conn, &cwd, show_all) {
        Ok(output) => output,
        Err(e) => fail_with(&e),
    };

    if args.has("json") {
        println!("{}", list_output_to_json(&output));
    } else {
        println!("{}", format_list_output(&output));
    }
}

fn run_mcp() {
    // MCP server is ported in milestone M8.
    eprintln!("candle: MCP server mode is not yet implemented in the Rust port");
    exit(1);
}

fn not_implemented(command: &str) -> ! {
    eprintln!("candle: '{command}' is not yet implemented in the Rust port");
    exit(1);
}
