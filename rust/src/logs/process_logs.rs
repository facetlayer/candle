//! Process output (log) storage and retrieval.
//!
//! Rows are written by the monitor process and read back by the CLI / MCP
//! server. The `timestamp` column is populated by its SQLite `DEFAULT
//! (strftime('%s','now'))`, so it is not supplied on insert.

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};

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
    /// The run this row belongs to: the id of that run's
    /// `process_start_initiated` row. `None` only for rows saved before the
    /// service's first launch. See `run_of_row` in `db/mod.rs`.
    pub run_id: Option<i64>,
}

/// Search parameters for [`get_process_logs`]. At least one of `project_dir` / `command_names` must be set.
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
    /// Only rows from each command's latest run (the highest `run_id`).
    pub latest_launch_only: bool,
    /// Only rows from this run.
    pub run_id: Option<i64>,
}

/// Insert a new process log line belonging to `run_id`.
///
/// With `run_id: None` the database assigns the run by position (the latest
/// launch at or before the row), which is only right for writers that can't
/// be overtaken by a newer launch. `timestamp` is intentionally omitted so the
/// column DEFAULT fills it in (unix seconds).
pub fn save_run_log(
    conn: &Connection,
    run_id: Option<i64>,
    command_name: &str,
    project_dir: &str,
    log_type: ProcessLogType,
    content: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "insert into process_output(command_name, project_dir, content, log_type, run_id) values(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![command_name, project_dir, content, log_type.as_i64(), run_id],
    )?;
    Ok(())
}

/// [`save_run_log`] with the run assigned by position.
pub fn save_process_log(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    log_type: ProcessLogType,
    content: Option<&str>,
) -> rusqlite::Result<()> {
    save_run_log(conn, None, command_name, project_dir, log_type, content)
}

/// Record a new launch of `command_name` and return its run id: the id of the
/// `process_start_initiated` row, which the database assigns as the row's own
/// run (see `run_of_row` in `db/mod.rs`).
pub fn start_run(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
) -> rusqlite::Result<i64> {
    save_process_log(
        conn,
        command_name,
        project_dir,
        ProcessLogType::ProcessStartInitiated,
        None,
    )?;
    Ok(conn.last_insert_rowid())
}

/// The latest run id of each command in scope that has one.
pub fn latest_run_ids(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
) -> rusqlite::Result<Vec<(String, i64)>> {
    let (scope, params) = scope_clause(&LogSearchOptions {
        project_dir: Some(project_dir.to_string()),
        command_names: command_names.to_vec(),
        ..Default::default()
    });
    let sql = format!(
        "select po.command_name, max(po.run_id) from process_output po where {scope} \
         and po.run_id is not null group by po.command_name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    rows.collect()
}

/// Build the log-search SQL + params.
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

    if let Some(run_id) = options.run_id {
        sql.push_str(" and po.run_id = ?");
        params.push(Value::Integer(run_id));
    }

    if options.latest_launch_only {
        push_latest_launch_filter(&mut sql);
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
    let mut conditions = Vec::new();
    let mut params = Vec::new();
    if let Some(project_dir) = &options.project_dir {
        conditions.push("po.project_dir = ?".to_string());
        params.push(Value::Text(project_dir.clone()));
    }
    if !options.command_names.is_empty() {
        let placeholders = vec!["?"; options.command_names.len()].join(", ");
        conditions.push(format!("po.command_name in ({placeholders})"));
        params.extend(options.command_names.iter().cloned().map(Value::Text));
    }
    if conditions.is_empty() {
        // Neither a project nor names: a caller error. Match nothing rather
        // than every row in the database.
        conditions.push("1 = 0".to_string());
    }
    (conditions.join(" and "), params)
}

/// Keep only rows from the row's command's latest run. `is` rather than `=` so
/// a command that has never been launched (every `run_id` null) keeps its rows.
fn push_latest_launch_filter(sql: &mut String) {
    sql.push_str(
        " and po.run_id is (select max(p2.run_id) from process_output p2 \
         where p2.project_dir = po.project_dir and p2.command_name = po.command_name)",
    );
}

fn push_log_type_filter(sql: &mut String, params: &mut Vec<Value>, log_types: &[i64]) {
    if log_types.is_empty() {
        return;
    }
    let placeholders = vec!["?"; log_types.len()].join(", ");
    sql.push_str(&format!(" and po.log_type in ({placeholders})"));
    params.extend(log_types.iter().copied().map(Value::Integer));
}

fn row_to_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProcessLog> {
    Ok(ProcessLog {
        id: row.get("id")?,
        command_name: row.get("command_name")?,
        project_dir: row.get("project_dir")?,
        content: row.get("content")?,
        log_type: row.get("log_type")?,
        timestamp: row.get("timestamp")?,
        run_id: row.get("run_id")?,
    })
}

/// Result of [`get_process_logs_with_eviction_info`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessLogResult {
    /// Logs in chronological (oldest-first) order.
    pub logs: Vec<ProcessLog>,
    /// True if older logs exist beyond the requested `limit` (i.e. were truncated).
    pub logs_were_evicted: bool,
}

