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
    /// Whether the monitor for this launch has reported its start result yet
    /// (`process_started` / `process_start_failed`). Until it has, a
    /// `process_exited` row can only have come from the previous instance —
    /// see [`LatestExecutionLogFilter::filter`].
    reported_start_result: bool,
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
                        reported_start_result: false,
                    },
                );
            } else if Self::is_start_result(log.log_type) {
                if let Some(status) = self.recent_command_launch.get_mut(&log.command_name) {
                    status.reported_start_result = true;
                }
            }
        }
    }

    fn is_start_result(log_type: i64) -> bool {
        log_type == ProcessLogType::ProcessStarted.as_i64()
            || log_type == ProcessLogType::ProcessStartFailed.as_i64()
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
            // A launch event moves the boundary forward — but only forward. Older
            // launches replayed in the same batch must not undo the boundary that
            // `check_latest_launch_status` already established.
            if log.log_type == ProcessLogType::ProcessStartInitiated.as_i64() {
                let is_newer = self
                    .recent_command_launch
                    .get(&log.command_name)
                    .is_none_or(|status| log.id > status.start_log_id);
                if is_newer {
                    self.recent_command_launch.insert(
                        log.command_name.clone(),
                        LaunchStatus {
                            start_log_id: log.id,
                            reported_start_result: false,
                        },
                    );
                }
            }

            let status = self.recent_command_launch.get(&log.command_name).copied();

            let should_include_log = if let Some(status) = status {
                // A monitor only writes process_exited after it has written
                // process_started, so an exit seen before this launch's start
                // result belongs to the instance that was just killed — its
                // shutdown can outlive the new launch record.
                if log.log_type == ProcessLogType::ProcessExited.as_i64()
                    && !status.reported_start_result
                {
                    false
                } else {
                    // Only include logs from the latest launch onward.
                    log.id >= status.start_log_id && self.passes_timestamp_window(log)
                }
            } else if self.show_past_logs_behavior == ShowPastLogsBehavior::ShowLogsFromPreviousLaunch
            {
                // No start event, but configured to show existing logs anyway
                self.passes_timestamp_window(log)
            } else {
                // 'only_show_after_recent_launch' -> exclude
                false
            };

            if Self::is_start_result(log.log_type) {
                if let Some(status) = self.recent_command_launch.get_mut(&log.command_name) {
                    status.reported_start_result = true;
                }
            }

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

    #[test]
    fn an_older_launch_in_the_batch_does_not_move_the_boundary_back() {
        // The same batch that was analyzed by check_latest_launch_status is then
        // passed to filter(). Replaying the earlier launch event must not reset the
        // boundary to it, or every stale line from that launch gets shown.
        let mut m = LogMaker::new();
        let mut filter =
            LatestExecutionLogFilter::new(ShowPastLogsBehavior::OnlyShowAfterRecentLaunch, None);
        let logs = vec![
            m.make("first-launch", ProcessLogType::ProcessStartInitiated, m.now),
            m.make("old-run", ProcessLogType::Stdout, m.now),
            m.make("second-launch", ProcessLogType::ProcessStartInitiated, m.now),
            m.make("new-run", ProcessLogType::Stdout, m.now),
        ];
        filter.check_latest_launch_status(&logs);

        let result = filter.filter(&logs);
        assert_eq!(contents(&result), vec!["second-launch", "new-run"]);
    }

    #[test]
    fn a_relaunch_seen_while_streaming_moves_the_boundary_forward() {
        let mut m = LogMaker::new();
        let mut filter =
            LatestExecutionLogFilter::new(ShowPastLogsBehavior::OnlyShowAfterRecentLaunch, None);
        let initial = vec![
            m.make("first-launch", ProcessLogType::ProcessStartInitiated, m.now),
            m.make("old-run", ProcessLogType::Stdout, m.now),
        ];
        filter.check_latest_launch_status(&initial);
        filter.filter(&initial);

        let next = vec![
            m.make("second-launch", ProcessLogType::ProcessStartInitiated, m.now),
            m.make("new-run", ProcessLogType::Stdout, m.now),
        ];
        let result = filter.filter(&next);
        assert_eq!(contents(&result), vec!["second-launch", "new-run"]);
    }

    #[test]
    fn hides_a_previous_instances_exit_that_lands_after_the_relaunch() {
        // The killed instance's monitor can write its process_exited row after the
        // new launch was recorded. It is recognizable because this launch has not
        // reported its own start result yet.
        let mut m = LogMaker::new();
        let mut filter =
            LatestExecutionLogFilter::new(ShowPastLogsBehavior::OnlyShowAfterRecentLaunch, None);
        let initial = vec![m.make("relaunch", ProcessLogType::ProcessStartInitiated, m.now)];
        filter.check_latest_launch_status(&initial);

        let logs = vec![
            initial[0].clone(),
            m.make("Process was stopped", ProcessLogType::ProcessExited, m.now),
            m.make("started", ProcessLogType::ProcessStarted, m.now),
            m.make("new-run", ProcessLogType::Stdout, m.now),
        ];

        let result = filter.filter(&logs);
        assert_eq!(contents(&result), vec!["relaunch", "started", "new-run"]);
    }

    #[test]
    fn shows_the_current_instances_own_exit() {
        // Once this launch has reported process_started, a later exit is its own.
        let mut m = LogMaker::new();
        let mut filter =
            LatestExecutionLogFilter::new(ShowPastLogsBehavior::OnlyShowAfterRecentLaunch, None);
        let initial = vec![m.make("relaunch", ProcessLogType::ProcessStartInitiated, m.now)];
        filter.check_latest_launch_status(&initial);

        let logs = vec![
            initial[0].clone(),
            m.make("started", ProcessLogType::ProcessStarted, m.now),
            m.make("Process exited with code 0", ProcessLogType::ProcessExited, m.now),
        ];

        let result = filter.filter(&logs);
        assert_eq!(
            contents(&result),
            vec!["relaunch", "started", "Process exited with code 0"]
        );
    }
}
