//! Log storage in `log_lines`, keyed by service and launch run.

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};

use crate::logs::log_type::ProcessLogType;

/// A log row, with its service's project and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLog {
    pub id: i64,
    pub command_name: String,
    pub project_dir: String,
    pub content: Option<String>,
    pub log_type: i64,
    pub timestamp: i64,
    /// Launch marker id, or `None` before the first launch.
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
    /// Only rows from each command's run before the latest (the second-highest
    /// `run_id`). A command with fewer than two runs matches nothing.
    pub previous_launch_only: bool,
    /// Only rows from this run.
    pub run_id: Option<i64>,
}

/// Insert a row for `run_id`, or the latest run when omitted.
/// Omit the run only if a newer launch cannot overtake this writer.
/// SQLite supplies the timestamp.
pub fn save_run_log(
    conn: &Connection,
    run_id: Option<i64>,
    command_name: &str,
    project_dir: &str,
    log_type: ProcessLogType,
    content: Option<&str>,
) -> rusqlite::Result<()> {
    insert_log(conn, run_id, command_name, project_dir, log_type, content)?;
    Ok(())
}

/// Batch output rows in one transaction to avoid a commit per line.
pub fn save_run_logs<'a>(
    conn: &Connection,
    run_id: Option<i64>,
    command_name: &str,
    project_dir: &str,
    entries: impl IntoIterator<Item = (ProcessLogType, &'a str)>,
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (log_type, content) in entries {
        insert_log(
            &tx,
            run_id,
            command_name,
            project_dir,
            log_type,
            Some(content),
        )?;
    }
    tx.commit()
}

/// Insert a row, creating its service if absent. Retry if cleanup removes the
/// service between creation and insertion.
fn insert_log(
    conn: &Connection,
    run_id: Option<i64>,
    command_name: &str,
    project_dir: &str,
    log_type: ProcessLogType,
    content: Option<&str>,
) -> rusqlite::Result<i64> {
    let mut insert = conn.prepare_cached(&format!(
        "insert into log_lines(service_id, run_id, log_type, content) \
         select s.id, coalesce(?3, {}), ?4, ?5 from services s \
         where s.project_dir = ?1 and s.command_name = ?2",
        crate::db::latest_run_of("s.id")
    ))?;
    loop {
        let inserted = insert.execute(rusqlite::params![
            project_dir,
            command_name,
            run_id,
            log_type.as_i64(),
            content
        ])?;
        if inserted > 0 {
            return Ok(conn.last_insert_rowid());
        }
        conn.execute(
            "insert or ignore into services(project_dir, command_name) values(?1, ?2)",
            rusqlite::params![project_dir, command_name],
        )?;
    }
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

/// Record a launch and return its row id, also assigned as its run id by trigger.
pub fn start_run(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
) -> rusqlite::Result<i64> {
    insert_log(
        conn,
        None,
        command_name,
        project_dir,
        ProcessLogType::ProcessStartInitiated,
        None,
    )
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
        "select s.command_name, (select max(l.run_id) from log_lines l where l.service_id = s.id) as run \
         from services s where {scope} and run is not null"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    rows.collect()
}

/// Search the latest runs through `max_log_id` in SQL, without loading logs.
pub fn latest_run_contains(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    message: &str,
    max_log_id: i64,
) -> rusqlite::Result<bool> {
    let (scope, mut params) = scope_clause(&LogSearchOptions {
        project_dir: Some(project_dir.to_string()),
        command_names: command_names.to_vec(),
        ..Default::default()
    });
    let mut sql = format!(
        "select 1 from services s join log_lines l on l.service_id = s.id \
         where {scope} and l.id <= ? and instr(l.content, ?) > 0"
    );
    params.push(Value::Integer(max_log_id));
    params.push(Value::Text(message.to_string()));
    push_latest_launch_filter(&mut sql);
    sql.push_str(" limit 1");
    conn.prepare(&sql)?.exists(params_from_iter(params))
}

/// Build a newest-first log query; reverse results for chronological order.
fn build_log_search_query(options: &LogSearchOptions) -> (String, Vec<Value>) {
    let (scope, mut params) = scope_clause(options);
    let mut sql = format!(
        "select l.id, s.command_name, s.project_dir, l.content, l.log_type, l.timestamp, l.run_id \
         from services s join log_lines l on l.service_id = s.id where {scope}"
    );

    if let Some(since) = options.since_timestamp {
        sql.push_str(" and l.timestamp > ?");
        params.push(Value::Integer(since));
    }

    if let Some(after) = options.after_log_id {
        sql.push_str(" and l.id > ?");
        params.push(Value::Integer(after));
    }

    if let Some(min_id) = options.min_log_id {
        sql.push_str(" and l.id >= ?");
        params.push(Value::Integer(min_id));
    }

    push_log_type_filter(&mut sql, &mut params, &options.log_types);

    if let Some(run_id) = options.run_id {
        sql.push_str(" and l.run_id = ?");
        params.push(Value::Integer(run_id));
    }

    if options.latest_launch_only {
        push_latest_launch_filter(&mut sql);
    }

    if options.previous_launch_only {
        push_previous_launch_filter(&mut sql);
    }

    sql.push_str(" order by l.id desc");

    if let Some(limit) = options.limit {
        sql.push_str(" limit ?");
        params.push(Value::Integer(limit));
    }

    (sql, params)
}

/// The project/command part of a `where` clause over `services s`.
fn scope_clause(options: &LogSearchOptions) -> (String, Vec<Value>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();
    if let Some(project_dir) = &options.project_dir {
        conditions.push("s.project_dir = ?".to_string());
        params.push(Value::Text(project_dir.clone()));
    }
    if !options.command_names.is_empty() {
        let placeholders = vec!["?"; options.command_names.len()].join(", ");
        conditions.push(format!("s.command_name in ({placeholders})"));
        params.extend(options.command_names.iter().cloned().map(Value::Text));
    }
    if conditions.is_empty() {
        // An unscoped query must not expose every log in the database.
        conditions.push("1 = 0".to_string());
    }
    (conditions.join(" and "), params)
}

/// Keep only rows from the row's command's latest run. `is` rather than `=` so
/// a command that has never been launched (every `run_id` null) keeps its rows.
fn push_latest_launch_filter(sql: &mut String) {
    sql.push_str(&format!(
        " and l.run_id is {}",
        crate::db::latest_run_of("s.id")
    ));
}

/// Select the previous run; `=` excludes NULL when no previous run exists.
fn push_previous_launch_filter(sql: &mut String) {
    sql.push_str(&format!(
        " and l.run_id = (select max(l2.run_id) from log_lines l2 \
         where l2.service_id = s.id and l2.run_id < {})",
        crate::db::latest_run_of("s.id")
    ));
}

fn push_log_type_filter(sql: &mut String, params: &mut Vec<Value>, log_types: &[i64]) {
    if log_types.is_empty() {
        return;
    }
    let placeholders = vec!["?"; log_types.len()].join(", ");
    sql.push_str(&format!(" and l.log_type in ({placeholders})"));
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

/// Fetch the newest matching rows, returned in chronological order.
pub fn get_process_logs(
    conn: &Connection,
    options: &LogSearchOptions,
) -> rusqlite::Result<Vec<ProcessLog>> {
    Ok(get_process_logs_with_eviction_info(conn, options)?.logs)
}

/// Fetch logs and, when the limit is reached, count matches to detect truncation.
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

    logs.reverse();
    Ok(ProcessLogResult {
        logs,
        logs_were_evicted,
    })
}

/// Log types that `candle logs` prints. The launch markers
/// (`process_start_initiated`, `process_started`) render as nothing.
pub fn printable_log_types() -> Vec<i64> {
    vec![
        ProcessLogType::Stdout.as_i64(),
        ProcessLogType::Stderr.as_i64(),
        ProcessLogType::ProcessStartFailed.as_i64(),
        ProcessLogType::ProcessExited.as_i64(),
    ]
}

/// Which runs of a command [`get_log_tail_of`] reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RunScope {
    /// The latest run only (the default for `candle logs`).
    #[default]
    Latest,
    /// The previous run (`logs --previous`).
    Previous,
    /// Every stored run, oldest first (`logs --all-runs`).
    All,
}

