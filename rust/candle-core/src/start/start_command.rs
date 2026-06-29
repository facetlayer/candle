//! The `start` / `check-start` command handler.
//!
//! Ports `handleStartCommand` from `src/start-command.ts`: resolve which
//! services to start (all configured ones when none are named), enforce the
//! transient `--shell` rules, and start each service sequentially.

use std::path::Path;

use rusqlite::Connection;

use crate::config::resolve_command_names_or_all;
use crate::errors::CandleError;
use crate::start::start_one_service::{start_one_service, RunOptions};

/// Options for [`handle_start_command`], mirroring Node's `StartOptions`.
#[derive(Debug, Clone)]
pub struct StartCommandOptions {
    pub project_dir: String,
    pub command_names: Vec<String>,
    pub shell: Option<String>,
    pub root: Option<String>,
    pub enable_stdin: bool,
    pub check_start: bool,
}

/// Start one or more services and return once each has reported a start result.
pub fn handle_start_command(
    conn: &Connection,
    opts: StartCommandOptions,
) -> Result<(), CandleError> {
    let mut command_names = opts.command_names.clone();

    // With no --shell, default to all configured services when none are named.
    if opts.shell.is_none() {
        command_names = resolve_command_names_or_all(Path::new(&opts.project_dir), &command_names)?;
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
                check_start: opts.check_start,
            },
        )?;
        return Ok(());
    }

    // Configured: start each resolved name sequentially. Transient flags are not
    // forwarded in this branch.
    for name in command_names {
        start_one_service(
            conn,
            RunOptions {
                command_name: name,
                project_dir: opts.project_dir.clone(),
                shell: None,
                root: None,
                enable_stdin: false,
                check_start: opts.check_start,
            },
        )?;
    }

    Ok(())
}
