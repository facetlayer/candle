//! CLI command handlers that span multiple subsystems (config + database +
//! output sink). Lower-level, single-subsystem logic lives in its own module
//! (e.g. [`crate::kill`]).

pub mod clear_logs;
pub mod erase_database;
pub mod find_orphans;
pub mod list;
pub mod list_ports;
pub mod logs;
pub mod open_browser;
pub mod restart;
pub mod wait_for_log;
pub mod watch;

use std::path::Path;

use rusqlite::Connection;

use crate::config::{find_project_dir, get_service_config_by_name};
use crate::db::process_table::find_processes_by_command_name_and_project_dir;
use crate::errors::CandleError;
use crate::logs::process_logs::has_logs_for_command;
use crate::project_scope::ProjectScope;

/// Validate that each name refers to a known service for the project, erroring
/// (as a usage error) on the first that does not.
///
/// A name is valid if it has any process row in the project (running or killed transient) OR it
/// resolves to a configured service (exact or loose match). An unknown name
/// yields `MissingServiceWithName` ("No service '<name>' configured for
/// directory: <dir>"), which the CLI prints to stderr before exiting non-zero.
pub fn assert_valid_command_names(
    conn: &Connection,
    cwd: &Path,
    names: &[String],
) -> Result<(), CandleError> {
    if names.is_empty() {
        return Ok(());
    }

    let project_dir = find_project_dir(cwd)?;
    let project_dir = project_dir.display().to_string();

    for name in names {
        // A live or transient process row makes the name valid regardless of config.
        let rows = find_processes_by_command_name_and_project_dir(conn, name, &project_dir)?;
        if !rows.is_empty() {
            continue;
        }

        // Otherwise it must resolve to a configured service.
        get_service_config_by_name(name, Some(cwd))?;
    }

    Ok(())
}

/// Validate names for the commands that read stored logs (`logs`,
/// `wait-for-log`), erroring with `No service '<name>' configured` on the first
/// unknown one.
///
/// A name is known if it has stored logs or a process row in `project_dir`
/// (so a finished transient service still counts), or, when `check_config` is
/// set, if it resolves to a service configured for `config_dir`. Callers clear
/// `check_config` when the project has no config file of its own, e.g. a
/// `--project-dir` that has since been deleted.
pub fn assert_known_service_names(
    conn: &Connection,
    config_dir: &Path,
    project_dir: &str,
    names: &[String],
    check_config: bool,
) -> Result<(), CandleError> {
    for name in names {
        if has_logs_for_command(conn, project_dir, name)? {
            continue;
        }
        let rows = find_processes_by_command_name_and_project_dir(conn, name, project_dir)?;
        if !rows.is_empty() {
            continue;
        }
        let configured = check_config
            && match get_service_config_by_name(name, Some(config_dir)) {
                Ok(_) => true,
                Err(CandleError::MissingServiceWithName { .. })
                | Err(CandleError::MissingSetupFile { .. }) => false,
                Err(e) => return Err(e),
            };
        if !configured {
            return Err(CandleError::unknown_service(name, project_dir));
        }
    }
    Ok(())
}

/// [`assert_known_service_names`] for a command's [`ProjectScope`]: config is
/// read from the scope's base directory, and only consulted when the project
/// has a config file of its own (an explicit `--project-dir` / MCP `projectDir`
/// may name a project that is gone). Shared by the CLI (`logs`, `wait-for-log`,
/// `clear-logs`) and the MCP `GetLogs` tool so they accept the same names.
pub fn assert_known_service_names_in_scope(
    conn: &Connection,
    scope: &ProjectScope,
    project_dir: &str,
    names: &[String],
) -> Result<(), CandleError> {
    let check_config = scope.require_own_config().is_ok();
    assert_known_service_names(conn, scope.base_dir(), project_dir, names, check_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempDir;
    use crate::db::process_table::{create_process_entry, CreateProcessEntry};
    use crate::db::{get_database, temp_db_dir};

    fn write_config(dir: &Path) {
        std::fs::write(
            dir.join(".candle.json"),
            "{\n  \"services\": [ { \"name\": \"echo\", \"shell\": \"x\" } ]\n}",
        )
        .unwrap();
    }

    #[test]
    fn configured_name_is_valid() {
        let proj = TempDir::new();
        write_config(proj.path());
        let db = temp_db_dir("assert-valid-config");
        let conn = get_database(Some(&db)).unwrap();

        assert!(assert_valid_command_names(&conn, proj.path(), &["echo".to_string()]).is_ok());

        drop(conn);
        let _ = std::fs::remove_dir_all(&db);
    }

    #[test]
    fn unknown_name_errors() {
        let proj = TempDir::new();
        write_config(proj.path());
        let db = temp_db_dir("assert-valid-unknown");
        let conn = get_database(Some(&db)).unwrap();

        let err =
            assert_valid_command_names(&conn, proj.path(), &["ghost".to_string()]).unwrap_err();
        assert!(matches!(err, CandleError::MissingServiceWithName { .. }));
        assert!(err.to_string().contains("ghost"));

        drop(conn);
        let _ = std::fs::remove_dir_all(&db);
    }

    #[test]
    fn transient_process_row_makes_name_valid() {
        let proj = TempDir::new();
        write_config(proj.path());
        let db = temp_db_dir("assert-valid-transient");
        let conn = get_database(Some(&db)).unwrap();

        // "transient" is not in config, but a process row exists for it.
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "transient".to_string(),
                project_dir: proj.path().display().to_string(),
                pid: 1234,
                log_collector_pid: None,
                shell: None,
                root: None,
                run_id: None,
                transient: false,
            },
        )
        .unwrap();

        assert!(assert_valid_command_names(&conn, proj.path(), &["transient".to_string()]).is_ok());

        drop(conn);
        let _ = std::fs::remove_dir_all(&db);
    }

    #[test]
    fn known_names_in_scope_accept_config_logs_and_rows() {
        use crate::logs::{save_process_log, ProcessLogType};

        let proj = TempDir::new();
        write_config(proj.path());
        let db = temp_db_dir("assert-known-scope");
        let conn = get_database(Some(&db)).unwrap();
        let project_dir = proj.path().display().to_string();
        let scope = ProjectScope::new(proj.path().to_path_buf(), None);
        let check = |name: &str| {
            assert_known_service_names_in_scope(&conn, &scope, &project_dir, &[name.to_string()])
        };

        // Configured.
        assert!(check("echo").is_ok());

        // A finished transient service: only stored logs remain.
        save_process_log(
            &conn,
            "done",
            &project_dir,
            ProcessLogType::Stdout,
            Some("x"),
        )
        .unwrap();
        assert!(check("done").is_ok());

        // Unknown: the shared full-form error.
        let err = check("ghost").unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("No service 'ghost' configured for directory: {project_dir}")
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&db);
    }
}
