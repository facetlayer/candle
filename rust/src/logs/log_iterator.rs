//! A forward cursor over `process_output` rows for a fixed query scope.
//!
//! Ported from `src/logs/LogIterator.ts`. The Node version exposes an async
//! generator `it()` that polls the DB every 100ms; here the polling loop lives in
//! the caller ([`crate::start::start_one_service`]), and this type just tracks the
//! cursor position (`current_log_id`) and fetches the next batch on demand.

use rusqlite::Connection;

use crate::logs::process_logs::{get_process_logs, LogSearchOptions, ProcessLog};

/// A cursor over the `process_output` table scoped to a `(project_dir,
/// command_names)` pair. Tracks the id of the last consumed row so repeated
/// fetches only return newer rows.
#[derive(Debug, Clone)]
pub struct LogIterator {
    project_dir: String,
    command_names: Vec<String>,
    /// Default limit applied to fetches when no per-call override is given.
    limit: Option<i64>,
    /// Id of the most recently consumed log, or `None` before any reset/fetch.
    pub current_log_id: Option<i64>,
}

impl LogIterator {
    /// Create a cursor scoped to a project dir and set of command names.
    pub fn new(project_dir: String, command_names: Vec<String>) -> Self {
        LogIterator::with_limit(project_dir, command_names, None)
    }

    /// Create a cursor with a default fetch `limit`.
    pub fn with_limit(project_dir: String, command_names: Vec<String>, limit: Option<i64>) -> Self {
        LogIterator {
            project_dir,
            command_names,
            limit,
            current_log_id: None,
        }
    }

    fn search_options(&self, limit: Option<i64>) -> LogSearchOptions {
        LogSearchOptions {
            project_dir: Some(self.project_dir.clone()),
            command_names: self.command_names.clone(),
            limit,
            since_timestamp: None,
            after_log_id: self.current_log_id,
        }
    }

    /// Seed `current_log_id` to the id of the newest existing matching row (or
    /// `None` if there are none), so subsequent fetches only see rows produced
    /// after this point. Mirrors `resetToLatestLogMessage`.
    pub fn reset_to_latest_log_message(&mut self, conn: &Connection) -> rusqlite::Result<()> {
        self.current_log_id = None;
        let options = LogSearchOptions {
            project_dir: Some(self.project_dir.clone()),
            command_names: self.command_names.clone(),
            limit: Some(1),
            since_timestamp: None,
            after_log_id: None,
        };
        let logs = get_process_logs(conn, &options)?;
        if let Some(latest) = logs.last() {
            self.current_log_id = Some(latest.id);
        }
        Ok(())
    }

    /// A snapshot copy at the current cursor position. Mirrors `copy()`.
    pub fn copy(&self) -> LogIterator {
        self.clone()
    }

    /// Fetch rows with id greater than `current_log_id` WITHOUT advancing the
    /// cursor. Mirrors the private `peekNextLogs`. The effective limit is
    /// `limit_override` if set, otherwise the constructor limit.
    pub fn peek_next_logs(
        &self,
        conn: &Connection,
        limit_override: Option<i64>,
    ) -> rusqlite::Result<Vec<ProcessLog>> {
        let limit = limit_override.or(self.limit);
        get_process_logs(conn, &self.search_options(limit))
    }

    /// Fetch rows with id greater than `current_log_id`, advancing the cursor to
    /// the last returned row. Mirrors `getNextLogs`. The effective limit is
    /// `limit_override` if set, otherwise the constructor limit.
    pub fn get_next_logs(
        &mut self,
        conn: &Connection,
        limit_override: Option<i64>,
    ) -> rusqlite::Result<Vec<ProcessLog>> {
        let logs = self.peek_next_logs(conn, limit_override)?;
        if let Some(last) = logs.last() {
            self.current_log_id = Some(last.id);
        }
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};
    use crate::logs::process_logs::save_process_log;
    use crate::logs::ProcessLogType;

