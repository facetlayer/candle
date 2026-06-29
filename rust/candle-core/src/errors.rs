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
    MissingSetupFile { cwd: String },
}

impl CandleError {
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
            CandleError::MissingSetupFile { cwd } => write!(
                f,
                "No .candle.json file found in (or above) current directory: {cwd}"
            ),
        }
    }
}

impl std::error::Error for CandleError {}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn missing_setup_file_display_name_and_flag() {
        let err = CandleError::MissingSetupFile {
            cwd: "/proj".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "No .candle.json file found in (or above) current directory: /proj"
        );
        assert!(err.is_usage_error());
        assert_eq!(err.name(), "MissingSetupFile");
    }
}
