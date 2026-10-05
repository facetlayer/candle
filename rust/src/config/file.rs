//! Config file IO, discovery, and service lookup.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::model::{
    CandleSetupConfig, ResolvedLogEvictionConfig, ServiceConfig, CONFIG_FILENAME,
    LOG_EVICTION_DEFAULTS,
};
use crate::config::paths::path_resolve;
use crate::config::validate::{unknown_key_warnings, validate_config};
use crate::errors::CandleError;

/// Result of locating a config file in the directory tree.
#[derive(Debug, Clone)]
pub struct FoundConfig {
    pub config: CandleSetupConfig,
    pub project_dir: PathBuf,
}

/// Result of resolving a service by name.
#[derive(Debug, Clone)]
pub struct FoundServiceConfig {
    pub service_config: ServiceConfig,
    pub project_dir: PathBuf,
}

/// Parse, normalize, and validate config. Empty files yield no services.
pub fn read_config_file(config_file_path: &Path) -> Result<CandleSetupConfig, CandleError> {
    let content = std::fs::read_to_string(config_file_path).map_err(|e| {
        CandleError::ConfigFileError(format!(
            "Failed to read {}: {e}",
            config_file_path.display()
        ))
    })?;
    let trimmed = content.trim();

    if trimmed.is_empty() {
        return Ok(CandleSetupConfig::default());
    }

    let value: Value =
        serde_json::from_str(trimmed).map_err(|e| CandleError::ConfigFileError(e.to_string()))?;

    let filename = config_file_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    for warning in unknown_key_warnings(&value, &filename) {
        warn_once(&warning);
    }

    validate_config(value)
}

/// Emit each config warning once per process.
fn warn_once(message: &str) {
    use std::collections::HashSet;
    use std::sync::Mutex;
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen
        .get_or_insert_with(HashSet::new)
        .insert(message.to_string())
    {
        crate::output::err(message);
    }
}

/// Make a path absolute lexically, without touching the filesystem.
fn to_absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Find the nearest config in this directory or its ancestors.
/// Wrap read/parse errors with the path; report the starting directory if absent.
pub fn find_config_file(start_dir: &Path) -> Result<FoundConfig, CandleError> {
    let starting_dir = start_dir.to_path_buf();
    let mut current = to_absolute(start_dir);

    loop {
        let config_file_path = current.join(CONFIG_FILENAME);
        if config_file_path.exists() {
            return match read_config_file(&config_file_path) {
                Ok(config) => Ok(FoundConfig {
                    config,
                    project_dir: current.clone(),
                }),
                Err(e) => Err(CandleError::ConfigFileError(format!(
                    "Invalid {CONFIG_FILENAME} at {}: {e}",
                    config_file_path.display()
                ))),
            };
        }

        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }

    Err(CandleError::MissingSetupFile {
        cwd: starting_dir.display().to_string(),
        explicit: false,
    })
}

/// Find the project directory (the directory containing the nearest config file).
pub fn find_project_dir(cwd: &Path) -> Result<PathBuf, CandleError> {
    Ok(find_config_file(cwd)?.project_dir)
}

/// Resolve log-eviction settings, applying defaults.
pub fn get_log_eviction_config(config: Option<&CandleSetupConfig>) -> ResolvedLogEvictionConfig {
    let le = config.and_then(|c| c.log_eviction.as_ref());
    ResolvedLogEvictionConfig {
        max_logs_per_service: le
            .and_then(|e| e.max_logs_per_service)
            .unwrap_or(LOG_EVICTION_DEFAULTS.max_logs_per_service),
        max_retention_seconds: le
            .and_then(|e| e.max_retention_seconds)
            .unwrap_or(LOG_EVICTION_DEFAULTS.max_retention_seconds),
    }
}

/// Exact-name service lookup.
pub fn find_service_by_name<'a>(
    config: &'a CandleSetupConfig,
    name: &str,
) -> Option<&'a ServiceConfig> {
    config.services.iter().find(|s| s.name == name)
}

/// All configured service names.
pub fn get_all_service_names(config: &CandleSetupConfig) -> Vec<String> {
    config.services.iter().map(|s| s.name.clone()).collect()
}

/// If `command_names` is non-empty, return it unchanged; otherwise return all
/// service names from the project config.
pub fn resolve_command_names_or_all(
    project_dir: &Path,
    command_names: &[String],
) -> Result<Vec<String>, CandleError> {
    if !command_names.is_empty() {
        return Ok(command_names.to_vec());
    }
    let found = find_config_file(project_dir)?;
    let names = get_all_service_names(&found.config);
    if names.is_empty() {
        return Err(CandleError::UsageError(
            "No services configured in .candle.json".to_string(),
        ));
    }
    Ok(names)
}

/// Match name substrings, preferring service roots in the search directory,
/// then its ancestors. Multiple matches at one level are ambiguous.
pub fn find_loose_command_name(
    command_name: &str,
    config: &CandleSetupConfig,
    project_dir: &Path,
    current_dir: &Path,
) -> Result<Option<ServiceConfig>, CandleError> {
    let search_dir = current_dir.to_path_buf();

    let matching: Vec<&ServiceConfig> = config
        .services
        .iter()
        .filter(|s| s.name.contains(command_name))
        .collect();

    if matching.is_empty() {
        match search_dir.parent() {
            Some(parent) if parent != search_dir && parent != project_dir => {
                return find_loose_command_name(command_name, config, project_dir, parent);
            }
            _ => return Ok(None),
        }
    }

    let with_matching_root: Vec<&ServiceConfig> = matching
        .iter()
        .copied()
        .filter(|service| match &service.root {
            None => search_dir == project_dir,
            Some(root) if root.is_empty() => search_dir == project_dir,
            Some(root) => path_resolve(project_dir, root) == search_dir,
        })
        .collect();

    if with_matching_root.len() == 1 {
        return Ok(Some(with_matching_root[0].clone()));
    } else if with_matching_root.len() > 1 {
        let names: Vec<&str> = with_matching_root.iter().map(|s| s.name.as_str()).collect();
        return Err(CandleError::ConfigFileError(format!(
            "Ambiguous service name \"{command_name}\". Multiple services match in current directory: {}",
            names.join(", ")
        )));
    }

    match search_dir.parent() {
        Some(parent) if parent != search_dir && parent != project_dir => {
            find_loose_command_name(command_name, config, project_dir, parent)
        }
        _ => {
            // At the project root, accept a single remaining substring match.
            if matching.len() == 1 {
                Ok(Some(matching[0].clone()))
            } else {
                Ok(None)
            }
        }
    }
}

