//! Lexical path validation and resolution helpers.
//!
//! Ported from `isValidRootPath` / `isValidRelativePath` / `getServiceCwd` in
//! `src/configFile.ts`. All operations are purely lexical (string-level) and
//! must NOT touch the filesystem — in particular do not use `canonicalize`,
//! which resolves symlinks and requires the path to exist. This mirrors Node's
//! `path.normalize`, which only folds `.` / `..` segments textually.

use std::path::{Path, PathBuf};

use crate::config::model::ServiceConfig;

/// POSIX absolute-path test (target platform is macOS/Linux, matching Node's
/// `path.isAbsolute` on those platforms).
pub fn is_absolute(p: &str) -> bool {
    p.starts_with('/')
}

/// Lexically normalize a POSIX-style path, folding `.` and `..` segments and
/// collapsing redundant separators. Mirrors `path.normalize`.
///
/// Note the deliberate quirk reproduced from the Node code: the validity check
/// is a STRING `starts_with("..")` test on the normalized result, so a path
/// segment literally named `..foo` normalizes to `..foo` and is therefore
/// treated as escaping.
pub fn lexical_normalize(p: &str) -> String {
    let is_abs = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();

    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                match out.last() {
                    Some(&last) if last != ".." => {
                        out.pop();
                    }
                    _ => {
                        if !is_abs {
                            out.push("..");
                        }
                        // For absolute paths, a leading `..` is dropped at root.
                    }
                }
            }
            seg => out.push(seg),
        }
    }

    let mut result = out.join("/");
    if is_abs {
        result = format!("/{result}");
    }
    if result.is_empty() {
        result = ".".to_string();
    }
    result
}

/// A root path is valid if it is absolute, or (after lexical normalization)
/// does not begin with `..`.
pub fn is_valid_root_path(p: &str) -> bool {
    if is_absolute(p) {
        return true;
    }
    !lexical_normalize(p).starts_with("..")
}

/// A relative path is valid if it is NOT absolute and (after lexical
/// normalization) does not begin with `..`.
pub fn is_valid_relative_path(p: &str) -> bool {
    if is_absolute(p) {
        return false;
    }
    !lexical_normalize(p).starts_with("..")
}

/// Lexical `path.resolve(base, p)`: absolute `p` wins; otherwise normalize the
/// join of `base` and `p`. `base` is assumed already absolute.
pub fn path_resolve(base: &Path, p: &str) -> PathBuf {
    if is_absolute(p) {
        PathBuf::from(lexical_normalize(p))
    } else {
        let joined = base.join(p);
        PathBuf::from(lexical_normalize(&joined.to_string_lossy()))
    }
}

/// Resolve a service's working directory given the config file path.
/// Mirrors `getServiceCwd`: `dirname(configPath)` joined with `service.root`
/// (absolute root wins), or just `dirname(configPath)` when no root is set.
pub fn get_service_cwd(config_path: &Path, service: &ServiceConfig) -> PathBuf {
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new(""));
    match &service.root {
        Some(root) if !root.is_empty() => path_resolve(config_dir, root),
        _ => config_dir.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_dot_dot() {
        assert_eq!(lexical_normalize("a/../b"), "b");
        assert_eq!(lexical_normalize("a/../../b"), "../b");
        assert_eq!(lexical_normalize("../x"), "../x");
        assert_eq!(lexical_normalize("./a"), "a");
        assert_eq!(lexical_normalize("foo"), "foo");
    }

    #[test]
    fn root_path_rules() {
        assert!(is_valid_root_path("/abs/path"));
        assert!(is_valid_root_path("a/../b")); // -> "b"
        assert!(is_valid_root_path("packages/api"));
        assert!(!is_valid_root_path("../x"));
        assert!(!is_valid_root_path("a/../../b")); // -> "../b"
        // Quirk: a segment literally named "..foo" starts with ".." so is invalid.
        assert!(!is_valid_root_path("..foo"));
    }

    #[test]
    fn relative_path_rules() {
        assert!(!is_valid_relative_path("/abs"));
        assert!(is_valid_relative_path("a/b"));
        assert!(!is_valid_relative_path("../escape"));
        assert!(!is_valid_relative_path("..foo"));
    }

    #[test]
    fn service_cwd_with_and_without_root() {
        let svc_no_root = ServiceConfig {
            name: "a".into(),
            shell: "x".into(),
            root: None,
            enable_stdin: None,
        };
        assert_eq!(
            get_service_cwd(Path::new("/proj/.candle.json"), &svc_no_root),
            PathBuf::from("/proj")
        );

        let svc_root = ServiceConfig {
            name: "a".into(),
            shell: "x".into(),
            root: Some("packages/api".into()),
            enable_stdin: None,
        };
        assert_eq!(
            get_service_cwd(Path::new("/proj/.candle.json"), &svc_root),
            PathBuf::from("/proj/packages/api")
        );

        let svc_abs = ServiceConfig {
            name: "a".into(),
            shell: "x".into(),
            root: Some("/elsewhere".into()),
            enable_stdin: None,
        };
        assert_eq!(
            get_service_cwd(Path::new("/proj/.candle.json"), &svc_abs),
            PathBuf::from("/elsewhere")
        );
    }
}
