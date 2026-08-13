//! Filters logs to only show logs from the most recent process launch for each command.
//!
//! Ported from `src/log-filters/LatestExecutionLogFilter.ts`.

use std::collections::HashMap;

use crate::logs::process_logs::ProcessLog;
use crate::logs::ProcessLogType;

/// What to do if no recent launch event is found in the logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowPastLogsBehavior {
    /// Show all logs anyway (useful for `logs` and `watch` commands).
    ShowLogsFromPreviousLaunch,
    /// Only show logs after finding a start event (useful for `run`).
    OnlyShowAfterRecentLaunch,
}

#[derive(Debug, Clone, Copy)]
struct LaunchStatus {
    start_log_id: i64,
}

/// Filters logs to only show logs from the most recent process launch for each
/// command. Optionally also applies a recency window (see `recent_window_ms`).
///
/// Usage:
/// 1. First call [`check_latest_launch_status`](LatestExecutionLogFilter::check_latest_launch_status)
///    with the existing recent logs.
/// 2. Call [`filter`](LatestExecutionLogFilter::filter) to get only logs from the
///    most recent launch.
#[derive(Debug, Clone)]
pub struct LatestExecutionLogFilter {
    recent_command_launch: HashMap<String, LaunchStatus>,
    show_past_logs_behavior: ShowPastLogsBehavior,
    recent_window_ms: Option<u64>,
    min_timestamp: Option<f64>,
}

impl LatestExecutionLogFilter {
    /// Create a new filter.
    pub fn new(show_past_logs_behavior: ShowPastLogsBehavior, recent_window_ms: Option<u64>) -> Self {
        LatestExecutionLogFilter {
            recent_command_launch: HashMap::new(),
            show_past_logs_behavior,
            recent_window_ms,
            min_timestamp: None,
        }
    }

    /// Analyze logs to determine the latest launch status for each command.
    /// Logs should be in chronological order (oldest first).
    pub fn check_latest_launch_status(&mut self, logs: &[ProcessLog]) {
        self.recent_command_launch.clear();

        if let Some(recent_window_ms) = self.recent_window_ms {
            // Log timestamps are stored in seconds (the DB column default is
            // strftime('%s', 'now')), so convert the window cutoff to seconds to match.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as f64;
            self.min_timestamp = Some((now_ms - recent_window_ms as f64) / 1000.0);
        }

        for log in logs {
            if log.log_type == ProcessLogType::ProcessStartInitiated.as_i64() {
                self.recent_command_launch.insert(
                    log.command_name.clone(),
                    LaunchStatus {
                        start_log_id: log.id,
                    },
                );
            }
        }
    }

    fn passes_timestamp_window(&self, log: &ProcessLog) -> bool {
        match self.min_timestamp {
            None => true,
            Some(min_timestamp) => (log.timestamp as f64) >= min_timestamp,
        }
    }

    /// Filter logs to only include logs from the most recent launch for each command.
    pub fn filter(&mut self, logs: &[ProcessLog]) -> Vec<ProcessLog> {
        let mut result: Vec<ProcessLog> = Vec::new();

        for log in logs {
            let status = self.recent_command_launch.get(&log.command_name).copied();

            let should_include_log = if let Some(status) = status {
                // We found a start event - only include logs from that point forward
                log.id >= status.start_log_id && self.passes_timestamp_window(log)
            } else if log.log_type == ProcessLogType::ProcessStartInitiated.as_i64() {
                // Found a start event - mark it and include this log (subject to time window)
                self.recent_command_launch.insert(
                    log.command_name.clone(),
                    LaunchStatus {
                        start_log_id: log.id,
                    },
                );
                self.passes_timestamp_window(log)
            } else if self.show_past_logs_behavior == ShowPastLogsBehavior::ShowLogsFromPreviousLaunch
            {
                // No start event, but configured to show existing logs anyway
                self.passes_timestamp_window(log)
            } else {
                // 'only_show_after_recent_launch' -> exclude
                false
            };

            if should_include_log {
                result.push(log.clone());
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_seconds() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    struct LogMaker {
        next_id: i64,
        now: i64,
    }

    impl LogMaker {
        fn new() -> Self {
            LogMaker {
                next_id: 1,
                now: now_seconds(),
            }
        }

        fn make(&mut self, content: &str, log_type: ProcessLogType, timestamp: i64) -> ProcessLog {
            let id = self.next_id;
            self.next_id += 1;
            ProcessLog {
                id,
                command_name: "svc".to_string(),
                project_dir: "/project".to_string(),
                content: Some(content.to_string()),
                log_type: log_type.as_i64(),
                timestamp,
            }
        }
    }

    fn contents(logs: &[ProcessLog]) -> Vec<String> {
        logs.iter()
            .map(|l| l.content.clone().unwrap_or_default())
            .collect()
    }

    #[test]
    fn shows_all_logs_from_previous_launch_when_no_window() {
        let mut m = LogMaker::new();
        let mut filter =
            LatestExecutionLogFilter::new(ShowPastLogsBehavior::ShowLogsFromPreviousLaunch, None);
        let logs = vec![
            m.make("a", ProcessLogType::Stdout, m.now),
            m.make("b", ProcessLogType::Stdout, m.now),
        ];
        filter.check_latest_launch_status(&logs);

        let result = filter.filter(&logs);
        assert_eq!(contents(&result), vec!["a", "b"]);
    }

    #[test]
    fn shows_recent_logs_within_window() {
        let mut m = LogMaker::new();
        let mut filter = LatestExecutionLogFilter::new(
            ShowPastLogsBehavior::ShowLogsFromPreviousLaunch,
            Some(10_000),
        );
        let logs = vec![
            m.make("recent-1", ProcessLogType::Stdout, m.now),
            m.make("recent-2", ProcessLogType::Stdout, m.now - 2),
        ];
        filter.check_latest_launch_status(&logs);

        let result = filter.filter(&logs);
        assert_eq!(contents(&result), vec!["recent-1", "recent-2"]);
    }

    #[test]
    fn hides_logs_older_than_window() {
        let mut m = LogMaker::new();
        let mut filter = LatestExecutionLogFilter::new(
            ShowPastLogsBehavior::ShowLogsFromPreviousLaunch,
            Some(10_000),
        );
        let logs = vec![
            m.make("old", ProcessLogType::Stdout, m.now - 60),
            m.make("recent", ProcessLogType::Stdout, m.now),
        ];
        filter.check_latest_launch_status(&logs);

        let result = filter.filter(&logs);
        assert_eq!(contents(&result), vec!["recent"]);
    }

    #[test]
    fn only_shows_logs_from_most_recent_launch() {
        let mut m = LogMaker::new();
        let mut filter =
            LatestExecutionLogFilter::new(ShowPastLogsBehavior::ShowLogsFromPreviousLaunch, None);
        let logs = vec![
            m.make("old-run", ProcessLogType::Stdout, m.now - 5),
            m.make("relaunch", ProcessLogType::ProcessStartInitiated, m.now - 1),
            m.make("new-run", ProcessLogType::Stdout, m.now),
        ];
        filter.check_latest_launch_status(&logs);

        let result = filter.filter(&logs);
        assert_eq!(contents(&result), vec!["relaunch", "new-run"]);
    }
}
