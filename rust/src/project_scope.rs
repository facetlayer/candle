//! Discover projects from the CWD, or use a literal --project-dir override.
//! Overrides need not exist, allowing cleanup of deleted projects.

use std::path::{Path, PathBuf};

use crate::config::{find_project_dir, CONFIG_FILENAME};
use crate::dirs::normalize_path;
use crate::errors::CandleError;

/// How a command decides which project directory it is acting on.
#[derive(Debug, Clone)]
pub enum ProjectScope {
    /// Discover the nearest config in this directory or its ancestors.
    Discover(PathBuf),
    /// Explicit project directory.
    Explicit(PathBuf),
}

impl ProjectScope {
    /// Build a scope, making explicit paths absolute and lexically normalized
    /// without filesystem access.
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

    /// Whether the project was named explicitly, allowing deleted-project cleanup.
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

    /// Require explicit projects to have their own config, preventing service
    /// lookup in one project with database rows keyed to another. Database-only
    /// commands skip this check so deleted projects remain accessible.
    pub fn require_own_config(&self) -> Result<(), CandleError> {
        let ProjectScope::Explicit(dir) = self else {
            return Ok(());
        };

        if dir.join(CONFIG_FILENAME).exists() {
            return Ok(());
        }

        Err(CandleError::MissingSetupFile {
            cwd: dir.display().to_string(),
            explicit: true,
        })
    }

    /// Resolve the database project key; explicit directories need no config.
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
        // An explicit child project must not borrow its ancestor's config.
        let parent = crate::config::test_support::TempDir::new();
        std::fs::write(parent.path().join(".candle.json"), "{}").unwrap();
        let child = parent.path().join("child");
        std::fs::create_dir_all(&child).unwrap();

        let scope = ProjectScope::new(cwd(), Some(&child.display().to_string()));
        assert!(scope.require_own_config().is_err());
    }

    #[test]
    fn missing_directory_still_resolves() {
        let scope = ProjectScope::new(cwd(), Some("/gone/project"));
        assert_eq!(scope.resolve().unwrap(), "/gone/project");
    }
}
