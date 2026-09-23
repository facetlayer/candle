//! Filters a log stream down to each command's latest run.
//!
//! Every row carries the `run_id` of the launch it belongs to (see
//! [`ProcessLog::run_id`]), so a row is in the latest run exactly when its
//! `run_id` is the highest seen for its command. This doesn't depend on the
//! order rows arrive in: output a previous instance writes after a restart
//! keeps its old `run_id` and is dropped. `formal/Candle/RunFilter.lean` proves
//! this for every input.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::logs::process_logs::{latest_run_ids, ProcessLog};

/// Keeps only rows from each command's latest run, optionally also only rows
/// within a recent time window.
///
/// Seed it with [`seed_latest_runs`](Self::seed_latest_runs) before filtering a
/// batch of existing rows, so rows from a run that has since been superseded
/// are dropped even if the newer run's rows aren't in the batch. While
/// streaming, a row from a newer run moves the filter on to that run.
#[derive(Debug, Clone, Default)]
pub struct LatestRunFilter {
    /// Highest `run_id` seen per command. `None` (the least `Option`) until a
    /// row with a run arrives; rows with no run are shown only until then.
    latest: HashMap<String, Option<i64>>,
    min_timestamp: Option<f64>,
}

impl LatestRunFilter {
    /// A filter over every row, or only rows newer than `recent_window_ms`.
    pub fn new(recent_window_ms: Option<u64>) -> Self {
        let min_timestamp = recent_window_ms.map(|window_ms| {
            // Log timestamps are unix seconds (the column default is
            // strftime('%s', 'now')), so convert the cutoff to seconds.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as f64;
            (now_ms - window_ms as f64) / 1000.0
        });
        LatestRunFilter {
            latest: HashMap::new(),
            min_timestamp,
        }
    }

    /// Learn each command's latest run from the database.
    pub fn seed_latest_runs(
        &mut self,
        conn: &Connection,
        project_dir: &str,
        command_names: &[String],
    ) -> rusqlite::Result<()> {
        for (command_name, run_id) in latest_run_ids(conn, project_dir, command_names)? {
            self.note_run(&command_name, Some(run_id));
        }
        Ok(())
    }

    fn note_run(&mut self, command_name: &str, run_id: Option<i64>) -> Option<i64> {
        let latest = self.latest.entry(command_name.to_string()).or_insert(None);
        if run_id > *latest {
            *latest = run_id;
        }
        *latest
    }

    /// The rows of `logs` that belong to their command's latest run.
    pub fn filter(&mut self, logs: &[ProcessLog]) -> Vec<ProcessLog> {
        let mut result = Vec::new();
        for log in logs {
            let latest = self.note_run(&log.command_name, log.run_id);
            let in_window = self
                .min_timestamp
                .is_none_or(|min| (log.timestamp as f64) >= min);
            if log.run_id == latest && in_window {
                result.push(log.clone());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::ProcessLogType;

    fn row(id: i64, run_id: Option<i64>, content: &str) -> ProcessLog {
        ProcessLog {
            id,
            command_name: "svc".to_string(),
            project_dir: "/project".to_string(),
            content: Some(content.to_string()),
            log_type: ProcessLogType::Stdout.as_i64(),
            timestamp: 0,
            run_id,
        }
    }

    fn contents(logs: &[ProcessLog]) -> Vec<String> {
        logs.iter()
            .map(|l| l.content.clone().unwrap_or_default())
            .collect()
    }

    #[test]
    fn keeps_only_the_latest_run_whatever_the_row_order() {
        // Run 4 replaced run 1, but run 1's monitor wrote its last rows late.
        let logs = vec![
            row(1, Some(1), "old-run"),
            row(4, Some(4), "relaunch"),
            row(5, Some(1), "old-late-output"),
            row(6, Some(1), "Process was stopped"),
            row(7, Some(4), "new-run"),
        ];
        let result = LatestRunFilter::new(None).filter(&logs);
        // Unseeded, the stream shows run 1 until run 4 appears, then only run 4.
        assert_eq!(contents(&result), vec!["old-run", "relaunch", "new-run"]);
    }

    #[test]
    fn a_seeded_filter_drops_superseded_runs_up_front() {
        let mut filter = LatestRunFilter::new(None);
        filter.note_run("svc", Some(4));
        let logs = vec![row(1, Some(1), "old-run"), row(7, Some(4), "new-run")];
        assert_eq!(contents(&filter.filter(&logs)), vec!["new-run"]);
    }

    #[test]
    fn rows_without_a_run_show_until_a_run_appears() {
        let logs = vec![
            row(1, None, "before-any-launch"),
            row(2, Some(2), "launch"),
            row(3, None, "not-in-this-run"),
        ];
        let result = LatestRunFilter::new(None).filter(&logs);
        assert_eq!(contents(&result), vec!["before-any-launch", "launch"]);
    }

    #[test]
    fn commands_are_filtered_independently() {
        let mut other = row(2, Some(1), "other-service");
        other.command_name = "other".to_string();
        let logs = vec![row(3, Some(3), "svc"), other];
        let result = LatestRunFilter::new(None).filter(&logs);
        assert_eq!(contents(&result), vec!["svc", "other-service"]);
    }

    #[test]
    fn hides_logs_older_than_the_window() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut old = row(1, Some(1), "old");
        old.timestamp = now - 60;
        let mut recent = row(2, Some(1), "recent");
        recent.timestamp = now;
        let result = LatestRunFilter::new(Some(10_000)).filter(&[old, recent]);
        assert_eq!(contents(&result), vec!["recent"]);
    }
}
