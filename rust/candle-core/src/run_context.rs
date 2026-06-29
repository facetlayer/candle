//! Run context detection.
//!
//! Ported from `src/runContext.ts`. Determines whether candle is being run by an
//! AI agent (Claude Code), based on the `CLAUDECODE` environment variable.

use std::sync::OnceLock;

/// Pure helper: truthiness of an optional env value, matching JS `!!value`
/// (present AND non-empty string => true).
fn truthy(value: Option<String>) -> bool {
    matches!(value, Some(v) if !v.is_empty())
}

/// Whether candle is being run by an AI agent.
///
/// True iff the `CLAUDECODE` env var is present and non-empty. Computed once and
/// cached, matching the module-level constant in `src/runContext.ts`.
pub fn is_run_by_agent() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| truthy(std::env::var("CLAUDECODE").ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness_table() {
        assert!(!truthy(None));
        assert!(!truthy(Some(String::new())));
        assert!(truthy(Some("1".to_string())));
        // Any non-empty string is truthy, including "false".
        assert!(truthy(Some("false".to_string())));
    }
}
