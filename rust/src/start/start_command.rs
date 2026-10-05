//! Start configured or transient services sequentially.

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

/// Start services in order, continuing after failures. Return a single error
/// unchanged; for multiple services, print each failure and return a summary.
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

/// Start services and return their names after startup is confirmed.
pub fn handle_start_command(
    conn: &Connection,
    opts: StartCommandOptions,
) -> Result<Vec<String>, CandleError> {
    let mut command_names = opts.command_names.clone();

    if opts.shell.is_none() {
        command_names = resolve_command_names_or_all(Path::new(&opts.project_dir), &command_names)?;
    }

    // --root applies only to transient services. Validate names first so typos
    // receive the unknown-service error.
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
