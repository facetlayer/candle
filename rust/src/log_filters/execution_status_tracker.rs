//! Tracks the latest lifecycle event per command across log batches.
//!
//! Ported from `src/log-filters/ExecutionStatusTracker.ts`.

use std::collections::{HashMap, HashSet};

use crate::logs::process_logs::ProcessLog;
use crate::logs::ProcessLogType;

/// Tracks the most recent lifecycle event for each command, so the number of
/// running processes can be derived.
#[derive(Debug, Clone, Default)]
pub struct ExecutionStatusTracker {
    execution_status: HashMap<String, i64>,
}

impl ExecutionStatusTracker {
    /// Create an empty tracker.
    pub fn new() -> Self {
        ExecutionStatusTracker::default()
    }

    fn is_lifecycle_event(log_type: i64) -> bool {
        log_type == ProcessLogType::ProcessStartInitiated.as_i64()
            || log_type == ProcessLogType::ProcessStartFailed.as_i64()
            || log_type == ProcessLogType::ProcessStarted.as_i64()
            || log_type == ProcessLogType::ProcessExited.as_i64()
    }

    /// Record the lifecycle events present in `logs` (last event per command wins).
    pub fn apply(&mut self, logs: &[ProcessLog]) {
        for log in logs {
            if Self::is_lifecycle_event(log.log_type) {
                self.execution_status
                    .insert(log.command_name.clone(), log.log_type);
            }
        }
    }

    /// Count the distinct commands whose latest lifecycle event indicates the
    /// process is running (started or start-initiated).
    pub fn count_running_processes(&self) -> usize {
        let mut running: HashSet<&str> = HashSet::new();
        for (command_name, &latest) in self.execution_status.iter() {
            if latest == ProcessLogType::ProcessStarted.as_i64()
                || latest == ProcessLogType::ProcessStartInitiated.as_i64()
            {
                running.insert(command_name.as_str());
            }
        }
        running.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(command_name: &str, log_type: ProcessLogType) -> ProcessLog {
        ProcessLog {
            id: 0,
            command_name: command_name.to_string(),
            project_dir: "/project".to_string(),
            content: None,
            log_type: log_type.as_i64(),
            timestamp: 0,
        }
    }

    #[test]
    fn counts_running_processes() {
        let mut tracker = ExecutionStatusTracker::new();
        tracker.apply(&[
            log("a", ProcessLogType::ProcessStartInitiated),
            log("a", ProcessLogType::ProcessStarted),
            log("b", ProcessLogType::ProcessStartInitiated),
            log("c", ProcessLogType::ProcessStarted),
            log("c", ProcessLogType::ProcessExited),
            log("d", ProcessLogType::ProcessStartFailed),
            // Non-lifecycle events are ignored.
            log("a", ProcessLogType::Stdout),
        ]);

        // a -> started (running), b -> initiated (running), c -> exited, d -> failed.
        assert_eq!(tracker.count_running_processes(), 2);
    }
}
