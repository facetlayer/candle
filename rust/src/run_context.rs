//! Run context detection.
//!
//! Determines whether candle is being run by an AI coding agent, based on the
//! marker environment variables those agents set, and whether the session is
//! interactive (a human at a terminal) versus non-interactive (agents, scripts,
//! pipes).

use std::io::IsTerminal;
use std::sync::OnceLock;

/// Environment variables that coding agents set on the commands they run, so
/// that tools can tell they're being driven by an agent.
///
/// Each of these is set by the agent itself (not by the user configuring it):
/// - `CLAUDECODE` — Claude Code.
/// - `GEMINI_CLI` — Gemini CLI, set by its `run_shell_command` tool.
/// - `CURSOR_AGENT` — Cursor; documented for exactly this purpose.
///
/// Deliberately absent: Codex. Its `CODEX_SANDBOX` variable signals that a
/// *sandbox* is active, not that Codex is driving — it is unset under
/// `--sandbox danger-full-access`, so keying on it would silently miss anyone
/// who turns the sandbox off. Codex sessions are still caught by the stdout
/// TTY check in [`is_interactive`].
const AGENT_ENV_VARS: [&str; 3] = ["CLAUDECODE", "GEMINI_CLI", "CURSOR_AGENT"];

/// Pure helper: truthiness of an optional env value, matching JS `!!value`
/// (present AND non-empty string => true).
fn truthy(value: Option<String>) -> bool {
    matches!(value, Some(v) if !v.is_empty())
}

/// Pure core of [`is_run_by_agent`], taking the environment as a lookup so it
/// can be tested without mutating the process environment.
fn detect_agent(lookup: impl Fn(&str) -> Option<String>) -> bool {
    AGENT_ENV_VARS.iter().any(|name| truthy(lookup(name)))
}

/// Whether candle is being run by an AI agent.
///
/// True iff any variable in [`AGENT_ENV_VARS`] is present and non-empty.
/// Computed once and cached.
pub fn is_run_by_agent() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| detect_agent(|name| std::env::var(name).ok()))
}

/// Whether candle is running in an interactive session: a human at a terminal.
///
/// False when run by an AI agent (see [`is_run_by_agent`]) or when stdout is not
/// a TTY (pipes, scripts, CI). Commands use this to pick between blocking,
/// watch-style behavior (interactive) and return-immediately behavior
/// (non-interactive).
pub fn is_interactive() -> bool {
    !is_run_by_agent() && std::io::stdout().is_terminal()
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

    /// Build a lookup that reports exactly one variable as set.
    fn only(set_name: &'static str, value: &'static str) -> impl Fn(&str) -> Option<String> {
        move |name| (name == set_name).then(|| value.to_string())
    }

    #[test]
    fn each_agent_var_triggers_agent_mode() {
        for name in AGENT_ENV_VARS {
            assert!(detect_agent(only(name, "1")), "{name} should trigger agent mode");
        }
    }

    #[test]
    fn no_agent_vars_means_not_agent_mode() {
        assert!(!detect_agent(|_| None));
        // Present but empty does not count.
        assert!(!detect_agent(only("CLAUDECODE", "")));
        // An unrelated variable does not count. In particular CODEX_SANDBOX,
        // which marks a sandbox rather than an agent (see AGENT_ENV_VARS).
        assert!(!detect_agent(only("CODEX_SANDBOX", "seatbelt")));
    }
}
