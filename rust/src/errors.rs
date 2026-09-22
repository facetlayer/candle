//! Error types.
//!
//! Ported from `src/errors.ts`. The Node code uses a structural convention where
//! an error is a "usage error" iff it carries a truthy `isUsageError` property,
//! and each class sets an explicit `.name` string (which does not always match the
//! class identifier). Both are reproduced here for parity.

use std::fmt;

/// Errors raised across the candle CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandleError {
    /// A user-facing usage error (bad arguments, etc.).
    UsageError(String),
    /// A configuration file error. Notably NOT a usage error.
    ConfigFileError(String),
    /// No service with the given name is configured for a directory.
    MissingServiceWithName { command_name: String, cwd: String },
    /// No `.candle.json` file was found in or above a directory.
    /// `explicit` is set when the directory came from `--project-dir`, which
    /// never searches parent directories.
    MissingSetupFile { cwd: String, explicit: bool },
    /// A service failed to start; carries the joined content of the recent log
    /// lines captured during launch. Mirrors `ProcessStartFailedError`.
    ProcessStartFailed {
        command_name: String,
        recent_logs: String,
    },
    /// A generic, non-usage error (timeouts, launch/IO failures). Mirrors a plain
    /// `Error` thrown in the Node start flow.
    Generic(String),
}

impl CandleError {
    /// The error every command reports for a service name it doesn't know:
    /// `No service '<name>' configured for directory: <project_dir>`.
    ///
    /// All unknown-name checks (CLI commands and MCP tools alike) build their
    /// error through this, so the text is identical everywhere.
    pub fn unknown_service(name: &str, project_dir: impl fmt::Display) -> CandleError {
        CandleError::MissingServiceWithName {
            command_name: name.to_string(),
            cwd: project_dir.to_string(),
        }
    }

    /// Whether this error is a user-facing usage error.
    ///
    /// True for everything except `ConfigFileError`, matching the `isUsageError`
    /// flag in `src/errors.ts`.
    pub fn is_usage_error(&self) -> bool {
        match self {
            CandleError::UsageError(_) => true,
            CandleError::ConfigFileError(_) => false,
            CandleError::MissingServiceWithName { .. } => true,
            CandleError::MissingSetupFile { .. } => true,
            CandleError::ProcessStartFailed { .. } => true,
            CandleError::Generic(_) => false,
        }
    }

    /// The literal `.name` string the Node class assigns to itself.
    ///
    /// Note these do not always match the variant name: `MissingServiceWithName`
    /// reports `"NeedRunCommandError"` and `MissingSetupFile` reports
    /// `"MissingSetupFile"`.
    pub fn name(&self) -> &str {
        match self {
            CandleError::UsageError(_) => "UsageError",
            CandleError::ConfigFileError(_) => "ConfigFileError",
            CandleError::MissingServiceWithName { .. } => "NeedRunCommandError",
            CandleError::MissingSetupFile { .. } => "MissingSetupFile",
            CandleError::ProcessStartFailed { .. } => "ProcessStartFailedError",
            CandleError::Generic(_) => "Error",
        }
    }
}

impl fmt::Display for CandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CandleError::UsageError(msg) => write!(f, "{msg}"),
            CandleError::ConfigFileError(msg) => write!(f, "{msg}"),
            CandleError::MissingServiceWithName { command_name, cwd } => write!(
                f,
                "No service '{command_name}' configured for directory: {cwd}"
            ),
            CandleError::MissingSetupFile {
                cwd,
                explicit: true,
            } => write!(
                f,
                "No .candle.json in {cwd} (--project-dir doesn't search parent directories)"
            ),
            CandleError::MissingSetupFile {
                cwd,
                explicit: false,
            } => write!(
                f,
                "No .candle.json file found in (or above) current directory: {cwd}\n\
                 To create one, run `candle add-service <name> --shell <cmd>` or `candle setup-project`."
            ),
            CandleError::ProcessStartFailed {
                command_name,
                recent_logs,
            } => {
                if recent_logs.is_empty() {
                    write!(f, "Process '{command_name}' failed to start.")
                } else {
                    write!(
                        f,
                        "Process '{command_name}' failed to start. Recent logs:\n{recent_logs}"
                    )
                }
            }
            CandleError::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CandleError {}

