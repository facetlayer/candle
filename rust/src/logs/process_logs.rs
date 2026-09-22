//! Process output (log) storage and retrieval.
//!
//! Ported from `src/logs/processLogs.ts` and `src/logs/buildLogSearchQuery.ts`.
//! Rows are written by the monitor process and read back by the CLI / MCP
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
    /// Only rows with `id >= min_log_id`.
    pub min_log_id: Option<i64>,
    /// Only rows of these `log_type`s. Empty matches every type.
    pub log_types: Vec<i64>,
    /// Drop rows older than each command's latest `process_start_initiated`,
    /// i.e. rows from a previous run.
    pub latest_launch_only: bool,
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
    let (scope, mut params) = scope_clause(options);
    let mut sql = format!("select po.* from process_output po where {scope}");

    if let Some(since) = options.since_timestamp {
        sql.push_str(" and po.timestamp > ?");
        params.push(Value::Integer(since));
    }

    if let Some(after) = options.after_log_id {
        sql.push_str(" and po.id > ?");
        params.push(Value::Integer(after));
    }

    if let Some(min_id) = options.min_log_id {
        sql.push_str(" and po.id >= ?");
        params.push(Value::Integer(min_id));
    }

    push_log_type_filter(&mut sql, &mut params, &options.log_types);

    if options.latest_launch_only {
        push_latest_launch_filter(&mut sql, &mut params);
    }

    sql.push_str(" order by po.timestamp desc, po.id desc");

    if let Some(limit) = options.limit {
        sql.push_str(" limit ?");
        params.push(Value::Integer(limit));
    }

    (sql, params)
}

/// The project/command part of a `where` clause over `process_output po`.
fn scope_clause(options: &LogSearchOptions) -> (String, Vec<Value>) {
    let mut params: Vec<Value> = Vec::new();
    let names_clause = |params: &mut Vec<Value>| {
        if options.command_names.len() == 1 {
            params.push(Value::Text(options.command_names[0].clone()));
            "po.command_name = ?".to_string()
        } else {
            let placeholders = vec!["?"; options.command_names.len()].join(", ");
            for name in &options.command_names {
                params.push(Value::Text(name.clone()));
            }
            format!("po.command_name in ({placeholders})")
        }
    };

    let clause = match (&options.project_dir, options.command_names.is_empty()) {
        (Some(project_dir), false) => {
            params.push(Value::Text(project_dir.clone()));
            let names = names_clause(&mut params);
            format!("po.project_dir = ? and {names}")
        }
        (Some(project_dir), true) => {
            params.push(Value::Text(project_dir.clone()));
            "po.project_dir = ?".to_string()
        }
        (None, false) => names_clause(&mut params),
        // Caller error; mirrors the JS `throw`. Yields nothing rather than panicking.
        (None, true) => "1 = 0".to_string(),
    };
    (clause, params)
}

/// Keep only rows at or after the row's command's latest launch marker.
fn push_latest_launch_filter(sql: &mut String, params: &mut Vec<Value>) {
    sql.push_str(
        " and po.id >= coalesce((select max(p2.id) from process_output p2 \
         where p2.project_dir = po.project_dir and p2.command_name = po.command_name \
         and p2.log_type = ?), 0)",
    );
    params.push(Value::Integer(
        ProcessLogType::ProcessStartInitiated.as_i64(),
    ));
}

fn push_log_type_filter(sql: &mut String, params: &mut Vec<Value>, log_types: &[i64]) {
    if log_types.is_empty() {
        return;
    }
    let placeholders = vec!["?"; log_types.len()].join(", ");
    sql.push_str(&format!(" and po.log_type in ({placeholders})"));
    for t in log_types {
        params.push(Value::Integer(*t));
    }
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

/// Result of [`get_process_logs_with_eviction_info`], mirroring `ProcessLogResult`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessLogResult {
    /// Logs in chronological (oldest-first) order.
    pub logs: Vec<ProcessLog>,
    /// True if older logs exist beyond the requested `limit` (i.e. were truncated).
    pub logs_were_evicted: bool,
}

