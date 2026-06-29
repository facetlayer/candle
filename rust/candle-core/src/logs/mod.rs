//! Log capture subsystem.

pub mod console_log;
pub mod log_iterator;
pub mod log_type;
pub mod process_logs;

pub use console_log::{console_log_row, console_log_system_message, ConsoleLogOptions, OutputFormat};
pub use log_iterator::LogIterator;
pub use log_type::ProcessLogType;
pub use process_logs::{
    get_process_logs, get_process_logs_with_eviction_info, save_process_log, LogSearchOptions,
    ProcessLog, ProcessLogResult,
};
