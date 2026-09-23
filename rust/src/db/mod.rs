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

/// Table DDL, matching `src/database/database.ts` exactly (column order, types,
/// nullability, defaults, autoincrement). Run additively/idempotently with
/// `if not exists` so it is safe on every startup.
const TABLE_STATEMENTS: &[(&str, &str)] = &[
    (
        "processes",
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
            root text,
            run_id integer
        )",
    ),
    (
        "process_output",
        "create table if not exists process_output(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            content text,
            log_type integer not null,
            timestamp integer not null default (strftime('%s', 'now')),
            run_id integer
        )",
    ),
    (
        "process_last_cleanup",
        "create table if not exists process_last_cleanup(
           timestamp integer not null
        )",
    ),
    (
        "stdin_messages",
        "create table if not exists stdin_messages(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            data text not null,
            encoding text not null default 'utf8',
            created_at integer not null default (strftime('%s', 'now'))
        )",
    ),
];

/// Index DDL, run after the tables exist (and after any table rebuild, which
/// drops the old table's indexes).
const INDEX_STATEMENTS: &[&str] = &[
    "create index if not exists idx_process_output_command_name on process_output(command_name)",
    "create index if not exists idx_process_output_project_dir on process_output(project_dir)",
    "create index if not exists idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc)",
    "create index if not exists idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id)",
    "create index if not exists idx_process_output_run on process_output(project_dir, command_name, run_id)",
    "create index if not exists idx_process_output_launches on process_output(project_dir, command_name, log_type, id)",
];

/// The run a log row belongs to when its writer didn't say: the latest
/// `process_start_initiated` row at or before it, by id. `row` names the row.
///
/// Every row's `run_id` is the id of its run's `process_start_initiated` row.
/// The monitor stamps its rows explicitly, so rows a previous instance writes
/// after a restart stay in the previous run whatever order they land in. This
/// position-based rule covers only the writers that don't know a run id: the
/// `process_start_initiated` row itself (which gets its own id), a monitor
/// launched by an older candle that is still running, and rows saved before
/// the column existed.
fn run_of_row(row: &str) -> String {
    format!(
        "(select max(p2.id) from process_output p2 \
         where p2.project_dir = {row}.project_dir and p2.command_name = {row}.command_name \
         and p2.log_type = 3 and p2.id <= {row}.id)"
    )
}

/// Assign [`run_of_row`] to every row inserted without a `run_id`.
fn create_assign_run_trigger(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "create trigger if not exists process_output_assign_run \
         after insert on process_output when new.run_id is null begin \
         update process_output set run_id = {} where id = new.id; end",
        run_of_row("new")
    ))
}

/// One-time backfill for rows stored before `run_id` existed.
fn backfill_run_ids(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "update process_output set run_id = {} where run_id is null",
        run_of_row("process_output")
    ))
}

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
///
/// `create table if not exists` leaves an existing table alone, so a database
/// written by a much older candle can lack columns the current code queries.
/// Any table missing columns is rebuilt to the current schema, keeping its rows.
fn run_migration(conn: &Connection) -> rusqlite::Result<()> {
    for (_, statement) in TABLE_STATEMENTS {
        conn.execute_batch(statement)?;
    }
    let mut rebuilt_output = false;
    for (table, statement) in TABLE_STATEMENTS {
        if !missing_columns(conn, table)?.is_empty() {
            rebuild_table(conn, table, statement)?;
            rebuilt_output |= *table == "process_output";
        }
    }
    for statement in INDEX_STATEMENTS {
        conn.execute_batch(statement)?;
    }
    // Dropping a rebuilt table drops its trigger too, so create it after.
    create_assign_run_trigger(conn)?;
    if rebuilt_output {
        backfill_run_ids(conn)?;
    }
    Ok(())
}

/// One column as reported by `PRAGMA table_info`.
#[derive(Debug, Clone)]
struct ColumnInfo {
    name: String,
    col_type: String,
    not_null: bool,
    has_default: bool,
}

fn table_columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<ColumnInfo>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| {
        Ok(ColumnInfo {
            name: row.get(1)?,
            col_type: row.get(2)?,
            not_null: row.get::<_, i64>(3)? != 0,
            has_default: row.get::<_, Option<String>>(4)?.is_some(),
        })
    })?;
    rows.collect()
}

/// Columns of each table in the current schema, read once from an in-memory
/// database built with the same DDL (so the list can't drift from it).
fn expected_columns(table: &str) -> Vec<ColumnInfo> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static EXPECTED: OnceLock<HashMap<String, Vec<ColumnInfo>>> = OnceLock::new();
    EXPECTED
        .get_or_init(|| {
            let mem = Connection::open_in_memory().expect("in-memory sqlite");
            TABLE_STATEMENTS
                .iter()
                .map(|(name, ddl)| {
                    mem.execute_batch(ddl).expect("schema DDL is valid");
                    let cols = table_columns(&mem, name).expect("table_info");
                    (name.to_string(), cols)
                })
                .collect()
        })
        .get(table)
        .cloned()
        .unwrap_or_default()
}

fn missing_columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<ColumnInfo>> {
    let actual = table_columns(conn, table)?;
    Ok(expected_columns(table)
        .into_iter()
        .filter(|c| !actual.iter().any(|a| a.name.eq_ignore_ascii_case(&c.name)))
        .collect())
}