/// Resolve a service by name (exact match first, then loose matching).
pub fn get_service_config_by_name(
    command_name: &str,
    current_dir: Option<&Path>,
) -> Result<FoundServiceConfig, CandleError> {
    let cwd;
    let dir: &Path = match current_dir {
        Some(d) => d,
        None => {
            cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            &cwd
        }
    };

    let found = find_config_file(dir)?;
    let FoundConfig {
        config,
        project_dir,
        ..
    } = found;

    let service_config = match find_service_by_name(&config, command_name) {
        Some(s) => Some(s.clone()),
        None => find_loose_command_name(command_name, &config, &project_dir, dir)?,
    };

    match service_config {
        Some(service_config) => Ok(FoundServiceConfig {
            service_config,
            project_dir,
        }),
        None => Err(CandleError::unknown_service(
            command_name,
            project_dir.display(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempDir;

    #[test]
    fn empty_file_is_valid() {
        let dir = TempDir::new();
        let path = dir.path().join(".candle.json");
        std::fs::write(&path, "   \n  ").unwrap();
        let cfg = read_config_file(&path).unwrap();
        assert!(cfg.services.is_empty());
    }

    #[test]
    fn upward_discovery_finds_parent_config() {
        let dir = TempDir::new();
        std::fs::write(
            dir.path().join(".candle.json"),
            "{\n  \"services\": [\n    {\n      \"name\": \"api\",\n      \"shell\": \"x\"\n    }\n  ]\n}",
        )
        .unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_config_file(&nested).unwrap();
        assert_eq!(found.project_dir, dir.path());
        assert_eq!(found.config.services.len(), 1);
    }

    #[test]
    fn missing_setup_file_reports_starting_dir() {
        let dir = TempDir::new();
        let start = dir.path().join("no").join("config").join("here");
        std::fs::create_dir_all(&start).unwrap();
        let err = find_config_file(&start).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "No .candle.json file found in (or above) current directory: {}\nTo create one, run `candle add-service <name> --shell <cmd>` or `candle setup-project`.",
                start.display()
            )
        );
        assert!(matches!(err, CandleError::MissingSetupFile { .. }));
    }

    #[test]
    fn parse_error_is_wrapped() {
        let dir = TempDir::new();
        let path = dir.path().join(".candle.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = find_config_file(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.starts_with(&format!("Invalid .candle.json at {}:", path.display())),
            "got: {msg}"
        );
        // Wrapping loses the MissingSetupFile type.
        assert!(matches!(err, CandleError::ConfigFileError(_)));
    }

    #[test]
    fn resolve_command_names_empty_returns_all() {
        let dir = TempDir::new();
        std::fs::write(
            dir.path().join(".candle.json"),
            "{\n  \"services\": [ { \"name\": \"a\", \"shell\": \"x\" }, { \"name\": \"b\", \"shell\": \"y\" } ]\n}",
        )
        .unwrap();
        let names = resolve_command_names_or_all(dir.path(), &[]).unwrap();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn resolve_command_names_zero_services_errors() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{\n  \"services\": []\n}").unwrap();
        let err = resolve_command_names_or_all(dir.path(), &[]).unwrap_err();
        assert_eq!(err.to_string(), "No services configured in .candle.json");
        assert!(matches!(err, CandleError::UsageError(_)));
    }

    #[test]
    fn loose_match_substring_fallback() {
        let dir = TempDir::new();
        std::fs::write(
            dir.path().join(".candle.json"),
            "{\n  \"services\": [ { \"name\": \"my-api-server\", \"shell\": \"x\" } ]\n}",
        )
        .unwrap();
        let found = get_service_config_by_name("api", Some(dir.path())).unwrap();
        assert_eq!(found.service_config.name, "my-api-server");
    }

    #[test]
    fn loose_match_ambiguity_in_directory() {
        let dir = TempDir::new();
        // Two services with no root both "match" the project directory.
        std::fs::write(
            dir.path().join(".candle.json"),
            "{\n  \"services\": [ { \"name\": \"api-one\", \"shell\": \"x\" }, { \"name\": \"api-two\", \"shell\": \"y\" } ]\n}",
        )
        .unwrap();
        // Canonicalize to match the discovered project directory.
        let err = get_service_config_by_name("api", Some(dir.path())).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("Ambiguous service name \"api\""),
            "got: {err}"
        );
    }

    #[test]
    fn missing_service_error() {
        let dir = TempDir::new();
        std::fs::write(
            dir.path().join(".candle.json"),
            "{\n  \"services\": [ { \"name\": \"api\", \"shell\": \"x\" } ]\n}",
        )
        .unwrap();
        let err = get_service_config_by_name("nope", Some(dir.path())).unwrap_err();
        assert!(matches!(err, CandleError::MissingServiceWithName { .. }));
        assert!(err
            .to_string()
            .starts_with("No service 'nope' configured for directory:"));
    }
}
