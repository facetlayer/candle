//! `processes` table CRUD.
//!
//! Ported from `src/database/processTable.ts`. Updates and deletes are keyed on
//! `(command_name, project_dir, pid)` exactly as the Node code does (the real PK
//! column `id` is exposed but not used as the mutation key).

use rusqlite::{params, Connection};
use std::time::{SystemTime, UNIX_EPOCH};

/// A row from the `processes` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    pub id: i64,
    pub command_name: String,
    pub project_dir: String,
    pub pid: i64,
    pub log_collector_pid: Option<i64>,
    pub start_time: i64,
    pub created_at: i64,
    pub killed_at: Option<i64>,
    pub shell: Option<String>,
    pub root: Option<String>,
}

/// Input for [`create_process_entry`].
#[derive(Debug, Clone)]
pub struct CreateProcessEntry {
    pub command_name: String,
    pub project_dir: String,
    pub pid: i64,
    pub log_collector_pid: Option<i64>,
    pub shell: Option<String>,
    pub root: Option<String>,
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProcessEntry> {
    Ok(ProcessEntry {
        id: row.get(0)?,
        command_name: row.get(1)?,
        project_dir: row.get(2)?,
        pid: row.get(3)?,
        log_collector_pid: row.get(4)?,
        start_time: row.get(5)?,
        created_at: row.get(6)?,
        killed_at: row.get(7)?,
        shell: row.get(8)?,
        root: row.get(9)?,
    })
}

const SELECT_COLS: &str = "id, command_name, project_dir, pid, log_collector_pid, start_time, created_at, killed_at, shell, root";

/// Insert a new process row. Sets `start_time` to the current unix seconds and
/// leaves `created_at`/`killed_at` to default/NULL. Returns the new row id.
pub fn create_process_entry(conn: &Connection, entry: &CreateProcessEntry) -> rusqlite::Result<i64> {
    conn.execute(
        "insert into processes (command_name, project_dir, pid, start_time, log_collector_pid, shell, root) \
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.command_name,
            entry.project_dir,
            entry.pid,
            now_unix_seconds(),
            entry.log_collector_pid,
            entry.shell,
            entry.root,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Mark a process row as killed at the given unix timestamp.
pub fn update_process_killed_at(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    pid: i64,
    killed_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "update processes set killed_at = ?1 where command_name = ?2 and project_dir = ?3 and pid = ?4",
        params![killed_at, command_name, project_dir, pid],
    )?;
    Ok(())
}

/// Delete a process row keyed on `(command_name, project_dir, pid)`.
pub fn delete_process_entry(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    pid: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "delete from processes where command_name = ?1 and project_dir = ?2 and pid = ?3",
        params![command_name, project_dir, pid],
    )?;
    Ok(())
}

fn query_entries(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> rusqlite::Result<Vec<ProcessEntry>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, row_to_entry)?;
    rows.collect()
}

pub fn find_processes_by_command_name_and_project_dir(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
) -> rusqlite::Result<Vec<ProcessEntry>> {
    query_entries(
        conn,
        &format!("select {SELECT_COLS} from processes where command_name = ?1 and project_dir = ?2"),
        params![command_name, project_dir],
    )
}

pub fn find_processes_by_project_dir(
    conn: &Connection,
    project_dir: &str,
) -> rusqlite::Result<Vec<ProcessEntry>> {
    query_entries(
        conn,
        &format!("select {SELECT_COLS} from processes where project_dir = ?1"),
        params![project_dir],
    )
}

pub fn find_running_processes_by_project_dir(
    conn: &Connection,
    project_dir: &str,
) -> rusqlite::Result<Vec<ProcessEntry>> {
    query_entries(
        conn,
        &format!(
            "select {SELECT_COLS} from processes where project_dir = ?1 and killed_at is null"
        ),
        params![project_dir],
    )
}

pub fn find_all_processes(conn: &Connection) -> rusqlite::Result<Vec<ProcessEntry>> {
    query_entries(conn, &format!("select {SELECT_COLS} from processes"), params![])
}

pub fn find_all_running_processes(conn: &Connection) -> rusqlite::Result<Vec<ProcessEntry>> {
    query_entries(
        conn,
        &format!("select {SELECT_COLS} from processes where killed_at is null"),
        params![],
    )
}

pub fn find_all_killed_processes(conn: &Connection) -> rusqlite::Result<Vec<ProcessEntry>> {
    query_entries(
        conn,
        &format!("select {SELECT_COLS} from processes where killed_at is not null"),
        params![],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};

    fn sample(command_name: &str, pid: i64) -> CreateProcessEntry {
        CreateProcessEntry {
            command_name: command_name.to_string(),
            project_dir: "/proj".to_string(),
            pid,
            log_collector_pid: Some(pid + 1000),
            shell: Some("npm run dev".to_string()),
            root: None,
        }
    }

    #[test]
    fn create_find_update_delete() {
        let dir = temp_db_dir("process-crud");
        let conn = get_database(Some(&dir)).unwrap();

        let id = create_process_entry(&conn, &sample("api", 100)).unwrap();
        assert!(id > 0);

        let found =
            find_processes_by_command_name_and_project_dir(&conn, "api", "/proj").unwrap();
        assert_eq!(found.len(), 1);
        let entry = &found[0];
        assert_eq!(entry.command_name, "api");
        assert_eq!(entry.pid, 100);
        assert_eq!(entry.log_collector_pid, Some(1100));
        assert_eq!(entry.shell, Some("npm run dev".to_string()));
        assert_eq!(entry.root, None); // None -> stored as NULL
        assert_eq!(entry.killed_at, None);
        assert!(entry.start_time > 0);
        assert!(entry.created_at > 0);

        // Running query sees it.
        assert_eq!(find_all_running_processes(&conn).unwrap().len(), 1);
        assert_eq!(find_running_processes_by_project_dir(&conn, "/proj").unwrap().len(), 1);

        // Mark killed.
        update_process_killed_at(&conn, "api", "/proj", 100, 12345).unwrap();
        let after = find_processes_by_command_name_and_project_dir(&conn, "api", "/proj").unwrap();
        assert_eq!(after[0].killed_at, Some(12345));
        assert_eq!(find_all_running_processes(&conn).unwrap().len(), 0);
        assert_eq!(find_all_killed_processes(&conn).unwrap().len(), 1);

        // Delete keyed on (command_name, project_dir, pid).
        delete_process_entry(&conn, "api", "/proj", 100).unwrap();
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_by_project_dir_scopes_results() {
        let dir = temp_db_dir("process-by-dir");
        let conn = get_database(Some(&dir)).unwrap();

        create_process_entry(&conn, &sample("api", 1)).unwrap();
        create_process_entry(&conn, &sample("worker", 2)).unwrap();
        let mut other = sample("api", 3);
        other.project_dir = "/other".to_string();
        create_process_entry(&conn, &other).unwrap();

        assert_eq!(find_processes_by_project_dir(&conn, "/proj").unwrap().len(), 2);
        assert_eq!(find_processes_by_project_dir(&conn, "/other").unwrap().len(), 1);
        assert_eq!(find_all_processes(&conn).unwrap().len(), 3);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
