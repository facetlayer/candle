//! Log capture subsystem.

pub mod log_iterator;
pub mod log_type;
pub mod process_logs;

pub use log_iterator::LogIterator;
pub use log_type::ProcessLogType;
pub use process_logs::{get_process_logs, save_process_log, LogSearchOptions, ProcessLog};
