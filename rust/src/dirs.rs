//! State and service working directories.

use std::path::{Component, Path, PathBuf};

/// Resolve and lexically normalize the service working directory.
/// An absolute `root` replaces `project_dir`.
pub fn resolve_launch_dir(project_dir: &str, root: Option<&str>) -> String {
    let joined = match root.filter(|r| !r.is_empty()) {
        Some(root) if Path::new(root).is_absolute() => PathBuf::from(root),
        Some(root) => Path::new(project_dir).join(root),
        None => PathBuf::from(project_dir),
    };
    normalize_path(&joined).to_string_lossy().into_owned()
}

/// Fold `.` and `..` without filesystem access; preserve leading relative `..`.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    // Tracks how many trailing components are `..`, which must not be popped.
    let mut pending_parents = 0usize;

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) if pending_parents == 0 => {
                    out.pop();
                }
                // The root is its own parent: `/..` is `/`, so drop the component.
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                // Preserve leading relative parents.
                _ => {
                    out.push("..");
                    pending_parents += 1;
                }
            },
            other => out.push(other.as_os_str()),
        }
    }

    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// State directory precedence: `CANDLE_DATABASE_DIR`, `XDG_STATE_HOME/candle`,
/// then `<home>/.local/state/candle`.
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

/// Read an environment variable, treating empty strings the same as unset.
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

    #[test]
    fn launch_dir_resolves_root() {
        assert_eq!(resolve_launch_dir("/proj", None), "/proj");
        assert_eq!(resolve_launch_dir("/proj", Some("")), "/proj");
        assert_eq!(resolve_launch_dir("/proj", Some("sub")), "/proj/sub");
        assert_eq!(
            resolve_launch_dir("/proj", Some("/elsewhere")),
            "/elsewhere"
        );
    }

    #[test]
    fn launch_dir_is_normalized() {
        assert_eq!(resolve_launch_dir("/proj", Some("./sub")), "/proj/sub");
        assert_eq!(resolve_launch_dir("/proj", Some("./a/./b")), "/proj/a/b");
        assert_eq!(resolve_launch_dir("/proj", Some("../sibling")), "/sibling");
        assert_eq!(
            resolve_launch_dir("/proj/nested", Some("../sub")),
            "/proj/sub"
        );
        assert_eq!(resolve_launch_dir("/proj/", Some("sub/")), "/proj/sub");
        assert_eq!(resolve_launch_dir("/", Some("../..")), "/");
        assert_eq!(resolve_launch_dir("relative", Some("../..")), "..");
        assert_eq!(resolve_launch_dir("/proj/./nested", None), "/proj/nested");
    }
}
