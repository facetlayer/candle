//! SQLite connections with WAL, busy timeout, and idempotent schema migration.

pub mod cleanup;
pub mod process_table;
pub mod stdin_messages;

use rusqlite::Connection;
use std::path::Path;

use crate::dirs::get_state_directory;

/// Idempotent table DDL. Logs use service ids to avoid repeating project paths;
/// PROCESS_OUTPUT_VIEW exposes the legacy columns.
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
            run_id integer,
            transient integer,
            pid_identity integer,
            monitor_identity integer,
            leader_exited integer
        )",
    ),
    (
        "services",
        "create table if not exists services(
            id integer primary key,
            project_dir text not null,
            command_name text not null,
            unique(project_dir, command_name)
        )",
    ),
    (
        "log_lines",
        "create table if not exists log_lines(
            id integer primary key autoincrement,
            service_id integer not null,
            run_id integer,
            log_type integer not null,
            timestamp integer not null default (strftime('%s', 'now')),
            content text
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

/// Indexes recreated after table rebuilds. Service and service/run indexes
/// support id cursors, latest-run queries, and eviction.
const INDEX_STATEMENTS: &[&str] = &[
    "create index if not exists idx_log_lines_service on log_lines(service_id)",
    "create index if not exists idx_log_lines_run on log_lines(service_id, run_id)",
    "create index if not exists idx_stdin_messages_lookup on stdin_messages(project_dir, command_name, id)",
];

/// Atomically assign each launch marker its own row id as run_id.
const LAUNCH_RUN_TRIGGER: &str = "create trigger if not exists log_lines_launch_run \
     after insert on log_lines when new.log_type = 3 begin \
     update log_lines set run_id = new.id where id = new.id; end";

/// Latest run for writers without an explicit run id, or NULL before launch.
/// `service` is an SQL expression for services.id. Monitors stamp runs explicitly
/// so late output stays with its original launch.
pub(crate) fn latest_run_of(service: &str) -> String {
    format!("(select max(lr.run_id) from log_lines lr where lr.service_id = {service})")
}

/// Legacy log view supporting reads and inserts by older running monitors.
const PROCESS_OUTPUT_VIEW: &str = "create view if not exists process_output as \
     select l.id, s.command_name, s.project_dir, l.content, l.log_type, l.timestamp, l.run_id \
     from log_lines l join services s on s.id = l.service_id";

fn process_output_view_triggers() -> [String; 2] {
    [
        format!(
            "create trigger if not exists process_output_insert \
             instead of insert on process_output begin \
             insert or ignore into services(project_dir, command_name) \
                 values(new.project_dir, new.command_name); \
             insert into log_lines(id, service_id, run_id, log_type, timestamp, content) \
                 select new.id, s.id, coalesce(new.run_id, {}), new.log_type, \
                 coalesce(new.timestamp, strftime('%s', 'now')), new.content \
                 from services s \
                 where s.project_dir = new.project_dir and s.command_name = new.command_name; \
             end",
            latest_run_of("s.id")
        ),
        "create trigger if not exists process_output_delete \
         instead of delete on process_output begin \
         delete from log_lines where id = old.id; end"
            .to_string(),
    ]
}

/// Open candle.db in the resolved or overridden state directory and migrate it.
pub fn get_database(override_dir: Option<&Path>) -> rusqlite::Result<Connection> {
    let state_dir = match override_dir {
        Some(dir) => dir.to_path_buf(),
        None => get_state_directory(),
    };

    create_private_dir(&state_dir).map_err(|e| {
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

/// Create a private state directory; leave existing permissions unchanged.
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Create or tighten private database and sidecar permissions; logs may contain
/// secrets. SQLite inherits database permissions for new sidecars. Best-effort:
/// open errors surface through SQLite.
#[cfg(unix)]
fn restrict_database_permissions(db_path: &Path) {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    // Opening and closing an existing file would release this process's POSIX
    // locks, including those held by other SQLite connections.
    let _ = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(db_path);

    for suffix in ["", "-wal", "-shm"] {
        let mut path = db_path.as_os_str().to_owned();
        path.push(suffix);
        let path = Path::new(&path);
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o077 != 0 {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
}

#[cfg(not(unix))]
fn restrict_database_permissions(_db_path: &Path) {}

/// Cap WAL size after checkpoints; otherwise it retains its peak size.
const WAL_SIZE_LIMIT_BYTES: i64 = 4 * 1024 * 1024;

/// Open and migrate an explicit database path supplied to monitor mode.
pub fn open_database_at(db_path: &Path) -> rusqlite::Result<Connection> {
    restrict_database_permissions(db_path);
    let conn = Connection::open(db_path)?;

    // WAL and busy_timeout allow concurrent CLI and monitor connections.
    conn.query_row("PRAGMA journal_mode=WAL", [], |_row| Ok(()))?;
    conn.pragma_update(None, "busy_timeout", 30000)?;
    // Keep trigger statement journals in memory to avoid slow temp-file writes.
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.query_row(
        &format!("PRAGMA journal_size_limit={WAL_SIZE_LIMIT_BYTES}"),
        [],
        |_row| Ok(()),
    )?;

    run_migration(&conn)?;

    Ok(conn)
}

/// Migrate legacy schemas, preserving rows. Rebuild tables missing columns
/// and replace the old process_output table with the compatibility view.
fn run_migration(conn: &Connection) -> rusqlite::Result<()> {
    for (_, statement) in TABLE_STATEMENTS {
        conn.execute_batch(statement)?;
    }
    for (table, statement) in TABLE_STATEMENTS {
        if !missing_columns(conn, table)?.is_empty() {
            rebuild_table(conn, table, statement)?;
        }
    }
    if process_output_is_table(conn)? {
        migrate_process_output(conn)?;
    }
    for statement in INDEX_STATEMENTS {
        conn.execute_batch(statement)?;
    }
    create_log_views_and_triggers(conn)
}

fn create_log_views_and_triggers(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(LAUNCH_RUN_TRIGGER)?;
    conn.execute_batch(PROCESS_OUTPUT_VIEW)?;
    for trigger in process_output_view_triggers() {
        conn.execute_batch(&trigger)?;
    }
    Ok(())
}

fn process_output_is_table(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "select exists(select 1 from sqlite_master where type = 'table' and name = 'process_output')",
        [],
        |row| row.get(0),
    )
}

/// Atomically move legacy logs to services/log_lines and install the view.
/// Preserve row ids and the highest issued id for run ids and cursors. Missing
/// runs are assigned by launch position; missing timestamps use migration time.
fn migrate_process_output(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        // Another process may have migrated it while we waited for the lock.
        if !process_output_is_table(conn)? {
            return Ok(());
        }
        let has = |name: &str| -> rusqlite::Result<bool> {
            Ok(table_columns(conn, "process_output")?
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(name)))
        };
        let run_id = if has("run_id")? {
            "po.run_id".to_string()
        } else {
            conn.execute_batch(
                "create index if not exists idx_process_output_launches \
                 on process_output(project_dir, command_name, log_type, id)",
            )?;
            "(select max(p2.id) from process_output p2 \
              where p2.project_dir = po.project_dir and p2.command_name = po.command_name \
              and p2.log_type = 3 and p2.id <= po.id)"
                .to_string()
        };
        let timestamp = if has("timestamp")? {
            "po.timestamp"
        } else {
            "strftime('%s', 'now')"
        };

        conn.execute_batch(&format!(
            "insert or ignore into services(project_dir, command_name) \
                 select distinct project_dir, command_name from process_output;
             insert into log_lines(id, service_id, run_id, log_type, timestamp, content) \
                 select po.id, s.id, {run_id}, po.log_type, {timestamp}, po.content \
                 from process_output po join services s \
                 on s.project_dir = po.project_dir and s.command_name = po.command_name \
                 order by po.id;"
        ))?;

        // Preserve the highest issued id, including deleted rows.
        let old_seq: Option<i64> = conn
            .query_row(
                "select seq from sqlite_sequence where name = 'process_output'",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(old_seq) = old_seq {
            let updated = conn.execute(
                "update sqlite_sequence set seq = max(seq, ?1) where name = 'log_lines'",
                [old_seq],
            )?;
            if updated == 0 {
                conn.execute(
                    "insert into sqlite_sequence(name, seq) values('log_lines', ?1)",
                    [old_seq],
                )?;
            }
        }

        conn.execute_batch("drop table process_output")?;
        create_log_views_and_triggers(conn)
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Reclaim space with VACUUM and best-effort WAL truncation. Active readers
/// can hold the WAL open; journal_size_limit trims it later.
pub fn reclaim_space(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("VACUUM")?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))
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

/// Derive current columns from DDL so migration metadata cannot drift.
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

/// Rebuild using current DDL: ALTER cannot add expression defaults. Copy old
/// columns, applying defaults or zero values for missing required columns.
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

    #[cfg(unix)]
    #[test]
    fn state_dir_and_database_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        let dir = temp_db_dir("perms").join("state");
        let conn = get_database(Some(&dir)).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("candle.db")), 0o600);
        assert_eq!(mode(&dir.join("candle.db-wal")), 0o600);
        drop(conn);

        // A database left world-readable by an older version is tightened.
        let db_path = dir.join("candle.db");
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let _conn = get_database(Some(&dir)).unwrap();
        assert_eq!(mode(&db_path), 0o600);
    }

    #[test]
    fn opens_and_creates_schema() {
        let dir = temp_db_dir("schema");
        let conn = get_database(Some(&dir)).unwrap();

        for table in [
            "processes",
            "services",
            "log_lines",
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

        for index in [
            "idx_log_lines_service",
            "idx_log_lines_run",
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

        let kind: String = conn
            .query_row(
                "select type from sqlite_master where name = 'process_output'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kind, "view");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn processes_columns_match_spec() {
        let dir = temp_db_dir("columns");
        let conn = get_database(Some(&dir)).unwrap();

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
            ("transient", "INTEGER", 0),
            ("pid_identity", "INTEGER", 0),
            ("monitor_identity", "INTEGER", 0),
            ("leader_exited", "INTEGER", 0),
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
        assert!(missing_columns(&conn, "processes").unwrap().is_empty());
        assert!(!process_output_is_table(&conn).unwrap());

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

        let idx: i64 = conn
            .query_row(
                "select count(*) from sqlite_master where type='index' and name like 'idx_process_output%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 0);

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
        run_migration(&conn).unwrap();
        drop(conn);
        let conn2 = get_database(Some(&dir)).unwrap();
        drop(conn2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Legacy process_output schema, including indexes and run trigger.
    const LEGACY_PROCESS_OUTPUT: &str = "
        create table process_output(
            id integer primary key autoincrement,
            command_name text not null,
            project_dir text not null,
            content text,
            log_type integer not null,
            timestamp integer not null default (strftime('%s', 'now')),
            run_id integer
        );
        create index idx_process_output_command_name on process_output(command_name);
        create index idx_process_output_project_dir on process_output(project_dir);
        create index idx_process_output_lookup on process_output(project_dir, command_name, timestamp desc, id desc);
        create index idx_process_output_run on process_output(project_dir, command_name, run_id);
        create index idx_process_output_launches on process_output(project_dir, command_name, log_type, id);
        create trigger process_output_assign_run after insert on process_output when new.run_id is null begin
            update process_output set run_id = (select max(p2.id) from process_output p2
                where p2.project_dir = new.project_dir and p2.command_name = new.command_name
                and p2.log_type = 3 and p2.id <= new.id) where id = new.id;
        end;";

    #[test]
    fn process_output_table_moves_into_log_lines() {
        use crate::logs::process_logs::{
            get_log_tail, latest_run_ids, start_run, LogSearchOptions,
        };

        let dir = temp_db_dir("log-lines-migration");
        {
            let old = Connection::open(dir.join("candle.db")).unwrap();
            old.execute_batch(LEGACY_PROCESS_OUTPUT).unwrap();
            old.execute_batch(
                "insert into process_output(command_name, project_dir, content, log_type, timestamp) values
                    ('api', '/proj', 'before any launch', 1, 1000),
                    ('api', '/proj', null, 3, 1001),
                    ('api', '/proj', 'first run', 1, 1002),
                    ('web', '/proj', null, 3, 1003),
                    ('api', '/proj', null, 3, 1004),
                    ('api', '/proj', 'second run', 2, 1005),
                    ('api', '/other', 'other project', 1, 1006),
                    ('api', '/proj', 'cleared', 1, 1007);
                 delete from process_output where content = 'cleared';",
            )
            .unwrap();
        }

        let conn = get_database(Some(&dir)).unwrap();
        assert!(!process_output_is_table(&conn).unwrap());

        type MigratedRow = (i64, String, String, Option<String>, i64, i64, Option<i64>);
        let rows: Vec<MigratedRow> = {
            let mut stmt = conn
                .prepare(
                    "select l.id, s.project_dir, s.command_name, l.content, l.log_type, l.timestamp, l.run_id \
                     from log_lines l join services s on s.id = l.service_id order by l.id",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            rows
        };
        let text = |s: &str| Some(s.to_string());
        let (p, a, w, o) = ("/proj", "api", "web", "/other");
        let expected = vec![
            (1, p, a, text("before any launch"), 1, 1000, None),
            (2, p, a, None, 3, 1001, Some(2)),
            (3, p, a, text("first run"), 1, 1002, Some(2)),
            (4, p, w, None, 3, 1003, Some(4)),
            (5, p, a, None, 3, 1004, Some(5)),
            (6, p, a, text("second run"), 2, 1005, Some(5)),
            (7, o, a, text("other project"), 1, 1006, None),
        ];
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(id, p, c, t, ty, ts, run)| (id, p.to_string(), c.to_string(), t, ty, ts, run))
            .collect();
        assert_eq!(rows, expected);

        let services: i64 = conn
            .query_row("select count(*) from services", [], |r| r.get(0))
            .unwrap();
        assert_eq!(services, 3);

        let tail = get_log_tail(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                ..Default::default()
            },
            100,
        )
        .unwrap();
        let contents: Vec<_> = tail.logs.iter().filter_map(|l| l.content.clone()).collect();
        assert_eq!(contents, vec!["second run"]);

        // Preserve the sequence above deleted id 8 for run ids and cursors.
        let run = start_run(&conn, "api", "/proj").unwrap();
        assert_eq!(run, 9);
        assert_eq!(
            latest_run_ids(&conn, "/proj", &["api".to_string()]).unwrap(),
            vec![("api".to_string(), 9)]
        );

        drop(conn);
        let conn = get_database(Some(&dir)).unwrap();
        let count: i64 = conn
            .query_row("select count(*) from log_lines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 8);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_monitors_still_write_through_the_process_output_view() {
        use crate::logs::process_logs::{get_log_tail, start_run, LogSearchOptions};

        let dir = temp_db_dir("log-lines-view");
        let conn = get_database(Some(&dir)).unwrap();
        let run = start_run(&conn, "api", "/proj").unwrap();

        // Exercise older monitor inserts, with and without run ids.
        let legacy_insert =
            "insert into process_output(command_name, project_dir, content, log_type, run_id) \
                             values(?1, ?2, ?3, ?4, ?5)";
        conn.execute(
            legacy_insert,
            rusqlite::params!["api", "/proj", "stamped", 1, run],
        )
        .unwrap();
        conn.execute(
            legacy_insert,
            rusqlite::params!["api", "/proj", "unstamped", 2, None::<i64>],
        )
        .unwrap();
        conn.execute(
            legacy_insert,
            rusqlite::params!["new", "/proj", "first line", 1, None::<i64>],
        )
        .unwrap();

        let runs: Vec<(String, Option<i64>)> = {
            let mut stmt = conn
                .prepare("select content, run_id from process_output where content is not null order by id")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            rows
        };
        assert_eq!(
            runs,
            vec![
                ("stamped".to_string(), Some(run)),
                ("unstamped".to_string(), Some(run)),
                ("first line".to_string(), None),
            ]
        );

        let tail = get_log_tail(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                ..Default::default()
            },
            100,
        )
        .unwrap();
        assert_eq!(tail.logs.len(), 2);

        conn.execute("delete from process_output where content = 'stamped'", [])
            .unwrap();
        let left: i64 = conn
            .query_row(
                "select count(*) from log_lines where content = 'stamped'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_line_costs_little_more_than_its_text() {
        use crate::logs::process_logs::{save_run_log, start_run};
        use crate::logs::ProcessLogType;

        let dir = temp_db_dir("log-lines-size");
        let conn = get_database(Some(&dir)).unwrap();
        // A long project path used to be repeated in every row and five indexes.
        let project = format!(
            "/Users/someone/src/{}/worktrees/feature-branch",
            "x".repeat(60)
        );
        let run = start_run(&conn, "web", &project).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        let lines = 20_000;
        for i in 1..=lines {
            save_run_log(
                &conn,
                Some(run),
                "web",
                &project,
                ProcessLogType::Stdout,
                Some(&i.to_string()),
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
        reclaim_space(&conn).unwrap();

        let bytes: i64 = conn
            .query_row(
                "select page_count * page_size from pragma_page_count, pragma_page_size",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // About 40 bytes per short line; the old schema exceeded 600 with this path.
        let per_line = bytes / lines;
        assert!(per_line < 50, "{per_line} bytes per log line");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
