//! Periodic database cleanup and log eviction.
//!
//! Cleanup runs at most once per [`CLEANUP_INTERVAL_SECONDS`] and performs, in
//! order: time-based log eviction, stale-process removal, per-service log
//! eviction, removal of `services` rows with no logs left, `VACUUM` plus a WAL
//! truncate, and a single-row update of `process_last_cleanup`.
//!
//! Eviction limits come from each project's `.candle.json` `logEviction` block,
//! resolved per `project_dir` (falling back to [`crate::config::LOG_EVICTION_DEFAULTS`]
//! of 1000 logs / 86400s when no config is found), so limits stay correct when
//! the database holds logs from multiple projects.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::config::{find_config_file, get_log_eviction_config, ResolvedLogEvictionConfig};
use crate::db::process_table::{
    delete_process_entry, find_all_killed_processes, find_all_running_processes,
};
use crate::logs::process_logs::save_run_log;
use crate::logs::ProcessLogType;
use crate::process_alive::is_entry_alive;

/// Minimum seconds between cleanup runs. Matches `CLEANUP_INTERVAL_SECONDS`.
pub const CLEANUP_INTERVAL_SECONDS: i64 = 10 * 60;

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Run cleanup only if more than [`CLEANUP_INTERVAL_SECONDS`] have elapsed since
/// the last run.
pub fn maybe_run_cleanup(conn: &Connection) -> rusqlite::Result<()> {
    let now = now_unix();
    let last_cleanup: Option<i64> = conn
        .query_row("select timestamp from process_last_cleanup", [], |row| {
            row.get(0)
        })
        .ok();

    if let Some(ts) = last_cleanup {
        if now - ts < CLEANUP_INTERVAL_SECONDS {
            return Ok(());
        }
    }

    run_cleanup(conn)
}

/// Delete all but the newest `keep` log rows of one service.
fn evict_service_logs(conn: &Connection, service_id: i64, keep: i64) -> rusqlite::Result<()> {
    // The id threshold: the row at offset = `keep` when sorted newest-first.
    // Everything with id <= that is evicted.
    let cutoff_id: Option<i64> = conn
        .query_row(
            "select id from log_lines where service_id = ?1 \
             order by id desc limit 1 offset ?2",
            params![service_id, keep],
            |row| row.get(0),
        )
        .ok();

    if let Some(cutoff) = cutoff_id {
        conn.execute(
            "delete from log_lines where service_id = ?1 and id <= ?2",
            params![service_id, cutoff],
        )?;
    }
    Ok(())
}

/// Delete all but the newest `keep` log rows of `command_name` in
/// `project_dir`. The monitor calls this while a service floods its output, so
/// the database stays bounded between the periodic cleanups.
pub fn evict_logs_of(
    conn: &Connection,
    project_dir: &str,
    command_name: &str,
    keep: i64,
) -> rusqlite::Result<()> {
    let service_id: Option<i64> = conn
        .query_row(
            "select id from services where project_dir = ?1 and command_name = ?2",
            params![project_dir, command_name],
            |row| row.get(0),
        )
        .ok();
    match service_id {
        Some(service_id) => evict_service_logs(conn, service_id, keep),
        None => Ok(()),
    }
}

/// Resolve the log-eviction config for a project directory, falling back to the
/// defaults on any error (missing/invalid config file).
pub(crate) fn resolve_eviction_config(project_dir: &str) -> ResolvedLogEvictionConfig {
    match find_config_file(Path::new(project_dir)) {
        Ok(found) => get_log_eviction_config(Some(&found.config)),
        Err(_) => get_log_eviction_config(None),
    }
}

