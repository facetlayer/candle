//! The `start` command handler.
//!
//! Resolves which services to start (all configured ones when none are named),
//! enforces the transient `--shell` rules, and starts each service sequentially.
//! Services that are already running are left alone.

use std::path::Path;

use rusqlite::Connection;

use crate::config::{get_service_config_by_name, resolve_command_names_or_all};
use crate::errors::CandleError;
use crate::output;
use crate::start::start_one_service::{start_one_service, IfRunning, RunOptions};

/// Options for [`handle_start_command`].
#[derive(Debug, Clone)]
pub struct StartCommandOptions {
    pub project_dir: String,
    pub command_names: Vec<String>,
    pub shell: Option<String>,
    pub root: Option<String>,
    pub enable_stdin: bool,
}

/// Start each named service in order, continuing past failures so one broken
/// service doesn't keep the rest from starting. With a single name its error is
/// returned as-is; with several, each failure is printed as it happens and a
/// summary error naming the failed services is returned at the end.
pub fn start_each(
    conn: &Connection,
    names: &[String],
    mut run_options: impl FnMut(&str) -> RunOptions,
) -> Result<(), CandleError> {
    if let [name] = names {
        start_one_service(conn, run_options(name))?;
        return Ok(());
    }

    let mut failed: Vec<&str> = Vec::new();
    for name in names {
        if let Err(e) = start_one_service(conn, run_options(name)) {
            output::err(&format!("Error: {e}"));
            failed.push(name);
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    Err(CandleError::Generic(format!(
        "{} of {} services failed to start: {}",
        failed.len(),
        names.len(),
        failed.join(", ")
    )))
}

/// Start one or more services and return the started service names once each
/// has reported a start result.
pub fn handle_start_command(
    conn: &Connection,
    opts: StartCommandOptions,
) -> Result<Vec<String>, CandleError> {
    let mut command_names = opts.command_names.clone();

    // With no --shell, default to all configured services when none are named.
    if opts.shell.is_none() {
        command_names = resolve_command_names_or_all(Path::new(&opts.project_dir), &command_names)?;
    }

    // --root sets the directory of a transient service. A configured service's
    // directory comes from its `root` in the config, so rather than silently
    // drop the flag, reject it. Unknown names are reported first, so a typo
    // gets the "No service configured" error rather than this one.
    if opts.shell.is_none() && opts.root.is_some() {
        for name in &command_names {
            get_service_config_by_name(name, Some(Path::new(&opts.project_dir)))?;
        }
        return Err(CandleError::UsageError(format!(
            "--root only applies to transient services started with --shell. \
             To change where '{}' runs, set its \"root\" in .candle.json.",
            command_names.join("', '")
        )));
    }

    // Transient: exactly one name, with the provided shell/root/enable-stdin.
    if let Some(shell) = &opts.shell {
        if command_names.len() != 1 {
            return Err(CandleError::UsageError(
                "Exactly one service name is required when using --shell".to_string(),
            ));
        }
        start_one_service(
            conn,
            RunOptions {
                command_name: command_names[0].clone(),
                project_dir: opts.project_dir.clone(),
                shell: Some(shell.clone()),
                root: opts.root.clone(),
                enable_stdin: opts.enable_stdin,
                if_running: IfRunning::Skip,
            },
        )?;
        return Ok(command_names);
    }

    // Configured: start each resolved name sequentially. Transient flags are not
    // forwarded in this branch.
    start_each(conn, &command_names, |name| RunOptions {
        command_name: name.to_string(),
        project_dir: opts.project_dir.clone(),
        shell: None,
        root: None,
        enable_stdin: false,
        if_running: IfRunning::Skip,
    })?;

    Ok(command_names)
}
