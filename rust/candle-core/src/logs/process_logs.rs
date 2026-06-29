//! Process output (log) storage and retrieval.
//!
//! Ported from `src/logs/processLogs.ts` and `src/logs/buildLogSearchQuery.ts`.
//! Rows are written by the log-collector sidecar and read back by the CLI / MCP
//! server. The `timestamp` column is populated by its SQLite `DEFAULT
//! (strftime('%s','now'))`, exactly like the Node `saveProcessLog`, so it is NOT
//! supplied on insert.

use rusqlite::types::Value;
use rusqlite::{Connection, ToSql};

use crate::logs::log_type::ProcessLogType;

/// A row from the `process_output` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLog {
    pub id: i64,
    pub command_name: String,
    pub project_dir: String,
    pub content: Option<String>,
    pub log_type: i64,
    pub timestamp: i64,
}

/// Search parameters for [`get_process_logs`], mirroring `LogSearchOptions` in
/// `processLogs.ts`. At least one of `project_dir` / `command_names` must be set.
#[derive(Debug, Clone, Default)]
pub struct LogSearchOptions {
    pub project_dir: Option<String>,
    /// If empty, matches all commands within the project.
    pub command_names: Vec<String>,
    pub limit: Option<i64>,
    pub since_timestamp: Option<i64>,
    pub after_log_id: Option<i64>,
}

/// Insert a new process log line.
///
/// `timestamp` is intentionally omitted so the column DEFAULT fills it in (unix
/// seconds), matching `saveProcessLog` in `processLogs.ts`.
pub fn save_process_log(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    log_type: ProcessLogType,
    content: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "insert into process_output(command_name, project_dir, content, log_type) values(?1, ?2, ?3, ?4)",
        rusqlite::params![command_name, project_dir, content, log_type.as_i64()],
    )?;
    Ok(())
}

/// Build the log-search SQL + params, faithfully porting `buildLogSearchQuery`.
///
/// Returns rows in newest-first order (`timestamp desc, id desc`); callers that
/// want chronological order should reverse the result (see [`get_process_logs`]).
fn build_log_search_query(options: &LogSearchOptions) -> (String, Vec<Value>) {
    let mut sql = String::new();
    let mut params: Vec<Value> = Vec::new();

    let has_command_names = !options.command_names.is_empty();

    match (&options.project_dir, has_command_names) {
        (Some(project_dir), true) => {
            if options.command_names.len() == 1 {
                sql.push_str(
                    "select po.* from process_output po where po.project_dir = ? and po.command_name = ?",
                );
                params.push(Value::Text(project_dir.clone()));
                params.push(Value::Text(options.command_names[0].clone()));
            } else {
                let placeholders = vec!["?"; options.command_names.len()].join(", ");
                sql.push_str(&format!(
                    "select po.* from process_output po where po.project_dir = ? and po.command_name in ({placeholders})"
                ));
                params.push(Value::Text(project_dir.clone()));
                for name in &options.command_names {
                    params.push(Value::Text(name.clone()));
                }
            }
        }
        (Some(project_dir), false) => {
            sql.push_str("select po.* from process_output po where po.project_dir = ?");
            params.push(Value::Text(project_dir.clone()));
        }
        (None, true) => {
            if options.command_names.len() == 1 {
                sql.push_str("select po.* from process_output po where po.command_name = ?");
                params.push(Value::Text(options.command_names[0].clone()));
            } else {
                let placeholders = vec!["?"; options.command_names.len()].join(", ");
                sql.push_str(&format!(
                    "select po.* from process_output po where po.command_name in ({placeholders})"
                ));
                for name in &options.command_names {
                    params.push(Value::Text(name.clone()));
                }
            }
        }
        (None, false) => {
            // Caller error; mirrors the JS `throw`. Returns a query that yields
            // nothing rather than panicking.
            sql.push_str("select po.* from process_output po where 1 = 0");
        }
    }

    if let Some(since) = options.since_timestamp {
        sql.push_str(" and po.timestamp > ?");
        params.push(Value::Integer(since));
    }

    if let Some(after) = options.after_log_id {
        sql.push_str(" and po.id > ?");
        params.push(Value::Integer(after));
    }

    sql.push_str(" order by po.timestamp desc, po.id desc");

    if let Some(limit) = options.limit {
        sql.push_str(" limit ?");
        params.push(Value::Integer(limit));
    }

    (sql, params)
}

fn row_to_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProcessLog> {
    Ok(ProcessLog {
        id: row.get("id")?,
        command_name: row.get("command_name")?,
        project_dir: row.get("project_dir")?,
        content: row.get("content")?,
        log_type: row.get("log_type")?,
        timestamp: row.get("timestamp")?,
    })
}

/// Fetch process logs in chronological (oldest-first) order.
///
/// Mirrors `getProcessLogs`: the SQL fetches newest-first (so a `limit` keeps the
/// most recent rows); the result is then reversed into chronological order.
pub fn get_process_logs(
    conn: &Connection,
    options: &LogSearchOptions,
) -> rusqlite::Result<Vec<ProcessLog>> {
    let (sql, params) = build_log_search_query(options);
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();

    let mut stmt = conn.prepare(&sql)?;
    let mut logs: Vec<ProcessLog> = stmt
        .query_map(param_refs.as_slice(), row_to_log)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Newest-first -> chronological.
    logs.reverse();
    Ok(logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};

    #[test]
    fn save_and_fetch_chronological() {
        let dir = temp_db_dir("process-logs");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "api", "/proj", ProcessLogType::ProcessStarted, None).unwrap();
        save_process_log(&conn, "api", "/proj", ProcessLogType::Stdout, Some("line one")).unwrap();
        save_process_log(&conn, "api", "/proj", ProcessLogType::Stdout, Some("line two")).unwrap();

        let logs = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(logs.len(), 3);
        // Chronological: insertion order preserved.
        assert_eq!(logs[0].log_type, ProcessLogType::ProcessStarted.as_i64());
        assert_eq!(logs[0].content, None);
        assert_eq!(logs[1].content, Some("line one".to_string()));
        assert_eq!(logs[2].content, Some("line two".to_string()));
        // Timestamp filled in by DEFAULT.
        assert!(logs[0].timestamp > 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn after_log_id_and_limit() {
        let dir = temp_db_dir("process-logs-filter");
        let conn = get_database(Some(&dir)).unwrap();

        for i in 0..5 {
            save_process_log(&conn, "api", "/proj", ProcessLogType::Stdout, Some(&format!("l{i}")))
                .unwrap();
        }

        // after_log_id = 2 -> ids 3,4,5.
        let after = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                after_log_id: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(after.len(), 3);
        assert!(after.iter().all(|l| l.id > 2));
        // Still chronological.
        assert!(after[0].id < after[2].id);

        // limit keeps the newest N, reversed to chronological.
        let limited = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].content, Some("l3".to_string()));
        assert_eq!(limited[1].content, Some("l4".to_string()));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_dir_scopes_results() {
        let dir = temp_db_dir("process-logs-scope");
        let conn = get_database(Some(&dir)).unwrap();

        save_process_log(&conn, "api", "/proj", ProcessLogType::Stdout, Some("a")).unwrap();
        save_process_log(&conn, "api", "/other", ProcessLogType::Stdout, Some("b")).unwrap();

        let logs = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].content, Some("a".to_string()));

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
