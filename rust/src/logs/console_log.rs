//! Console rendering of `process_output` rows.
//!
//! Ported from `src/logs.ts` (`consoleLogRow` / `consoleLogSystemMessage` and the
//! stdout/stderr helpers). All output goes through [`crate::output::out`] — even
//! stderr-typed log lines, which Node renders via `console.log` (stdout) with a
//! `[stderr]` prefix, not via `console.error`.
//!
//! There are no ANSI colors in this code path, so `FORCE_COLOR` has no effect.

use crate::logs::log_type::ProcessLogType;
use crate::logs::process_logs::ProcessLog;
use crate::output;

/// Output format for a rendered log row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pretty,
    Json,
}

/// Options controlling how a row is rendered, mirroring `ConsoleLogOptions`.
#[derive(Debug, Clone, Default)]
pub struct ConsoleLogOptions {
    pub format: Option<OutputFormat>,
    /// A literal prefix prepended to pretty output.
    pub prefix: Option<String>,
    /// When true, prepend `[<command_name>] ` to the prefix (used by `logs`).
    pub enable_app_name_prefix: bool,
}

impl ConsoleLogOptions {
    /// Pretty format with no prefix.
    pub fn pretty() -> Self {
        ConsoleLogOptions {
            format: Some(OutputFormat::Pretty),
            prefix: None,
            enable_app_name_prefix: false,
        }
    }
}

fn format(options: &ConsoleLogOptions) -> OutputFormat {
    options.format.unwrap_or(OutputFormat::Pretty)
}

fn console_log_stdout(format: OutputFormat, msg: &str, prefix: &str) {
    match format {
        OutputFormat::Json => {
            output::out(&serde_json::json!({ "stdout": msg }).to_string());
        }
        OutputFormat::Pretty => {
            output::out(&format!("{prefix}{msg}"));
        }
    }
}

fn console_log_stderr(format: OutputFormat, msg: &str, prefix: &str) {
    match format {
        OutputFormat::Json => {
            output::out(&serde_json::json!({ "stderr": msg }).to_string());
        }
        OutputFormat::Pretty => {
            output::out(&format!("{prefix}[stderr] {msg}"));
        }
    }
}

/// Render a bracketed system message (start/exit lifecycle notices).
pub fn console_log_system_message(format: OutputFormat, msg: &str, prefix: &str) {
    match format {
        OutputFormat::Json => {
            output::out(&serde_json::json!({ "message": msg }).to_string());
        }
        OutputFormat::Pretty => {
            output::out(&format!("{prefix}[{msg}]"));
        }
    }
}

/// Render a single `process_output` row.
///
/// `stdout` lines print as-is, `stderr` lines gain a `[stderr] ` prefix,
/// `process_exited` / `process_start_failed` render as bracketed system messages,
/// and `process_start_initiated` / `process_started` are hidden (no output).
pub fn console_log_row(row: &ProcessLog, options: &ConsoleLogOptions) {
    let fmt = format(options);

    // Build the effective prefix. enable_app_name_prefix prepends `[cmd] ` to any
    // existing prefix, exactly like the Node `[${command_name}] ${prefix || ''}`.
    let base_prefix = options.prefix.clone().unwrap_or_default();
    let prefix = if options.enable_app_name_prefix {
        format!("[{}] {}", row.command_name, base_prefix)
    } else {
        base_prefix
    };

    let content = row.content.as_deref().unwrap_or_default();

    match ProcessLogType::try_from(row.log_type) {
        Ok(ProcessLogType::Stdout) => console_log_stdout(fmt, content, &prefix),
        Ok(ProcessLogType::Stderr) => console_log_stderr(fmt, content, &prefix),
        Ok(ProcessLogType::ProcessExited) | Ok(ProcessLogType::ProcessStartFailed) => {
            console_log_system_message(fmt, content, &prefix)
        }
        // process_start_initiated / process_started are hidden.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::capture;

    fn row(log_type: ProcessLogType, content: Option<&str>) -> ProcessLog {
        ProcessLog {
            id: 1,
            command_name: "api".to_string(),
            project_dir: "/proj".to_string(),
            content: content.map(str::to_string),
            log_type: log_type.as_i64(),
            timestamp: 100,
        }
    }

    #[test]
    fn stdout_pretty_no_prefix() {
        let (_, captured) = capture(|| {
            console_log_row(&row(ProcessLogType::Stdout, Some("hello")), &ConsoleLogOptions::pretty());
        });
        assert_eq!(captured.stdout, vec!["hello".to_string()]);
    }

    #[test]
    fn stderr_pretty_has_prefix() {
        let (_, captured) = capture(|| {
            console_log_row(&row(ProcessLogType::Stderr, Some("boom")), &ConsoleLogOptions::pretty());
        });
        assert_eq!(captured.stdout, vec!["[stderr] boom".to_string()]);
    }

    #[test]
    fn app_name_prefix_and_stderr_combine() {
        let opts = ConsoleLogOptions {
            format: Some(OutputFormat::Pretty),
            prefix: None,
            enable_app_name_prefix: true,
        };
        let (_, captured) = capture(|| {
            console_log_row(&row(ProcessLogType::Stderr, Some("boom")), &opts);
        });
        assert_eq!(captured.stdout, vec!["[api] [stderr] boom".to_string()]);
    }

    #[test]
    fn system_message_is_bracketed() {
        let (_, captured) = capture(|| {
            console_log_row(&row(ProcessLogType::ProcessExited, Some("exited code 0")), &ConsoleLogOptions::pretty());
        });
        assert_eq!(captured.stdout, vec!["[exited code 0]".to_string()]);
    }

    #[test]
    fn lifecycle_start_types_are_hidden() {
        let (_, captured) = capture(|| {
            console_log_row(&row(ProcessLogType::ProcessStartInitiated, None), &ConsoleLogOptions::pretty());
            console_log_row(&row(ProcessLogType::ProcessStarted, None), &ConsoleLogOptions::pretty());
        });
        assert!(captured.stdout.is_empty());
    }

    #[test]
    fn json_format_stdout() {
        let (_, captured) = capture(|| {
            let opts = ConsoleLogOptions {
                format: Some(OutputFormat::Json),
                prefix: None,
                enable_app_name_prefix: false,
            };
            console_log_row(&row(ProcessLogType::Stdout, Some("hi")), &opts);
        });
        assert_eq!(captured.stdout, vec!["{\"stdout\":\"hi\"}".to_string()]);
    }
}
