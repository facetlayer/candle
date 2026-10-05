//! CLI handlers combining config, database, and output.

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

/// Reject the first name without a process row or configured match.
/// Killed transient rows also count as known services.
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
        let rows = find_processes_by_command_name_and_project_dir(conn, name, &project_dir)?;
        if !rows.is_empty() {
            continue;
        }

        get_service_config_by_name(name, Some(cwd))?;
    }

    Ok(())
}

/// Validate log-reader names against stored logs, process rows, or config.
/// Skip config checks for projects without their own config, including deleted
/// explicit projects; finished transient services remain valid.
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

/// Validate names using the scope's config only if the project has its own file.
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

        assert!(check("echo").is_ok());

        save_process_log(
            &conn,
            "done",
            &project_dir,
            ProcessLogType::Stdout,
            Some("x"),
        )
        .unwrap();
        assert!(check("done").is_ok());

        let err = check("ghost").unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("No service 'ghost' configured for directory: {project_dir}")
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&db);
    }
}