/// Perform a full cleanup pass.
pub fn run_cleanup(conn: &Connection) -> rusqlite::Result<()> {
    let now = now_unix();
    let mut config_cache: HashMap<String, ResolvedLogEvictionConfig> = HashMap::new();

    // (1) Time-based eviction: per project_dir, delete logs older than that
    //     project's maxRetentionSeconds.
    let project_dirs: Vec<String> = {
        let mut stmt = conn.prepare("select distinct project_dir from services")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for project_dir in &project_dirs {
        let cfg = *config_cache
            .entry(project_dir.clone())
            .or_insert_with(|| resolve_eviction_config(project_dir));
        let cutoff = now - cfg.max_retention_seconds as i64;
        conn.execute(
            "delete from log_lines where service_id in \
             (select id from services where project_dir = ?1) and timestamp < ?2",
            params![project_dir, cutoff],
        )?;
    }

    // (2) Remove database entries for processes that are no longer alive.
    cleanup_stale_processes(conn)?;

    // (3) Per-service eviction: keep only maxLogsPerService logs per
    //     service. The threshold is per-project, so the over-limit filter is
    //     applied in Rust rather than in the SQL HAVING.
    let services: Vec<(i64, String, i64)> = {
        let mut stmt = conn.prepare(
            "select s.id, s.project_dir, \
             (select count(*) from log_lines l where l.service_id = s.id) as log_count \
             from services s",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (service_id, project_dir, log_count) in services {
        let cfg = *config_cache
            .entry(project_dir.clone())
            .or_insert_with(|| resolve_eviction_config(&project_dir));
        let max_logs = cfg.max_logs_per_service as i64;

        if log_count <= max_logs {
            continue;
        }

        evict_service_logs(conn, service_id, max_logs)?;
    }

    // (4) Forget services with no logs left. A writer that races this recreates
    //     the row (see `insert_log` in `logs/process_logs.rs`).
    conn.execute(
        "delete from services \
         where not exists (select 1 from log_lines l where l.service_id = services.id)",
        [],
    )?;

    // (5) Reclaim space.
    crate::db::reclaim_space(conn)?;

    // (6) Upsert the single-row process_last_cleanup timestamp (update-all, then
    //     insert if the table was empty).
    let updated = conn.execute(
        "update process_last_cleanup set timestamp = ?1",
        params![now],
    )?;
    if updated == 0 {
        conn.execute(
            "insert into process_last_cleanup(timestamp) values(?1)",
            params![now],
        )?;
    }

    Ok(())
}

/// Remove `processes` rows whose underlying OS processes are gone.
///
/// - For each running row (`killed_at is null`): keep it if the log collector OR
///   the service pid is alive; otherwise write a `process_exited` log line and
///   delete the row.
/// - Delete any row that was marked killed (`killed_at` set) but never removed
///   (the collector died before it could clean up).
pub fn cleanup_stale_processes(conn: &Connection) -> rusqlite::Result<()> {
    for proc in find_all_running_processes(conn)? {
        // The monitor is still managing it, or the service itself is alive.
        if is_entry_alive(&proc) {
            continue;
        }

        // Both are dead -> stale entry. The exit belongs to that process's run.
        save_run_log(
            conn,
            proc.run_id,
            &proc.command_name,
            &proc.project_dir,
            ProcessLogType::ProcessExited,
            Some("Process cleaned up (stale entry after restart or crash)"),
        )?;
        delete_process_entry(conn, &proc.command_name, &proc.project_dir, proc.pid)?;
    }

    // Killed-but-not-deleted rows.
    for proc in find_all_killed_processes(conn)? {
        delete_process_entry(conn, &proc.command_name, &proc.project_dir, proc.pid)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::process_table::{
        create_process_entry, find_all_processes, update_process_killed_at, CreateProcessEntry,
    };
    use crate::db::{get_database, temp_db_dir};
    use crate::logs::process_logs::{get_process_logs, save_process_log, LogSearchOptions};

    fn count_logs(conn: &Connection, project_dir: &str, command_name: &str) -> usize {
        get_process_logs(
            conn,
            &LogSearchOptions {
                project_dir: Some(project_dir.to_string()),
                command_names: vec![command_name.to_string()],
                ..Default::default()
            },
        )
        .unwrap()
        .len()
    }

    #[test]
    fn per_service_eviction_keeps_newest_default_limit() {
        let dir = temp_db_dir("cleanup-eviction");
        let conn = get_database(Some(&dir)).unwrap();

        // Insert 1005 rows; default limit is 1000.
        for i in 0..1005 {
            save_process_log(
                &conn,
                "api",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("line {i}")),
            )
            .unwrap();
        }
        assert_eq!(count_logs(&conn, "/proj", "api"), 1005);

        run_cleanup(&conn).unwrap();

        assert_eq!(count_logs(&conn, "/proj", "api"), 1000);
        // The newest line survived; the oldest did not.
        let logs = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(logs.last().unwrap().content, Some("line 1004".to_string()));
        assert_eq!(logs.first().unwrap().content, Some("line 5".to_string()));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn time_based_eviction_removes_old_logs() {
        let dir = temp_db_dir("cleanup-time");
        let conn = get_database(Some(&dir)).unwrap();

        // One ancient row (well past the 86400s default retention) and one fresh.
        let ancient = now_unix() - 90_000;
        conn.execute(
            "insert into process_output(command_name, project_dir, content, log_type, timestamp) values('api','/proj','old',1,?1)",
            params![ancient],
        )
        .unwrap();
        save_process_log(&conn, "api", "/proj", ProcessLogType::Stdout, Some("fresh")).unwrap();

        run_cleanup(&conn).unwrap();

        let logs = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].content, Some("fresh".to_string()));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_cleanup_upserts_single_timestamp_row() {
        let dir = temp_db_dir("cleanup-timestamp");
        let conn = get_database(Some(&dir)).unwrap();

        run_cleanup(&conn).unwrap();
        let count: i64 = conn
            .query_row("select count(*) from process_last_cleanup", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);

        // A second run keeps it single-row (update-in-place).
        run_cleanup(&conn).unwrap();
        let count2: i64 = conn
            .query_row("select count(*) from process_last_cleanup", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count2, 1);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn maybe_run_cleanup_is_gated() {
        let dir = temp_db_dir("cleanup-gate");
        let conn = get_database(Some(&dir)).unwrap();

        // Seed a very recent cleanup timestamp.
        conn.execute(
            "insert into process_last_cleanup(timestamp) values(?1)",
            params![now_unix()],
        )
        .unwrap();

        // Many over-limit logs, but cleanup should be skipped (recent timestamp).
        for i in 0..1005 {
            save_process_log(
                &conn,
                "api",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("{i}")),
            )
            .unwrap();
        }
        maybe_run_cleanup(&conn).unwrap();
        assert_eq!(count_logs(&conn, "/proj", "api"), 1005);

        // Force the timestamp into the past -> cleanup runs.
        conn.execute(
            "update process_last_cleanup set timestamp = ?1",
            params![now_unix() - CLEANUP_INTERVAL_SECONDS - 1],
        )
        .unwrap();
        maybe_run_cleanup(&conn).unwrap();
        assert_eq!(count_logs(&conn, "/proj", "api"), 1000);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_cleanup_removes_dead_and_keeps_alive() {
        let dir = temp_db_dir("cleanup-stale");
        let conn = get_database(Some(&dir)).unwrap();

        let me = std::process::id() as i64;

        // Alive (this process).
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "alive".to_string(),
                project_dir: "/proj".to_string(),
                pid: me,
                log_collector_pid: None,
                shell: None,
                root: None,
                run_id: None,
                transient: false,
            },
        )
        .unwrap();

        // Stale (both pids dead).
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "dead".to_string(),
                project_dir: "/proj".to_string(),
                pid: 2_000_000_000,
                log_collector_pid: Some(2_000_000_001),
                shell: None,
                root: None,
                run_id: None,
                transient: false,
            },
        )
        .unwrap();

        // Killed-but-not-deleted.
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "killed".to_string(),
                project_dir: "/proj".to_string(),
                pid: me,
                log_collector_pid: None,
                shell: None,
                root: None,
                run_id: None,
                transient: false,
            },
        )
        .unwrap();
        update_process_killed_at(&conn, "killed", "/proj", me, now_unix()).unwrap();

        cleanup_stale_processes(&conn).unwrap();

        let remaining = find_all_processes(&conn).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].command_name, "alive");

        // Stale removal logged the exact process_exited message.
        let logs = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["dead".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].log_type, ProcessLogType::ProcessExited.as_i64());
        assert_eq!(
            logs[0].content,
            Some("Process cleaned up (stale entry after restart or crash)".to_string())
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
