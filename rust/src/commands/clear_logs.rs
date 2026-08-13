//! `clear-logs` command handler.
//!
//! Ported from `src/clear-logs-command.ts`. Deletes stored process output for
//! the named command(s) within the project, then cleans up orphaned rows and
//! vacuums the database.

use rusqlite::Connection;

use crate::output;

/// Clear logs for the given command(s) in the project.
///
/// Returns the `rusqlite::Result` so the CLI layer can map a database error to
/// the `console.error` + exit 1 path (matching the TS `catch`).
pub fn handle_clear_logs_command(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
) -> rusqlite::Result<()> {
    output::out(&format!("Clearing logs for project: {project_dir}"));

    let mut cleared_count: usize = 0;

    for command_name in command_names {
        // Clear logs for this specific project directory and command.
        let changes = conn.execute(
            "DELETE FROM process_output WHERE command_name = ?1 AND project_dir = ?2",
            rusqlite::params![command_name, project_dir],
        )?;
        cleared_count += changes;
    }

    if cleared_count > 0 {
        output::out(&format!("\u{2713} Cleared {cleared_count} log entries"));
    } else {
        output::out("- No logs found to clear");
    }

    // Clean up orphaned logs and optimize the database.
    conn.execute(
        "DELETE FROM process_output WHERE (command_name, project_dir) NOT IN (SELECT command_name, project_dir FROM processes)",
        [],
    )?;
    conn.execute("VACUUM", [])?;

    output::out("\nLogs cleared successfully!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};
    use crate::logs::process_logs::{get_process_logs, save_process_log, LogSearchOptions};
    use crate::logs::ProcessLogType;

    #[test]
    fn clears_matching_rows() {
        let dir = temp_db_dir("clear-logs-match");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("alpha")).unwrap();
        save_process_log(&conn, "svc", "/proj", ProcessLogType::Stdout, Some("beta")).unwrap();

        let (_, captured) = output::capture(|| {
            handle_clear_logs_command(&conn, "/proj", &["svc".to_string()]).unwrap();
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "Clearing logs for project: /proj"));
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "\u{2713} Cleared 2 log entries"));
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "\nLogs cleared successfully!"));

        let remaining = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["svc".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(remaining.is_empty());

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_matching_rows_prints_dash_line() {
        let dir = temp_db_dir("clear-logs-none");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) = output::capture(|| {
            handle_clear_logs_command(&conn, "/proj", &["ghost".to_string()]).unwrap();
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "- No logs found to clear"));
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "\nLogs cleared successfully!"));
        assert!(!captured.stdout.iter().any(|l| l.starts_with('\u{2713}')));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_command_names_is_noop() {
        let dir = temp_db_dir("clear-logs-empty-names");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) = output::capture(|| {
            handle_clear_logs_command(&conn, "/proj", &[]).unwrap();
        });

        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "- No logs found to clear"));
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "\nLogs cleared successfully!"));
        assert!(!captured.stdout.iter().any(|l| l.starts_with('\u{2713}')));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