/// Fetch process logs in chronological (oldest-first) order.
///
/// The SQL fetches newest-first (so a `limit` keeps the
/// most recent rows); the result is then reversed into chronological order.
pub fn get_process_logs(
    conn: &Connection,
    options: &LogSearchOptions,
) -> rusqlite::Result<Vec<ProcessLog>> {
    Ok(get_process_logs_with_eviction_info(conn, options)?.logs)
}

/// Fetch process logs plus a flag indicating whether older logs were evicted.
///
/// When a `limit` is set and we got at
/// least that many rows, re-run the same (limitless) query wrapped in a
/// `count(*)` subquery; if the total exceeds what we returned, older logs were
/// truncated.
pub fn get_process_logs_with_eviction_info(
    conn: &Connection,
    options: &LogSearchOptions,
) -> rusqlite::Result<ProcessLogResult> {
    let (sql, params) = build_log_search_query(options);
    let mut stmt = conn.prepare(&sql)?;
    let mut logs: Vec<ProcessLog> = stmt
        .query_map(params_from_iter(params), row_to_log)?
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
            let total: i64 =
                conn.query_row(&count_sql, params_from_iter(count_params), |row| row.get(0))?;
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
    /// Chronological rows: the newest `limit` printable rows of each command's
    /// latest run.
    pub logs: Vec<ProcessLog>,
    /// Whether printable rows from the latest run were left out by `limit`.
    pub truncated: bool,
}

/// Fetch the last `limit` printable log rows of the latest run, for
/// `candle logs --count`.
///
/// Marker rows and a previous run's rows never count against the limit: they
/// made `--count 3` print two lines whenever `process_started` fell inside the
/// window, or right after a restart.
pub fn get_log_tail(
    conn: &Connection,
    options: &LogSearchOptions,
    limit: i64,
) -> rusqlite::Result<LogTail> {
    let result = get_process_logs_with_eviction_info(
        conn,
        &LogSearchOptions {
            limit: Some(limit),
            log_types: printable_log_types(),
            latest_launch_only: true,
            ..options.clone()
        },
    )?;
    Ok(LogTail {
        logs: result.logs,
        truncated: result.logs_were_evicted,
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

    #[test]
    fn a_previous_runs_late_rows_stay_out_of_the_latest_run() {
        let dir = temp_db_dir("process-logs-late-rows");
        let conn = get_database(Some(&dir)).unwrap();
        let save = |run: Option<i64>, t: ProcessLogType, c: Option<&str>| {
            save_run_log(&conn, run, "api", "/proj", t, c).unwrap();
        };

        let old_run = start_run(&conn, "api", "/proj").unwrap();
        save(Some(old_run), ProcessLogType::ProcessStarted, None);
        save(Some(old_run), ProcessLogType::Stdout, Some("old output"));
        let new_run = start_run(&conn, "api", "/proj").unwrap();
        // The old monitor finishes writing after the relaunch.
        save(
            Some(old_run),
            ProcessLogType::Stdout,
            Some("old late output"),
        );
        save(
            Some(old_run),
            ProcessLogType::ProcessExited,
            Some("Process was stopped"),
        );
        save(Some(new_run), ProcessLogType::ProcessStarted, None);
        save(Some(new_run), ProcessLogType::Stdout, Some("new output"));

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
        assert_eq!(contents, vec!["new output"]);
        assert!(!tail.truncated);
        assert_eq!(
            latest_run_ids(&conn, "/proj", &[]).unwrap(),
            vec![("api".to_string(), new_run)]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