/// Fetch process logs in chronological (oldest-first) order.
///
/// Mirrors `getProcessLogs`: the SQL fetches newest-first (so a `limit` keeps the
/// most recent rows); the result is then reversed into chronological order.
pub fn get_process_logs(
    conn: &Connection,
    options: &LogSearchOptions,
) -> rusqlite::Result<Vec<ProcessLog>> {
    Ok(get_process_logs_with_eviction_info(conn, options)?.logs)
}

/// Fetch process logs plus a flag indicating whether older logs were evicted.
///
/// Mirrors `getProcessLogsWithEvictionInfo`: when a `limit` is set and we got at
/// least that many rows, re-run the same (limitless) query wrapped in a
/// `count(*)` subquery; if the total exceeds what we returned, older logs were
/// truncated.
pub fn get_process_logs_with_eviction_info(
    conn: &Connection,
    options: &LogSearchOptions,
) -> rusqlite::Result<ProcessLogResult> {
    let (sql, params) = build_log_search_query(options);
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();

    let mut stmt = conn.prepare(&sql)?;
    let mut logs: Vec<ProcessLog> = stmt
        .query_map(param_refs.as_slice(), row_to_log)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut logs_were_evicted = false;
    if let Some(limit) = options.limit {
        if logs.len() as i64 >= limit {
            let count_options = LogSearchOptions {
                limit: None,
                ..options.clone()
            };
            let (inner_sql, count_params) = build_log_search_query(&count_options);
            let count_sql = format!("select count(*) as total from ({inner_sql})");
            let count_refs: Vec<&dyn ToSql> =
                count_params.iter().map(|v| v as &dyn ToSql).collect();
            let total: i64 = conn.query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))?;
            if total > logs.len() as i64 {
                logs_were_evicted = true;
            }
        }
    }

    // Newest-first -> chronological.
    logs.reverse();
    Ok(ProcessLogResult {
        logs,
        logs_were_evicted,
    })
}

/// Log types that `candle logs` prints. The launch markers
/// (`process_start_initiated`, `process_started`) render as nothing.
fn printable_log_types() -> Vec<i64> {
    vec![
        ProcessLogType::Stdout.as_i64(),
        ProcessLogType::Stderr.as_i64(),
        ProcessLogType::ProcessStartFailed.as_i64(),
        ProcessLogType::ProcessExited.as_i64(),
    ]
}

/// Result of [`get_log_tail`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogTail {
    /// Chronological rows: the newest `limit` printable rows, plus the launch
    /// markers a [`LatestExecutionLogFilter`](crate::log_filters::LatestExecutionLogFilter)
    /// needs to tell this launch's rows from the previous one's.
    pub logs: Vec<ProcessLog>,
    /// Whether printable rows from the *latest* launch were left out by `limit`.
    /// Rows from a previous launch are excluded up front and never count.
    pub truncated: bool,
}

