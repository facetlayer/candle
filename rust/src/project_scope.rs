//! Which project a command acts on.
//!
//! By default a command discovers its project the way `git` does: start at the
//! CWD and walk up to the nearest config file. `--project-dir <dir>` overrides
//! that — the named directory *is* the project, used exactly as given.
//!
//! The override is deliberately literal. It never walks up to an ancestor, and
//! it never checks that the directory (or its config file) exists, because its
//! main use is cleaning up after a project directory that has been deleted:
//! `candle kill --project-dir /gone/project` has to be able to name a project
//! that is no longer on disk.

use std::path::{Path, PathBuf};

use crate::config::{find_project_dir, CONFIG_FILENAMES};
use crate::dirs::normalize_path;
use crate::errors::CandleError;

/// How a command decides which project directory it is acting on.
#[derive(Debug, Clone)]
pub enum ProjectScope {
    /// No `--project-dir`: search this directory and its ancestors for a config
    /// file. The project is wherever that file is found.
    Discover(PathBuf),
    /// `--project-dir <dir>`: this directory is the project, verbatim.
    Explicit(PathBuf),
}

impl ProjectScope {
    /// Build a scope from the CWD and the `--project-dir` value, if any.
    ///
    /// An explicit directory is made absolute against the CWD and lexically
    /// normalized, so `--project-dir .` and `--project-dir ./sub/..` name the
    /// same project as plain discovery would. Normalization is textual: the
    /// path is never resolved against the filesystem, which may no longer have
    /// it.
    pub fn new(cwd: PathBuf, project_dir_flag: Option<&str>) -> Self {
        match project_dir_flag.filter(|d| !d.is_empty()) {
            Some(dir) => {
                let path = Path::new(dir);
                let absolute = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    cwd.join(path)
                };
                ProjectScope::Explicit(normalize_path(&absolute))
            }
            None => ProjectScope::Discover(cwd),
        }
    }

    /// Whether the project directory was named explicitly.
    ///
    /// Commands use this to relax checks that assume a project still exists —
    /// see the module docs.
    pub fn is_explicit(&self) -> bool {
        matches!(self, ProjectScope::Explicit(_))
    }

    /// The directory to read config from: the explicit project directory, or
    /// the CWD to start discovery at.
    pub fn base_dir(&self) -> &Path {
        match self {
            ProjectScope::Discover(cwd) => cwd,
            ProjectScope::Explicit(dir) => dir,
        }
    }

    /// Require an explicit project directory to hold a config file of its own.
    ///
    /// Config discovery walks up to ancestors. That is right for the CWD, but
    /// for an explicitly named project it would resolve services out of a
    /// *parent* project while the process rows stay keyed to the directory the
    /// user named — one command acting on two projects at once.
    ///
    /// Commands that need service definitions call this; the ones that work
    /// purely off database rows (`kill`, `logs`, `clear-logs`, `wait-for-log`)
    /// do not, so they keep working for a project that is gone.
    pub fn require_own_config(&self) -> Result<(), CandleError> {
        let ProjectScope::Explicit(dir) = self else {
            return Ok(());
        };

        if CONFIG_FILENAMES.iter().any(|name| dir.join(name).exists()) {
            return Ok(());
        }

        Err(CandleError::MissingSetupFile {
            cwd: dir.display().to_string(),
            explicit: true,
        })
    }

    /// The project directory that keys this project's rows in the database.
    ///
    /// Discovery can fail (no config file anywhere above the CWD); an explicit
    /// directory never does.
    pub fn resolve(&self) -> Result<String, CandleError> {
        match self {
            ProjectScope::Discover(cwd) => Ok(find_project_dir(cwd)?.display().to_string()),
            ProjectScope::Explicit(dir) => Ok(dir.display().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> PathBuf {
        PathBuf::from("/home/user/proj")
    }

    #[test]
    fn no_flag_discovers_from_cwd() {
        let scope = ProjectScope::new(cwd(), None);
        assert!(!scope.is_explicit());
        assert_eq!(scope.base_dir(), Path::new("/home/user/proj"));
    }

    #[test]
    fn empty_flag_is_treated_as_absent() {
        // `--project-dir=` should not silently target the filesystem root.
        let scope = ProjectScope::new(cwd(), Some(""));
        assert!(!scope.is_explicit());
    }

    #[test]
    fn absolute_flag_is_used_as_is() {
        let scope = ProjectScope::new(cwd(), Some("/srv/other"));
        assert!(scope.is_explicit());
        assert_eq!(scope.resolve().unwrap(), "/srv/other");
    }

    #[test]
    fn relative_flag_resolves_against_cwd() {
        let scope = ProjectScope::new(cwd(), Some("../sibling"));
        assert_eq!(scope.resolve().unwrap(), "/home/user/sibling");
    }

    #[test]
    fn flag_is_normalized_like_a_discovered_dir() {
        assert_eq!(
            ProjectScope::new(cwd(), Some(".")).resolve().unwrap(),
            "/home/user/proj"
        );
        assert_eq!(
            ProjectScope::new(cwd(), Some("/srv/a/./b/../c"))
                .resolve()
                .unwrap(),
            "/srv/a/c"
        );
    }

    #[test]
    fn discovery_never_requires_its_own_config() {
        // Walking up is the whole point when no --project-dir was given.
        assert!(ProjectScope::new(cwd(), None).require_own_config().is_ok());
    }

    #[test]
    fn explicit_dir_without_a_config_is_rejected() {
        let scope = ProjectScope::new(cwd(), Some("/gone/project"));
        let err = scope.require_own_config().unwrap_err();
        assert!(err.to_string().contains("/gone/project"), "{err}");
    }

    #[test]
    fn explicit_dir_with_a_config_is_accepted() {
        let dir = crate::config::test_support::TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{}").unwrap();

        let scope = ProjectScope::new(cwd(), Some(&dir.path().display().to_string()));
        assert!(scope.require_own_config().is_ok());
    }

    #[test]
    fn explicit_dir_is_not_rescued_by_an_ancestor_config() {
        // The ancestor describes a different project; naming the child must not
        // silently borrow it.
        let parent = crate::config::test_support::TempDir::new();
        std::fs::write(parent.path().join(".candle.json"), "{}").unwrap();
        let child = parent.path().join("child");
        std::fs::create_dir_all(&child).unwrap();

        let scope = ProjectScope::new(cwd(), Some(&child.display().to_string()));
        assert!(scope.require_own_config().is_err());
    }

    #[test]
    fn missing_directory_still_resolves() {
        // The whole point of the override: name a project that is gone.
        let scope = ProjectScope::new(cwd(), Some("/gone/project"));
        assert_eq!(scope.resolve().unwrap(), "/gone/project");
    }
}
