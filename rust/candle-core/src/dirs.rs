//! State / database directory resolution.
//!
//! Ported from `src/dirs.ts`. Resolves the state directory where the SQLite
//! database lives, using the same precedence as the Node implementation.

use std::path::PathBuf;

/// Resolve the state directory, given the relevant environment values.
///
/// This is a pure helper so it can be unit-tested without racing on process
/// environment. Precedence (matching `src/dirs.ts`):
///
/// 1. `CANDLE_DATABASE_DIR` -> used verbatim.
/// 2. `XDG_STATE_HOME` -> `<XDG_STATE_HOME>/candle`.
/// 3. Default -> `<home>/.local/state/candle`.
pub fn resolve_state_dir(
    candle_database_dir: Option<&str>,
    xdg_state_home: Option<&str>,
    home: &str,
) -> PathBuf {
    if let Some(dir) = candle_database_dir {
        return PathBuf::from(dir);
    }

    if let Some(xdg) = xdg_state_home {
        let mut path = PathBuf::from(xdg);
        path.push("candle");
        return path;
    }

    let mut path = PathBuf::from(home);
    path.push(".local");
    path.push("state");
    path.push("candle");
    path
}

/// Read the relevant environment variables and resolve the state directory.
///
/// Mirrors `getStateDirectory()` in `src/dirs.ts`.
pub fn get_state_directory() -> PathBuf {
    let candle_database_dir = non_empty_env("CANDLE_DATABASE_DIR");
    let xdg_state_home = non_empty_env("XDG_STATE_HOME");
    let home = std::env::var("HOME").unwrap_or_default();

    resolve_state_dir(
        candle_database_dir.as_deref(),
        xdg_state_home.as_deref(),
        &home,
    )
}

/// Path to the `candle.db` file inside the resolved state directory.
pub fn candle_db_path() -> PathBuf {
    let mut path = get_state_directory();
    path.push("candle.db");
    path
}

/// Read an environment variable, treating empty strings the same as unset
/// (matching JS truthiness of `process.env.X`).
fn non_empty_env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candle_database_dir_takes_precedence_verbatim() {
        let result = resolve_state_dir(Some("/custom/db"), Some("/xdg"), "/home/user");
        assert_eq!(result, PathBuf::from("/custom/db"));
    }

    #[test]
    fn xdg_state_home_appends_candle() {
        let result = resolve_state_dir(None, Some("/xdg/state"), "/home/user");
        assert_eq!(result, PathBuf::from("/xdg/state/candle"));
    }

    #[test]
    fn default_uses_home_local_state() {
        let result = resolve_state_dir(None, None, "/home/user");
        assert_eq!(result, PathBuf::from("/home/user/.local/state/candle"));
    }

    #[test]
    fn candle_database_dir_wins_over_xdg() {
        let result = resolve_state_dir(Some("/verbatim"), Some("/xdg"), "/home/user");
        assert_eq!(result, PathBuf::from("/verbatim"));
    }
}