/// Fetch the last `limit` printable log rows, for `candle logs --count`.
///
/// The limit applies only to printable rows from each command's latest run.
/// Counting marker rows against it made `--count 3` print two lines whenever
/// `process_started`, which the monitor writes after the first output, fell
/// inside the window; counting a previous run's rows did the same after a
/// restart.
pub fn get_log_tail(
    conn: &Connection,
    options: &LogSearchOptions,
    limit: i64,
) -> rusqlite::Result<LogTail> {
    let printable = LogSearchOptions {
        limit: Some(limit),
        log_types: printable_log_types(),
        latest_launch_only: true,
        ..options.clone()
    };
    let mut logs = get_process_logs(conn, &printable)?;
    let Some(window_min_id) = logs.iter().map(|l| l.id).min() else {
        return Ok(LogTail::default());
    };

    // Latest launch boundary per command, even if it predates the window.
    let (scope, mut params) = scope_clause(options);
    let mut boundary_sql =
        format!("select max(po.id) from process_output po where {scope} and po.log_type = ?");
    params.push(Value::Integer(
        ProcessLogType::ProcessStartInitiated.as_i64(),
    ));
    if let Some(after) = options.after_log_id {
        boundary_sql.push_str(" and po.id > ?");
        params.push(Value::Integer(after));
    }
    boundary_sql.push_str(" group by po.command_name");
    let refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
    let mut stmt = conn.prepare(&boundary_sql)?;
    let oldest_boundary: Option<i64> = stmt
        .query_map(refs.as_slice(), |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .min();

    let markers = LogSearchOptions {
        limit: None,
        min_log_id: Some(oldest_boundary.map_or(window_min_id, |b| b.min(window_min_id))),
        log_types: vec![
            ProcessLogType::ProcessStartInitiated.as_i64(),
            ProcessLogType::ProcessStarted.as_i64(),
        ],
        ..options.clone()
    };
    logs.extend(get_process_logs(conn, &markers)?);
    logs.sort_by_key(|l| l.id);

    // Were printable rows from the latest launch cut off by the limit?
    let (scope, mut params) = scope_clause(options);
    let mut count_sql =
        format!("select count(*) from process_output po where {scope} and po.id < ?");
    params.push(Value::Integer(window_min_id));
    if let Some(after) = options.after_log_id {
        count_sql.push_str(" and po.id > ?");
        params.push(Value::Integer(after));
    }
    push_log_type_filter(&mut count_sql, &mut params, &printable_log_types());
    push_latest_launch_filter(&mut count_sql, &mut params);
    let refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
    let hidden: i64 = conn.query_row(&count_sql, refs.as_slice(), |row| row.get(0))?;

    Ok(LogTail {
        logs,
        truncated: hidden > 0,
    })
}

/// The names of every command with stored logs in `project_dir` (restricted to
/// rows after `after_log_id` when given), sorted by name.
pub fn command_names_with_logs(
    conn: &Connection,
    project_dir: &str,
    after_log_id: Option<i64>,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "select distinct command_name from process_output \
         where project_dir = ?1 and id > ?2 order by command_name",
    )?;
    let names = stmt
        .query_map(
            rusqlite::params![project_dir, after_log_id.unwrap_or(i64::MIN)],
            |row| row.get::<_, String>(0),
        )?
        .collect();
    names
}

/// Whether any logs are stored for `command_name` in `project_dir`.
pub fn has_logs_for_command(
    conn: &Connection,
    project_dir: &str,
    command_name: &str,
) -> rusqlite::Result<bool> {
    conn.query_row(
        "select exists(select 1 from process_output where project_dir = ?1 and command_name = ?2)",
        rusqlite::params![project_dir, command_name],
        |row| row.get(0),
    )
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
        save_process_log(
            &conn,
            "api",
            "/proj",
            ProcessLogType::Stdout,
            Some("line one"),
        )
        .unwrap();
        save_process_log(
            &conn,
            "api",
            "/proj",
            ProcessLogType::Stdout,
            Some("line two"),
        )
        .unwrap();

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
            save_process_log(
                &conn,
                "api",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("l{i}")),
            )
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
    fn eviction_flag_set_when_more_logs_exist() {
        let dir = temp_db_dir("process-logs-eviction");
        let conn = get_database(Some(&dir)).unwrap();

        for i in 0..5 {
            save_process_log(
                &conn,
                "api",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("l{i}")),
            )
            .unwrap();
        }

        // limit 2 with 5 rows present -> eviction detected.
        let result = get_process_logs_with_eviction_info(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.logs.len(), 2);
        assert!(result.logs_were_evicted);

        // limit covering everything -> no eviction.
        let result = get_process_logs_with_eviction_info(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".to_string()),
                command_names: vec!["api".to_string()],
                limit: Some(100),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.logs.len(), 5);
        assert!(!result.logs_were_evicted);

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
