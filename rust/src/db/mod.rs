//! SQLite database bootstrap.
//!
//! Ported from `src/database/database.ts`. Opens `candle.db` in the resolved
//! state directory, sets WAL + busy_timeout pragmas, and runs an additive,
//! idempotent schema migration.
//!
//! Unlike the Node implementation, this does NOT keep a process-wide singleton
//! connection; a fresh connection is opened on each call. This is simpler and
//! correct for the multi-process usage candle relies on (each connection sets
//! WAL + busy_timeout). The schema DDL is byte-parity with the Node version so
//! the Vitest suite can open the same DB with raw SQL.

pub mod cleanup;
pub mod process_table;
pub mod stdin_messages;

use rusqlite::Connection;
use std::path::Path;

use crate::dirs::get_state_directory;

/// Schema DDL, matching `src/database/database.ts` exactly (column order, types,
/// nullability, defaults, autoincrement). Run additively/idempotently with
/// `if not exists` so it is safe on every startup.
const SCHEMA_STATEMENTS: &[&str] = &[
    "create table if not exists processes(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            pid integer not null,
            log_collector_pid integer,
            start_time integer not null,
            created_at integer not null default (strftime('%s', 'now')),
            killed_at integer,
            shell text,
            root text
        )",
    "create table if not exists process_output(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            content text,
            log_type integer not null,
            timestamp integer not null default (strftime('%s', 'now'))
        )",
    "create table if not exists process_last_cleanup(
           timestamp integer not null
        )",
    "create table if not exists stdin_messages(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            data text not null,
            encoding text not null default 'utf8',
            created_at integer not null default (strftime('%s', 'now'))
        )",
    "create index if not exists idx_process_output_command_name on process_output(command_name)",
    "create index if not exists idx_process_output_project_dir on process_output(project_dir)",
    "create index if not exists idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc)",
    "create index if not exists idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id)",
];

/// Open a connection to the candle database.
///
/// Resolves the state directory (using `override_dir` if given, else
/// [`get_state_directory`]), creates it recursively, opens `candle.db`, sets the
/// WAL journal mode and a 30s busy timeout, then runs the additive schema
/// migration.
pub fn get_database(override_dir: Option<&Path>) -> rusqlite::Result<Connection> {
    let state_dir = match override_dir {
        Some(dir) => dir.to_path_buf(),
        None => get_state_directory(),
    };

    std::fs::create_dir_all(&state_dir).map_err(|e| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(format!(
                "failed to create state dir {}: {e}",
                state_dir.display()
            )),
        )
    })?;

    open_database_at(&state_dir.join("candle.db"))
}

/// Open a connection to a candle database file at an explicit path.
///
/// Used by monitor mode, which is handed an absolute path to the
/// `candle.db` file (rather than a state directory). Opens the file, sets the
/// WAL journal mode and 30s busy timeout, then runs the additive, idempotent
/// schema migration so the tables are guaranteed to exist.
pub fn open_database_at(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;

    // WAL + busy_timeout are mandatory for multi-process concurrency. journal_mode
    // returns a row ("wal"); query_row consumes it.
    conn.query_row("PRAGMA journal_mode=WAL", [], |_row| Ok(()))?;
    conn.pragma_update(None, "busy_timeout", 30000)?;

    run_migration(&conn)?;

    Ok(conn)
}

/// Run the additive, idempotent schema migration on an open connection.
fn run_migration(conn: &Connection) -> rusqlite::Result<()> {
    for statement in SCHEMA_STATEMENTS {
        conn.execute_batch(statement)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn temp_db_dir(label: &str) -> std::path::PathBuf {
    let unique = format!(
        "candle-db-test-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_creates_schema() {
        let dir = temp_db_dir("schema");
        let conn = get_database(Some(&dir)).unwrap();

        // All 4 tables exist.
        for table in [
            "processes",
            "process_output",
            "process_last_cleanup",
            "stdin_messages",
        ] {
            let count: i64 = conn
                .query_row(
                    "select count(*) from sqlite_master where type='table' and name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} should exist");
        }

        // All 4 indexes exist.
        for index in [
            "idx_process_output_command_name",
            "idx_process_output_project_dir",
            "idx_process_output_lookup",
            "idx_stdin_messages_lookup",
        ] {
            let count: i64 = conn
                .query_row(
                    "select count(*) from sqlite_master where type='index' and name=?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "index {index} should exist");
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn processes_columns_match_spec() {
        let dir = temp_db_dir("columns");
        let conn = get_database(Some(&dir)).unwrap();

        // (name, type, notnull) for each column via PRAGMA table_info.
        let cols: Vec<(String, String, i64)> = {
            let mut stmt = conn.prepare("PRAGMA table_info(processes)").unwrap();
            let collected = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            collected
        };

        let expected = vec![
            ("id", "INTEGER", 0),
            ("command_name", "TEXT", 1),
            ("project_dir", "TEXT", 1),
            ("pid", "INTEGER", 1),
            ("log_collector_pid", "INTEGER", 0),
            ("start_time", "INTEGER", 1),
            ("created_at", "INTEGER", 1),
            ("killed_at", "INTEGER", 0),
            ("shell", "TEXT", 0),
            ("root", "TEXT", 0),
        ];
        assert_eq!(cols.len(), expected.len());
        for (actual, exp) in cols.iter().zip(expected.iter()) {
            assert_eq!(actual.0, exp.0);
            assert_eq!(actual.1.to_uppercase(), exp.1);
            assert_eq!(actual.2, exp.2, "notnull mismatch for {}", exp.0);
        }

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_mode_is_wal() {
        let dir = temp_db_dir("wal");
        let conn = get_database(Some(&dir)).unwrap();

        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = temp_db_dir("idempotent");
        let conn = get_database(Some(&dir)).unwrap();
        // Re-running migration on the same connection must not error.
        run_migration(&conn).unwrap();
        // Opening a second time (reuses existing file) must also succeed.
        drop(conn);
        let conn2 = get_database(Some(&dir)).unwrap();
        drop(conn2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
