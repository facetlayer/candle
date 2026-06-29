//! CLI command handlers that span multiple subsystems (config + database +
//! output sink). Lower-level, single-subsystem logic lives in its own module
//! (e.g. [`crate::kill`]).

pub mod list;
pub mod logs;
pub mod wait_for_log;

use std::path::Path;

use rusqlite::Connection;

use crate::config::{find_project_dir, get_service_config_by_name};
use crate::db::process_table::find_processes_by_command_name_and_project_dir;
use crate::errors::CandleError;

/// Validate that each name refers to a known service for the project, erroring
/// (as a usage error) on the first that does not.
///
/// Ports `assertValidCommandNames` -> `getServiceInfoByName`: a name is valid if
/// it has any process row in the project (running or killed transient) OR it
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
        let rows = find_processes_by_command_name_and_project_dir(conn, name, &project_dir)
            .map_err(|e| CandleError::ConfigFileError(format!("database error: {e}")))?;
        if !rows.is_empty() {
            continue;
        }

        // Otherwise it must resolve to a configured service.
        get_service_config_by_name(name, Some(cwd))?;
    }

    Ok(())
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

        let err = assert_valid_command_names(&conn, proj.path(), &["ghost".to_string()])
            .unwrap_err();
        assert!(matches!(err, CandleError::MissingServiceWithName { .. }));
        assert!(err.to_string().contains("ghost"));
        assert!(err.is_usage_error());

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
            },
        )
        .unwrap();

        assert!(
            assert_valid_command_names(&conn, proj.path(), &["transient".to_string()]).is_ok()
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&db);
    }
}
