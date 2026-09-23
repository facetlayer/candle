//! Process log type enum.
//!
//! Stored in the `process_output.log_type` integer column.

/// Content of the `process_start_failed` row the monitor writes when Candle
/// itself stopped the process (`kill`, `restart`, or a `start` replacing it)
/// during the startup grace period. `ps` / `list` read it to tell that
/// deliberate stop apart from a failed start, which shows as `FAILED`.
pub const STOPPED_WHILE_STARTING_MESSAGE: &str = "Process was stopped while starting";

/// Marks a run that ended by a signal Candle didn't send (a segfault, the OOM
/// killer); `ps` / `list` show such a run as `FAILED`.
pub const KILLED_BY_SIGNAL: &str = "killed by signal";

/// `Process was killed by signal <n>`, the exit row for a crash by signal.
pub fn killed_by_signal_message(signal: i32) -> String {
    format!("Process was {KILLED_BY_SIGNAL} {signal}")
}

/// Type of a captured process log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum ProcessLogType {
    Stdout = 1,
    Stderr = 2,
    /// Saved immediately when we begin launching a subprocess.
    ProcessStartInitiated = 3,
    /// Saved when the subprocess fails to start.
    ProcessStartFailed = 4,
    /// Saved when the subprocess has successfully started.
    ProcessStarted = 5,
    /// Saved when the subprocess exits.
    ProcessExited = 6,
}

impl ProcessLogType {
    /// The integer value stored in the database.
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

impl TryFrom<i64> for ProcessLogType {
    type Error = i64;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ProcessLogType::Stdout),
            2 => Ok(ProcessLogType::Stderr),
            3 => Ok(ProcessLogType::ProcessStartInitiated),
            4 => Ok(ProcessLogType::ProcessStartFailed),
            5 => Ok(ProcessLogType::ProcessStarted),
            6 => Ok(ProcessLogType::ProcessExited),
            other => Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_i64_matches_node_values() {
        assert_eq!(ProcessLogType::Stdout.as_i64(), 1);
        assert_eq!(ProcessLogType::Stderr.as_i64(), 2);
        assert_eq!(ProcessLogType::ProcessStartInitiated.as_i64(), 3);
        assert_eq!(ProcessLogType::ProcessStartFailed.as_i64(), 4);
        assert_eq!(ProcessLogType::ProcessStarted.as_i64(), 5);
        assert_eq!(ProcessLogType::ProcessExited.as_i64(), 6);
    }

    #[test]
    fn round_trip_all_variants() {
        for value in 1..=6 {
            let parsed = ProcessLogType::try_from(value).expect("should parse");
            assert_eq!(parsed.as_i64(), value);
        }
    }

    #[test]
    fn unknown_value_returns_err() {
        assert_eq!(ProcessLogType::try_from(0), Err(0));
        assert_eq!(ProcessLogType::try_from(7), Err(7));
    }
}
