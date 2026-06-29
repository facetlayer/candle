//! Thin log-collector binary.
//!
//! Parses the launch info (either a single JSON object on stdin, or CLI flags)
//! and hands off to [`candle_core::log_collector::monitor::run`], which contains
//! the entire supervision lifecycle. All DB / log / cleanup logic lives in
//! candle-core.

use std::io::Read;
use std::path::PathBuf;
use std::process::exit;

use candle_core::dirs::candle_db_path;
use candle_core::log_collector::{monitor, LogCollectorLaunchInfo};

use clap::Parser;

/// CLI flags, used when arguments are passed (the launcher uses stdin JSON).
#[derive(Parser, Debug)]
#[command(name = "log-collector")]
struct Args {
    #[arg(long)]
    command_name: Option<String>,
    #[arg(long)]
    project_dir: Option<String>,
    #[arg(long)]
    shell: Option<String>,
    #[arg(long)]
    root: Option<String>,
    #[arg(long)]
    enable_stdin: bool,
    #[arg(long)]
    database_path: Option<String>,
}

fn launch_info_from_args(args: Args) -> LogCollectorLaunchInfo {
    let (command_name, project_dir, shell) =
        match (args.command_name, args.project_dir, args.shell) {
            (Some(c), Some(p), Some(s)) => (c, p, s),
            _ => {
                eprintln!("Error: --command-name, --project-dir, and --shell are required");
                exit(1);
            }
        };

    // Resolve projectDir to an absolute path (mirrors Path.resolve).
    let project_dir = std::path::absolute(&project_dir)
        .unwrap_or_else(|_| PathBuf::from(&project_dir))
        .to_string_lossy()
        .into_owned();

    let database_path = args
        .database_path
        .map(PathBuf::from)
        .unwrap_or_else(candle_db_path);

    LogCollectorLaunchInfo {
        command_name,
        project_dir,
        shell,
        root: args.root,
        enable_stdin: args.enable_stdin,
        database_path,
    }
}

fn get_launch_info() -> LogCollectorLaunchInfo {
    // No args -> read a single JSON launch-info object from stdin (to EOF).
    if std::env::args().len() <= 1 {
        let mut input = String::new();
        if std::io::stdin().read_to_string(&mut input).is_err() {
            eprintln!("Error: failed to read launch info from stdin");
            exit(1);
        }
        return serde_json::from_str(input.trim()).unwrap_or_else(|e| {
            eprintln!("Error: failed to parse launch info from stdin: {e}");
            exit(1);
        });
    }

    launch_info_from_args(Args::parse())
}

fn main() {
    let launch_info = get_launch_info();
    let code = monitor::run(launch_info);
    exit(code.unwrap_or(0));
}
