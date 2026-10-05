//! Restart services, reloading configured commands and reusing transient commands.

use std::path::Path;

use rusqlite::Connection;

use crate::config::file::{find_config_file, find_service_by_name, get_all_service_names};
use crate::db::process_table::{
    find_processes_by_command_name_and_project_dir, find_running_processes_by_project_dir,
    ProcessEntry,
};
use crate::errors::CandleError;
use crate::kill::handle_kill_command;
use crate::start::service_lock::{self, ServiceStartLock};
use crate::start::start_each;
use crate::start::start_one_service::{IfRunning, RunOptions};

/// Whether the service is configured and should be reloaded on restart.
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

/// Lock services in sorted order to avoid deadlocks between restarts.
fn acquire_start_locks(
    project_dir: &str,
    names: &[String],
) -> Result<Vec<ServiceStartLock>, CandleError> {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted.dedup();
    sorted
        .into_iter()
        .map(|name| {
            service_lock::acquire(project_dir, name)
                .map_err(|e| CandleError::Generic(format!("Failed to acquire start lock: {e}")))
        })
        .collect()
}

/// Restart named services, or all project services; return their names.
/// Validate usage before killing. Continue after individual failures and
/// return an error if any restart fails.
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
        // Capture transient commands before killing their process rows.
        let mut process_info: Vec<(String, Option<ProcessEntry>)> = Vec::new();
        for name in &names {
            let processes =
                find_processes_by_command_name_and_project_dir(conn, name, project_dir)?;
            process_info.push((name.clone(), processes.into_iter().next()));
        }

        // Hold start locks so concurrent restarts finish launching before this kill.
        {
            let _start_locks = acquire_start_locks(project_dir, &names)?;
            handle_kill_command(conn, project_dir, &names, true, false)?;
        }

        // Explicit shell wins; otherwise reload config or reuse the transient command.
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