/// The one prefix every fatal user-facing error carries on stderr.
pub const ERROR_PREFIX: &str = "Error: ";

/// Format a fatal user-facing error as `Error: <message>`.
///
/// Messages are written without a prefix; this adds it in one place. A message
/// that already starts with the prefix is returned unchanged, so a caller can
/// never produce `Error: Error: ...`. Only the first line is prefixed; any
/// following lines (hints, recent logs) are kept as-is.
pub fn error_line(message: &str) -> String {
    if message.starts_with(ERROR_PREFIX) {
        message.to_string()
    } else {
        format!("{ERROR_PREFIX}{message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_line_adds_a_single_prefix() {
        assert_eq!(error_line("bad args"), "Error: bad args");
        assert_eq!(error_line("Error: bad args"), "Error: bad args");
        assert_eq!(error_line("first\nsecond"), "Error: first\nsecond");
    }

    #[test]
    fn usage_error_display_and_flags() {
        let err = CandleError::UsageError("bad args".to_string());
        assert_eq!(err.to_string(), "bad args");
        assert!(err.is_usage_error());
        assert_eq!(err.name(), "UsageError");
    }

    #[test]
    fn config_file_error_is_not_usage_error() {
        let err = CandleError::ConfigFileError("Config file error: oops".to_string());
        assert_eq!(err.to_string(), "Config file error: oops");
        assert!(!err.is_usage_error());
        assert_eq!(err.name(), "ConfigFileError");
    }

    #[test]
    fn missing_service_display_name_and_flag() {
        let err = CandleError::MissingServiceWithName {
            command_name: "api".to_string(),
            cwd: "/proj".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "No service 'api' configured for directory: /proj"
        );
        assert!(err.is_usage_error());
        assert_eq!(err.name(), "NeedRunCommandError");
    }

    #[test]
    fn unknown_service_uses_the_full_form() {
        let err = CandleError::unknown_service("nope", "/proj");
        assert_eq!(
            err.to_string(),
            "No service 'nope' configured for directory: /proj"
        );
        assert!(matches!(err, CandleError::MissingServiceWithName { .. }));
    }

    #[test]
    fn process_start_failed_puts_logs_on_their_own_lines() {
        let err = CandleError::ProcessStartFailed {
            command_name: "api".to_string(),
            recent_logs: "sh: x: command not found\nProcess failed to start: exited with code 127"
                .to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Process 'api' failed to start. Recent logs:\nsh: x: command not found\nProcess failed to start: exited with code 127"
        );

        let bare = CandleError::ProcessStartFailed {
            command_name: "api".to_string(),
            recent_logs: String::new(),
        };
        assert_eq!(bare.to_string(), "Process 'api' failed to start.");
    }

    #[test]
    fn missing_setup_file_display_name_and_flag() {
        let err = CandleError::MissingSetupFile {
            cwd: "/proj".to_string(),
            explicit: false,
        };
        assert_eq!(
            err.to_string(),
            "No .candle.json file found in (or above) current directory: /proj\n\
             To create one, run `candle add-service <name> --shell <cmd>` or `candle setup-project`."
        );
        assert!(err.is_usage_error());
        assert_eq!(err.name(), "MissingSetupFile");
    }

    #[test]
    fn missing_setup_file_for_explicit_project_dir() {
        let err = CandleError::MissingSetupFile {
            cwd: "/proj/sub".to_string(),
            explicit: true,
        };
        assert_eq!(
            err.to_string(),
            "No .candle.json in /proj/sub (--project-dir doesn't search parent directories)"
        );
    }
}
