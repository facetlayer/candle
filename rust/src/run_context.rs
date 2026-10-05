//! Detect agent-driven and interactive sessions.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// Agent markers. Codex has no reliable marker: `CODEX_SANDBOX` only indicates
/// sandboxing, so its non-TTY sessions are covered by [`is_interactive`].
const AGENT_ENV_VARS: [&str; 3] = ["CLAUDECODE", "GEMINI_CLI", "CURSOR_AGENT"];

fn truthy(value: Option<String>) -> bool {
    matches!(value, Some(v) if !v.is_empty())
}

/// Environment lookup is injectable for tests.
fn detect_agent(lookup: impl Fn(&str) -> Option<String>) -> bool {
    AGENT_ENV_VARS.iter().any(|name| truthy(lookup(name)))
}

/// Whether any agent marker is non-empty, cached on first use.
pub fn is_run_by_agent() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| detect_agent(|name| std::env::var(name).ok()))
}

/// Interactive when stdout is a TTY and no agent marker is set.
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

    fn only(set_name: &'static str, value: &'static str) -> impl Fn(&str) -> Option<String> {
        move |name| (name == set_name).then(|| value.to_string())
    }

    #[test]
    fn each_agent_var_triggers_agent_mode() {
        for name in AGENT_ENV_VARS {
            assert!(
                detect_agent(only(name, "1")),
                "{name} should trigger agent mode"
            );
        }
    }

    #[test]
    fn no_agent_vars_means_not_agent_mode() {
        assert!(!detect_agent(|_| None));
        assert!(!detect_agent(only("CLAUDECODE", "")));
        // CODEX_SANDBOX identifies a sandbox, not an agent.
        assert!(!detect_agent(only("CODEX_SANDBOX", "seatbelt")));
    }
}
