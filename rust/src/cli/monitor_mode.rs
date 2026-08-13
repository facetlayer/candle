//! Argument handling for `candle --monitor`.
//!
//! Monitor mode is how the `candle` binary re-invokes itself to supervise one
//! service subprocess (see [`crate::monitor`]). It is an internal entry point —
//! the CLI launcher normally hands it a single JSON [`MonitorLaunchInfo`] object
//! on stdin — but it also accepts explicit flags, which is handy for debugging:
//!
//! ```text
//! candle --monitor --command-name api --project-dir . --shell 'npm run dev'
//! ```

use std::io::Read;
use std::path::PathBuf;
use std::process::exit;

use crate::dirs::candle_db_path;
use crate::monitor::MonitorLaunchInfo;

/// Read the launch info, run the supervision loop, and exit with the service's
/// exit code. Never returns.
pub fn run_monitor_mode(args: &[String]) -> ! {
    let launch_info = match launch_info_from_flags(args) {
        Some(info) => info,
        // No flags beyond `--monitor` → read a single JSON launch-info object
        // from stdin (to EOF). This is the launcher's handshake.
        None => read_launch_info_from_stdin(),
    };

    exit(crate::monitor::run(launch_info).unwrap_or(0));
}

fn read_launch_info_from_stdin() -> MonitorLaunchInfo {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("Error: failed to read launch info from stdin");
        exit(1);
    }
    serde_json::from_str(input.trim()).unwrap_or_else(|e| {
        eprintln!("Error: failed to parse launch info from stdin: {e}");
        exit(1);
    })
}

/// Read the value for `flag`: either the inline `--flag=value` form, or the next
/// argument (advancing `i` past it).
fn take_value(args: &[String], i: &mut usize, inline: &Option<String>, flag: &str) -> String {
    if let Some(v) = inline {
        return v.clone();
    }
    *i += 1;
    match args.get(*i) {
        Some(v) => v.clone(),
        None => {
            eprintln!("Error: {flag} needs a value");
            exit(1);
        }
    }
}

/// Parse the flag form. Returns `None` when no flags other than `--monitor` were
/// given, meaning the caller should fall back to the stdin handshake.
fn launch_info_from_flags(args: &[String]) -> Option<MonitorLaunchInfo> {
    let mut command_name = None;
    let mut project_dir = None;
    let mut shell = None;
    let mut root = None;
    let mut database_path = None;
    let mut enable_stdin = false;
    let mut saw_flag = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        // Accept `--flag value` and `--flag=value`.
        let (name, inline_value) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (arg.as_str(), None),
        };

        match name {
            "--monitor" => {}
            "--command-name" => {
                command_name = Some(take_value(args, &mut i, &inline_value, name));
                saw_flag = true;
            }
            "--project-dir" => {
                project_dir = Some(take_value(args, &mut i, &inline_value, name));
                saw_flag = true;
            }
            "--shell" => {
                shell = Some(take_value(args, &mut i, &inline_value, name));
                saw_flag = true;
            }
            "--root" => {
                root = Some(take_value(args, &mut i, &inline_value, name));
                saw_flag = true;
            }
            "--database-path" => {
                database_path = Some(take_value(args, &mut i, &inline_value, name));
                saw_flag = true;
            }
            "--enable-stdin" => {
                enable_stdin = true;
                saw_flag = true;
            }
            other => {
                eprintln!("Error: unrecognized option for --monitor: {other}");
                exit(1);
            }
        }
        i += 1;
    }

    if !saw_flag {
        return None;
    }

    let (command_name, project_dir, shell) = match (command_name, project_dir, shell) {
        (Some(c), Some(p), Some(s)) => (c, p, s),
        _ => {
            eprintln!("Error: --command-name, --project-dir, and --shell are required");
            exit(1);
        }
    };

    // Resolve projectDir to an absolute path (the launcher always passes one).
    let project_dir = std::path::absolute(&project_dir)
        .unwrap_or_else(|_| PathBuf::from(&project_dir))
        .to_string_lossy()
        .into_owned();

    Some(MonitorLaunchInfo {
        command_name,
        project_dir,
        shell,
        root,
        enable_stdin,
        database_path: database_path.map(PathBuf::from).unwrap_or_else(candle_db_path),
    })
}
