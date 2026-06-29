//! `restart` command handler.
//!
//! Ported from `src/restart-command.ts`. Kills the named (or all running)
//! processes for the project, then starts each one again. Config-defined
//! services are reloaded from `.candle.json` so edits to `shell`/`root` take
//! effect; transient (not-in-config) services reuse the `shell`/`root` captured
//! on the stored DB row.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;

use crate::config::file::{find_config_file, find_service_by_name};
use crate::db::process_table::{
    find_processes_by_command_name_and_project_dir, find_running_processes_by_project_dir,
    ProcessEntry,
};
use crate::errors::CandleError;
use crate::kill::handle_kill_command;
use crate::output;
use crate::start::start_one_service::{start_one_service, RunOptions};

fn db_err(e: rusqlite::Error) -> CandleError {
    CandleError::Generic(format!("database error: {e}"))
}

/// Returns true if the named service has an entry in the project's
/// `.candle.json`. Mirrors `isServiceDefinedInConfig`: restart reloads
/// config-defined services from the config file (picking up edits to
/// `shell`/`root`) rather than relaunching with the captured command.
fn is_service_defined_in_config(project_dir: &str, name: &str) -> bool {
    find_config_file(Path::new(project_dir))
        .map(|f| find_service_by_name(&f.config, name).is_some())
        .unwrap_or(false)
}

/// Restart the given command(s), or all running processes when none are named.
///
/// The empty-names "No running processes" usage error is raised before the
/// kill+start work, so it propagates to the caller (the CLI maps it to stderr +
/// exit 1). Failures inside the kill+start loop are caught and printed as
/// `Failed to restart: <msg>`, and the handler still returns `Ok`.
pub fn handle_restart(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
) -> Result<(), CandleError> {
    // Resolve the list of command names to restart.
    let names: Vec<String> = if command_names.is_empty() {
        let running = find_running_processes_by_project_dir(conn, project_dir).map_err(db_err)?;
        if running.is_empty() {
            return Err(CandleError::UsageError(
                "No running processes found in this project to restart".to_string(),
            ));
        }
        // Deduplicate command names (preserving first-seen order) to avoid
        // killing the same service multiple times.
        let mut seen: HashSet<&str> = HashSet::new();
        let mut deduped: Vec<String> = Vec::new();
        for p in &running {
            if seen.insert(p.command_name.as_str()) {
                deduped.push(p.command_name.clone());
            }
        }
        deduped
    } else {
        command_names.to_vec()
    };

    // Everything below mirrors the TS try/catch: on error, print
    // "Failed to restart: <msg>" to stderr and return Ok overall.
    let result: Result<(), CandleError> = (|| {
        // Fetch process info for all command names before killing.
        let mut process_info: Vec<(&String, Option<ProcessEntry>)> = Vec::new();
        for name in &names {
            let processes =
                find_processes_by_command_name_and_project_dir(conn, name, project_dir)
                    .map_err(db_err)?;
            process_info.push((name, processes.into_iter().next()));
        }

        // Kill all existing processes (deduped inside handle_kill_command).
        handle_kill_command(conn, project_dir, &names, false, false).map_err(db_err)?;

        // Restart each service. For config-defined services, pass shell/root as
        // None so start_one_service reloads from .candle.json. Only transient
        // processes reuse the captured shell/root.
        for (name, entry) in &process_info {
            let (shell, root) = if is_service_defined_in_config(project_dir, name) {
                (None, None)
            } else {
                match entry {
                    Some(e) => (e.shell.clone(), e.root.clone()),
                    None => (None, None),
                }
            };

            start_one_service(
                conn,
                RunOptions {
                    command_name: (*name).clone(),
                    project_dir: project_dir.to_string(),
                    shell,
                    root,
                    enable_stdin: false,
                    check_start: false,
                },
            )?;
        }

        Ok(())
    })();

    if let Err(e) = result {
        output::err(&format!("Failed to restart: {e}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};

    #[test]
    fn empty_names_no_running_is_usage_error() {
        let dir = temp_db_dir("restart-no-running");
        let conn = get_database(Some(&dir)).unwrap();

        let err = handle_restart(&conn, "/proj", &[]).unwrap_err();
        assert!(err.is_usage_error());
        assert!(err.to_string().contains("No running processes"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