    fn seed(conn: &Connection, n: usize) {
        for i in 0..n {
            save_process_log(
                conn,
                "api",
                "/proj",
                ProcessLogType::Stdout,
                Some(&format!("line {i}")),
            )
            .unwrap();
        }
    }

    #[test]
    fn reset_seeds_to_latest_then_yields_only_newer() {
        let dir = temp_db_dir("log-iterator-reset");
        let conn = get_database(Some(&dir)).unwrap();
        seed(&conn, 3);

        let mut it = LogIterator::new("/proj".to_string(), vec!["api".to_string()]);
        it.reset_to_latest_log_message(&conn).unwrap();
        assert_eq!(it.current_log_id, Some(3));

        // Nothing new yet.
        assert!(it.get_next_logs(&conn, None).unwrap().is_empty());

        // Add two more; the cursor only returns those.
        seed(&conn, 2);
        let next = it.get_next_logs(&conn, None).unwrap();
        assert_eq!(next.len(), 2);
        assert_eq!(next[0].id, 4);
        assert_eq!(next[1].id, 5);
        assert_eq!(it.current_log_id, Some(5));

        // Exhausted again.
        assert!(it.get_next_logs(&conn, None).unwrap().is_empty());

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reset_on_empty_table_is_none() {
        let dir = temp_db_dir("log-iterator-empty");
        let conn = get_database(Some(&dir)).unwrap();

        let mut it = LogIterator::new("/proj".to_string(), vec!["api".to_string()]);
        it.reset_to_latest_log_message(&conn).unwrap();
        assert_eq!(it.current_log_id, None);

        // A fresh cursor with no position returns everything.
        seed(&conn, 2);
        let all = it.get_next_logs(&conn, None).unwrap();
        assert_eq!(all.len(), 2);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_preserves_position_independently() {
        let dir = temp_db_dir("log-iterator-copy");
        let conn = get_database(Some(&dir)).unwrap();
        seed(&conn, 1);

        let mut it = LogIterator::new("/proj".to_string(), vec!["api".to_string()]);
        it.reset_to_latest_log_message(&conn).unwrap();
        let initial = it.copy();
        assert_eq!(initial.current_log_id, Some(1));

        // Advance the original; the copy stays put.
        seed(&conn, 2);
        it.get_next_logs(&conn, None).unwrap();
        assert_eq!(it.current_log_id, Some(3));
        assert_eq!(initial.current_log_id, Some(1));

        // The copy, fetched from its frozen position, sees everything after id 1.
        let mut initial = initial;
        let from_initial = initial.get_next_logs(&conn, None).unwrap();
        assert_eq!(from_initial.len(), 2);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn peek_does_not_advance() {
        let dir = temp_db_dir("log-iterator-peek");
        let conn = get_database(Some(&dir)).unwrap();
        seed(&conn, 2);

        let it = LogIterator::new("/proj".to_string(), vec!["api".to_string()]);
        let first = it.peek_next_logs(&conn, None).unwrap();
        let second = it.peek_next_logs(&conn, None).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        assert_eq!(it.current_log_id, None);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn limit_caps_results_and_per_call_override_wins() {
        let dir = temp_db_dir("log-iterator-limit");
        let conn = get_database(Some(&dir)).unwrap();
        seed(&conn, 5);

        // Constructor limit of 2: a fresh cursor returns only the newest 2 rows,
        // in chronological order.
        let mut it = LogIterator::with_limit("/proj".to_string(), vec!["api".to_string()], Some(2));
        let two = it.get_next_logs(&conn, None).unwrap();
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].id, 4);
        assert_eq!(two[1].id, 5);
        assert_eq!(it.current_log_id, Some(5));

        // A per-call override takes precedence over the constructor limit.
        let mut it2 = LogIterator::with_limit("/proj".to_string(), vec!["api".to_string()], Some(2));
        let one = it2.get_next_logs(&conn, Some(1)).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id, 5);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