/// Result of [`get_log_tail`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogTail {
    /// Chronological rows: the newest `limit` printable rows of each command's
    /// selected runs (the latest run, for [`get_log_tail`]).
    pub logs: Vec<ProcessLog>,
    /// Whether printable rows from the selected runs were left out by `limit`.
    pub truncated: bool,
}

/// Fetch the latest run's last `limit` printable rows per command.
/// Launch markers and previous runs do not count against the limit.
pub fn get_log_tail(
    conn: &Connection,
    options: &LogSearchOptions,
    limit: i64,
) -> rusqlite::Result<LogTail> {
    get_log_tail_of(conn, options, limit, RunScope::Latest)
}

/// [`get_log_tail`] over the runs `runs` selects instead of only the latest.
pub fn get_log_tail_of(
    conn: &Connection,
    options: &LogSearchOptions,
    limit: i64,
    runs: RunScope,
) -> rusqlite::Result<LogTail> {
    let result = get_process_logs_with_eviction_info(
        conn,
        &LogSearchOptions {
            limit: Some(limit),
            log_types: printable_log_types(),
            latest_launch_only: runs == RunScope::Latest,
            previous_launch_only: runs == RunScope::Previous,
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
        "select s.command_name from services s where s.project_dir = ?1 \
         and exists(select 1 from log_lines l where l.service_id = s.id and l.id > ?2) \
         order by s.command_name",
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
        "select exists(select 1 from services s join log_lines l on l.service_id = s.id \
         where s.project_dir = ?1 and s.command_name = ?2)",
        rusqlite::params![project_dir, command_name],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};

    #[test]
    fn save_run_logs_writes_a_batch_in_order() {
        let dir = temp_db_dir("process-logs-batch");
        let conn = get_database(Some(&dir)).unwrap();

        save_run_logs(
            &conn,
            Some(7),
            "api",
            "/proj",
            [
                (ProcessLogType::Stdout, "one"),
                (ProcessLogType::Stderr, "two"),
                (ProcessLogType::Stdout, "three"),
            ],
        )
        .unwrap();

        let logs = get_process_logs(
            &conn,
            &LogSearchOptions {
                project_dir: Some("/proj".into()),
                command_names: vec!["api".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let rows: Vec<_> = logs
            .iter()
            .map(|l| (l.log_type, l.content.as_deref().unwrap(), l.run_id))
            .collect();
        assert_eq!(
            rows,
            vec![
                (ProcessLogType::Stdout.as_i64(), "one", Some(7)),
                (ProcessLogType::Stderr.as_i64(), "two", Some(7)),
                (ProcessLogType::Stdout.as_i64(), "three", Some(7)),
            ]
        );
    }

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
        assert_eq!(logs[0].log_type, ProcessLogType::ProcessStarted.as_i64());
        assert_eq!(logs[0].content, None);
        assert_eq!(logs[1].content, Some("line one".to_string()));
        assert_eq!(logs[2].content, Some("line two".to_string()));
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
        assert!(after[0].id < after[2].id);

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
