//! `restart` command handler.
//!
//! Kills the named services (or, with no names, every service in the project),
//! then starts each one again; a service that wasn't running is simply started.
//! Config-defined services are reloaded from `.candle.json` so edits to
//! `shell`/`root` take effect; transient (not-in-config) services reuse the
//! `shell`/`root` captured on the stored DB row, unless `--shell` replaces it.

use std::path::Path;

use rusqlite::Connection;

use crate::config::file::{find_config_file, find_service_by_name, get_all_service_names};
use crate::db::process_table::{
    find_processes_by_command_name_and_project_dir, find_running_processes_by_project_dir,
    ProcessEntry,
};
use crate::errors::CandleError;
use crate::kill::handle_kill_command;
use crate::start::start_each;
use crate::start::start_one_service::{IfRunning, RunOptions};

/// Returns true if the named service has an entry in the project's
/// `.candle.json`. Restart reloads config-defined services from the config
/// file (picking up edits to `shell`/`root`) rather than relaunching with the
/// captured command.
fn is_service_defined_in_config(project_dir: &str, name: &str) -> bool {
    find_config_file(Path::new(project_dir))
        .map(|f| find_service_by_name(&f.config, name).is_some())
        .unwrap_or(false)
}

/// Every service in the project: the configured ones in file order, then any
/// running transient processes that aren't in the config.
fn all_project_services(conn: &Connection, project_dir: &str) -> Result<Vec<String>, CandleError> {
    let mut names = find_config_file(Path::new(project_dir))
        .map(|f| get_all_service_names(&f.config))
        .unwrap_or_default();
    for p in find_running_processes_by_project_dir(conn, project_dir)? {
        if !names.contains(&p.command_name) {
            names.push(p.command_name);
        }
    }
    Ok(names)
}

/// Restart the given command(s), or every service in the project when none are
/// named. `shell`/`root` replace the command of a single transient process.
/// Returns the resolved list of restarted command names.
///
/// Usage errors (nothing to restart, `--shell` with other than one name) are
/// raised before the kill+start work. A failure inside the kill+start loop is
/// returned as `Failed to restart: <msg>`; with several services, the others
/// are still restarted. Either way the CLI prints it to stderr and exits 1, so
/// scripts and CI can tell a restart failed.
pub fn handle_restart(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    shell: Option<String>,
    root: Option<String>,
) -> Result<Vec<String>, CandleError> {
    let names: Vec<String> = if command_names.is_empty() {
        all_project_services(conn, project_dir)?
    } else {
        command_names.to_vec()
    };
    if names.is_empty() {
        return Err(CandleError::UsageError(
            "No services to restart: none are configured in .candle.json and none are running"
                .to_string(),
        ));
    }
    if shell.is_some() && names.len() != 1 {
        return Err(CandleError::UsageError(
            "Exactly one service name is required when using --shell".to_string(),
        ));
    }
    if shell.is_none() && root.is_some() {
        return Err(CandleError::UsageError(
            "--root only applies to transient services started with --shell.".to_string(),
        ));
    }

    let result: Result<(), CandleError> = (|| {
        // Fetch process info for all command names before killing.
        let mut process_info: Vec<(String, Option<ProcessEntry>)> = Vec::new();
        for name in &names {
            let processes =
                find_processes_by_command_name_and_project_dir(conn, name, project_dir)?;
            process_info.push((name.clone(), processes.into_iter().next()));
        }

        // Kill all existing processes (deduped inside handle_kill_command).
        // Services that aren't running are simply started, so don't report them.
        handle_kill_command(conn, project_dir, &names, true, false)?;

        // Restart each service. An explicit --shell wins; otherwise
        // config-defined services pass shell/root as None so start_one_service
        // reloads from .candle.json, and transient processes reuse the
        // captured shell/root.
        start_each(conn, &names, |name| {
            let entry = process_info
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, e)| e.as_ref());
            let (shell, root) = if shell.is_some() {
                (shell.clone(), root.clone())
            } else if is_service_defined_in_config(project_dir, name) {
                (None, None)
            } else {
                match entry {
                    Some(e) => (e.shell.clone(), e.root.clone()),
                    None => (None, None),
                }
            };
            RunOptions {
                command_name: name.to_string(),
                project_dir: project_dir.to_string(),
                shell,
                root,
                enable_stdin: false,
                if_running: IfRunning::Replace,
            }
        })
    })();

    if let Err(e) = result {
        return Err(CandleError::Generic(format!("Failed to restart: {e}")));
    }

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};

    #[test]
    fn empty_names_nothing_to_restart_errors() {
        let dir = temp_db_dir("restart-no-running");
        let conn = get_database(Some(&dir)).unwrap();

        let err = handle_restart(&conn, "/proj", &[], None, None).unwrap_err();
        assert!(matches!(err, CandleError::UsageError(_)));
        assert!(err.to_string().contains("No services to restart"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