/// Rebuild `table` with the current DDL, copying rows across.
///
/// `ALTER TABLE ... ADD COLUMN` can't add a column whose default is an
/// expression (`strftime(...)`), so the table is recreated instead. Columns the
/// old table lacks take their schema default; a `not null` one with no default
/// gets a zero value so old rows still fit.
fn rebuild_table(conn: &Connection, table: &str, create_statement: &str) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        // Another process may have migrated it while we waited for the lock.
        let missing = missing_columns(conn, table)?;
        if missing.is_empty() {
            return Ok(());
        }
        let actual = table_columns(conn, table)?;
        let expected = expected_columns(table);

        let mut targets = Vec::new();
        let mut sources = Vec::new();
        for col in &expected {
            if actual
                .iter()
                .any(|a| a.name.eq_ignore_ascii_case(&col.name))
            {
                targets.push(col.name.clone());
                sources.push(col.name.clone());
            } else if col.not_null && !col.has_default {
                let zero = if col.col_type.to_ascii_lowercase().contains("text") {
                    "''"
                } else {
                    "0"
                };
                targets.push(col.name.clone());
                sources.push(zero.to_string());
            }
        }

        let old = format!("{table}__candle_migrate_old");
        conn.execute_batch(&format!("ALTER TABLE {table} RENAME TO {old}"))?;
        conn.execute_batch(create_statement)?;
        conn.execute_batch(&format!(
            "INSERT INTO {table} ({}) SELECT {} FROM {old}",
            targets.join(", "),
            sources.join(", ")
        ))?;
        // Dropping the old table drops its indexes; the caller recreates them.
        conn.execute_batch(&format!("DROP TABLE {old}"))?;
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
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
            ("run_id", "INTEGER", 0),
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
    fn run_ids_are_backfilled_and_assigned_by_position() {
        let dir = temp_db_dir("run-id-backfill");
        {
            // The schema just before run_id, with two launches of 'api'.
            let old = Connection::open(dir.join("candle.db")).unwrap();
            old.execute_batch(
                "create table process_output(
                    id integer primary key autoincrement,
                    command_name text not null,
                    project_dir text not null,
                    content text,
                    log_type integer not null,
                    timestamp integer not null default (strftime('%s', 'now'))
                );
                insert into process_output(command_name, project_dir, content, log_type) values
                    ('api', '/proj', 'before any launch', 1),
                    ('api', '/proj', null, 3),
                    ('api', '/proj', 'first run', 1),
                    ('other', '/proj', null, 3),
                    ('api', '/proj', null, 3),
                    ('api', '/proj', 'second run', 1);",
            )
            .unwrap();
        }

        let conn = get_database(Some(&dir)).unwrap();
        let run_of = |content: &str| -> Option<i64> {
            conn.query_row(
                "select run_id from process_output where content = ?1",
                [content],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(run_of("before any launch"), None);
        assert_eq!(run_of("first run"), Some(2));
        assert_eq!(run_of("second run"), Some(5));

        // A writer that doesn't know its run (an older monitor) gets the latest
        // launch by position; one that does keeps its own.
        conn.execute_batch(
            "insert into process_output(command_name, project_dir, content, log_type)
                 values('api', '/proj', 'legacy writer', 1);
             insert into process_output(command_name, project_dir, content, log_type, run_id)
                 values('api', '/proj', 'late row from run 2', 1, 2);",
        )
        .unwrap();
        assert_eq!(run_of("legacy writer"), Some(5));
        assert_eq!(run_of("late row from run 2"), Some(2));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_database_missing_columns_is_upgraded() {
        let dir = temp_db_dir("old-schema");
        {
            // A very old candle.db: no log_collector_pid/killed_at/shell/root on
            // processes, no timestamp on process_output.
            let old = Connection::open(dir.join("candle.db")).unwrap();
            old.execute_batch(
                "create table processes(
                    id integer primary key autoincrement,
                    command_name text not null,
                    project_dir text not null,
                    pid integer not null,
                    start_time integer not null,
                    created_at integer not null default (strftime('%s', 'now'))
                );
                create table process_output(
                    id integer primary key autoincrement,
                    command_name text not null,
                    project_dir text not null,
                    content text,
                    log_type integer not null
                );
                create index idx_process_output_command_name on process_output(command_name);
                insert into processes(command_name, project_dir, pid, start_time) values('api', '/proj', 123, 456);
                insert into process_output(command_name, project_dir, content, log_type) values('api', '/proj', 'hello', 1);",
            )
            .unwrap();
        }

        let conn = get_database(Some(&dir)).unwrap();
        for table in ["processes", "process_output"] {
            assert!(
                missing_columns(&conn, table).unwrap().is_empty(),
                "{table} still missing columns"
            );
        }

        let (name, pid, shell): (String, i64, Option<String>) = conn
            .query_row("select command_name, pid, shell from processes", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), pid, shell), ("api", 123, None));

        let (content, ts): (String, i64) = conn
            .query_row("select content, timestamp from process_output", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(content, "hello");
        assert!(ts > 0, "rebuilt rows take the schema default timestamp");

        // Indexes exist again after the rebuild dropped them.
        let idx: i64 = conn
            .query_row(
                "select count(*) from sqlite_master where type='index' and name='idx_process_output_lookup'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 1);

        // New writes work against the upgraded tables.
        crate::logs::process_logs::save_process_log(
            &conn,
            "api",
            "/proj",
            crate::logs::ProcessLogType::Stdout,
            Some("after"),
        )
        .unwrap();

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
