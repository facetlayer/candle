//! Mutating config commands: setup-project, add-service, remove-service, set-config.
//!
//! Ported from `src/setup-project-command.ts`, `src/addServerConfig.ts`,
//! `src/removeServerConfig.ts`, and `src/set-config-command.ts`.
//!
//! Each function returns the success message string(s) the CLI layer should
//! print (rather than printing directly), or a [`CandleError`].

use std::path::{Path, PathBuf};

use crate::config::file::{find_config_file, read_config_file};
use crate::config::model::{
    CandleSetupConfig, LogEvictionConfig, ServiceConfig, DEFAULT_CONFIG_FILENAME,
};
use crate::config::validate::validate_config;
use crate::errors::CandleError;

/// Arguments for [`add_server_config`].
#[derive(Debug, Clone)]
pub struct AddServerConfigArgs {
    pub name: String,
    pub shell: String,
    pub root: Option<String>,
    pub enable_stdin: bool,
}

/// Re-run validation over a (possibly mutated) config by round-tripping it
/// through the same `validateConfig` path the Node code uses. Catches invalid
/// roots, duplicate names, etc. introduced by mutations.
fn revalidate(config: &CandleSetupConfig) -> Result<(), CandleError> {
    validate_config(config.to_value()).map(|_| ())
}

/// `setup-project`: create `.candle.json` if none exists at/above `cwd`.
/// Mirrors `handleSetupProject`.
pub fn handle_setup_project(cwd: &Path) -> Result<String, CandleError> {
    match find_config_file(cwd) {
        Ok(found) => {
            let config_path = found.project_dir.join(&found.config_filename);
            Ok(format!(
                "Config file already exists at {}",
                config_path.display()
            ))
        }
        Err(CandleError::MissingSetupFile { .. }) => {
            let config_path = cwd.join(DEFAULT_CONFIG_FILENAME);
            write_config_file(&config_path, &CandleSetupConfig::default())?;
            Ok(format!("Created {DEFAULT_CONFIG_FILENAME} in {}", cwd.display()))
        }
        Err(e) => Err(e),
    }
}

/// Find an existing config file (using its discovered filename) or create a new
/// `.candle.json` in `start_dir`. Mirrors `findOrCreateSetupFile`.
fn find_or_create_setup_file(start_dir: &Path) -> Result<PathBuf, CandleError> {
    match find_config_file(start_dir) {
        Ok(found) => Ok(found.project_dir.join(&found.config_filename)),
        Err(CandleError::MissingSetupFile { .. }) => {
            let config_path = start_dir.join(DEFAULT_CONFIG_FILENAME);
            write_config_file(&config_path, &CandleSetupConfig::default())?;
            Ok(config_path)
        }
        Err(e) => Err(e),
    }
}

/// `add-service`: add a new service to the config. Mirrors `addServerConfig`.
pub fn add_server_config(
    args: &AddServerConfigArgs,
    start_dir: &Path,
) -> Result<String, CandleError> {
    let config_path = find_or_create_setup_file(start_dir)?;
    let mut config = read_config_file(&config_path)?;

    if config.services.iter().any(|s| s.name == args.name) {
        return Err(CandleError::ConfigFileError(format!(
            "Service '{}' already exists in configuration",
            args.name
        )));
    }

    let new_service = ServiceConfig {
        name: args.name.clone(),
        shell: args.shell.clone(),
        root: args.root.clone().filter(|r| !r.is_empty()),
        enable_stdin: if args.enable_stdin { Some(true) } else { None },
    };

    config.services.push(new_service);
    revalidate(&config)?;
    write_config_file(&config_path, &config)?;

    Ok(format!(
        "Service '{}' added successfully to .candle.json",
        args.name
    ))
}

/// `remove-service`: remove a service by name. Mirrors `removeServerConfig`.
pub fn remove_server_config(name: &str, start_dir: &Path) -> Result<String, CandleError> {
    let found = find_config_file(start_dir)?;
    let config_path = found.project_dir.join(&found.config_filename);
    let mut config = read_config_file(&config_path)?;

    let original_len = config.services.len();
    config.services.retain(|s| s.name != name);

    if config.services.len() == original_len {
        return Err(CandleError::ConfigFileError(format!(
            "Service '{name}' not found in configuration"
        )));
    }

    revalidate(&config)?;
    write_config_file(&config_path, &config)?;

    Ok(format!("Service '{name}' removed from .candle.json"))
}

/// `set-config`: set a single config key. Mirrors `handleSetConfig`.
pub fn handle_set_config(key: &str, value: &str, cwd: &Path) -> Result<String, CandleError> {
    // Validate the key/value first (matching the Node order, before reading the file).
    let parsed = parse_config_value(key, value)?;

    let found = find_config_file(cwd)?;
    let config_path = found.project_dir.join(&found.config_filename);
    let mut config = read_config_file(&config_path)?;

    apply_config_value(&mut config, &parsed);
    revalidate(&config)?;
    write_config_file(&config_path, &config)?;

    Ok(format!(
        "Set '{key}' to '{value}' in {}",
        found.config_filename
    ))
}

/// A validated set-config assignment.
enum ParsedConfigValue {
    MaxLogsPerService(u64),
    MaxRetentionSeconds(u64),
}

const VALID_CONFIG_KEYS: [&str; 2] = [
    "logEviction.maxLogsPerService",
    "logEviction.maxRetentionSeconds",
];

fn parse_config_value(key: &str, value: &str) -> Result<ParsedConfigValue, CandleError> {
    match key {
        "logEviction.maxLogsPerService" => js_positive_int(value)
            .map(ParsedConfigValue::MaxLogsPerService)
            .ok_or_else(|| {
                CandleError::UsageError(
                    "Invalid value for 'logEviction.maxLogsPerService': expected a positive integer"
                        .to_string(),
                )
            }),
        "logEviction.maxRetentionSeconds" => js_positive_int(value)
            .map(ParsedConfigValue::MaxRetentionSeconds)
            .ok_or_else(|| {
                CandleError::UsageError(
                    "Invalid value for 'logEviction.maxRetentionSeconds': expected a positive integer"
                        .to_string(),
                )
            }),
        _ => Err(CandleError::UsageError(format!(
            "Unknown config key '{key}'. Valid keys: {}",
            VALID_CONFIG_KEYS.join(", ")
        ))),
    }
}

fn apply_config_value(config: &mut CandleSetupConfig, parsed: &ParsedConfigValue) {
    match parsed {
        ParsedConfigValue::MaxLogsPerService(n) => {
            let le = config.log_eviction.get_or_insert_with(LogEvictionConfig::default);
            le.max_logs_per_service = Some(*n);
            config.ensure_key("logEviction");
        }
        ParsedConfigValue::MaxRetentionSeconds(n) => {
            let le = config.log_eviction.get_or_insert_with(LogEvictionConfig::default);
            le.max_retention_seconds = Some(*n);
            config.ensure_key("logEviction");
        }
    }
}

/// Mimic JS `Number(value)` and require an integer `>= 1`.
///
/// Matches the JS coercion quirks: leading/trailing whitespace is trimmed,
/// empty string -> 0 (rejected), `1e3` and `0x10` are accepted, `3.5` / `3abc`
/// are rejected.
fn js_positive_int(value: &str) -> Option<u64> {
    let num = js_number(value)?;
    if num.is_finite() && num.fract() == 0.0 && num >= 1.0 {
        Some(num as u64)
    } else {
        None
    }
}

/// Mimic JS `Number(value)` coercion, returning `None` for `NaN`.
fn js_number(input: &str) -> Option<f64> {
    let s = input.trim();
    if s.is_empty() {
        return Some(0.0);
    }

    // Radix prefixes (no sign allowed, matching JS).
    let radix = |prefix_lower: &str, prefix_upper: &str, base: u32| -> Option<Option<f64>> {
        let body = s
            .strip_prefix(prefix_lower)
            .or_else(|| s.strip_prefix(prefix_upper));
        body.map(|body| {
            i128::from_str_radix(body, base)
                .ok()
                .map(|v| v as f64)
        })
    };
    if let Some(r) = radix("0x", "0X", 16) {
        return r;
    }
    if let Some(r) = radix("0o", "0O", 8) {
        return r;
    }
    if let Some(r) = radix("0b", "0B", 2) {
        return r;
    }

    match s {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }

    // Reject Rust-accepted-but-JS-rejected spellings of inf/nan. Any remaining
    // valid decimal (including `1e3`) parses here.
    let lower = s.to_ascii_lowercase();
    if lower.contains("inf") || lower.contains("nan") {
        return None;
    }

    s.parse::<f64>().ok()
}

fn write_config_file(path: &Path, config: &CandleSetupConfig) -> Result<(), CandleError> {
    std::fs::write(path, config.to_json_string())
        .map_err(|e| CandleError::ConfigFileError(format!("Failed to write {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempDir;

    fn read_to_string(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn js_number_coercion_table() {
        assert_eq!(js_positive_int("5"), Some(5));
        assert_eq!(js_positive_int(" 5 "), Some(5));
        assert_eq!(js_positive_int("1e3"), Some(1000));
        assert_eq!(js_positive_int("0x10"), Some(16));
        assert_eq!(js_positive_int("3.5"), None);
        assert_eq!(js_positive_int("3abc"), None);
        assert_eq!(js_positive_int(""), None); // -> 0, fails >= 1
        assert_eq!(js_positive_int("0"), None);
        assert_eq!(js_positive_int("-5"), None);
        assert_eq!(js_positive_int("Infinity"), None);
        assert_eq!(js_positive_int("nan"), None);
    }

    #[test]
    fn setup_project_creates_file() {
        let dir = TempDir::new();
        let msg = handle_setup_project(dir.path()).unwrap();
        assert_eq!(msg, format!("Created .candle.json in {}", dir.path().display()));
        let contents = read_to_string(&dir.path().join(".candle.json"));
        assert_eq!(contents, "{\n  \"services\": []\n}");
    }

    #[test]
    fn setup_project_existing_file() {
        let dir = TempDir::new();
        let path = dir.path().join(".candle.json");
        std::fs::write(&path, "{\n  \"services\": []\n}").unwrap();
        let msg = handle_setup_project(dir.path()).unwrap();
        assert_eq!(msg, format!("Config file already exists at {}", path.display()));
    }

    #[test]
    fn add_service_creates_and_appends() {
        let dir = TempDir::new();
        let args = AddServerConfigArgs {
            name: "api".to_string(),
            shell: "npm run dev".to_string(),
            root: None,
            enable_stdin: false,
        };
        let msg = add_server_config(&args, dir.path()).unwrap();
        assert_eq!(msg, "Service 'api' added successfully to .candle.json");
        let contents = read_to_string(&dir.path().join(".candle.json"));
        assert_eq!(
            contents,
            "{\n  \"services\": [\n    {\n      \"name\": \"api\",\n      \"shell\": \"npm run dev\"\n    }\n  ]\n}"
        );
    }

    #[test]
    fn add_service_with_root_and_stdin() {
        let dir = TempDir::new();
        let args = AddServerConfigArgs {
            name: "api".to_string(),
            shell: "cmd".to_string(),
            root: Some("packages/api".to_string()),
            enable_stdin: true,
        };
        add_server_config(&args, dir.path()).unwrap();
        let contents = read_to_string(&dir.path().join(".candle.json"));
        assert_eq!(
            contents,
            "{\n  \"services\": [\n    {\n      \"name\": \"api\",\n      \"shell\": \"cmd\",\n      \"root\": \"packages/api\",\n      \"enableStdin\": true\n    }\n  ]\n}"
        );
    }

    #[test]
    fn add_service_duplicate_errors() {
        let dir = TempDir::new();
        let args = AddServerConfigArgs {
            name: "api".to_string(),
            shell: "cmd".to_string(),
            root: None,
            enable_stdin: false,
        };
        add_server_config(&args, dir.path()).unwrap();
        let err = add_server_config(&args, dir.path()).unwrap_err();
        assert_eq!(err.to_string(), "Service 'api' already exists in configuration");
    }

    #[test]
    fn add_service_invalid_root_errors() {
        let dir = TempDir::new();
        let args = AddServerConfigArgs {
            name: "api".to_string(),
            shell: "cmd".to_string(),
            root: Some("../escape".to_string()),
            enable_stdin: false,
        };
        let err = add_server_config(&args, dir.path()).unwrap_err();
        assert_eq!(err.to_string(), "Service \"api\" has invalid root path: \"../escape\"");
    }

    #[test]
    fn remove_service_works_and_missing_errors() {
        let dir = TempDir::new();
        std::fs::write(
            dir.path().join(".candle.json"),
            "{\n  \"services\": [\n    {\n      \"name\": \"api\",\n      \"shell\": \"x\"\n    }\n  ]\n}",
        )
        .unwrap();
        let msg = remove_server_config("api", dir.path()).unwrap();
        assert_eq!(msg, "Service 'api' removed from .candle.json");
        let contents = read_to_string(&dir.path().join(".candle.json"));
        assert_eq!(contents, "{\n  \"services\": []\n}");

        let err = remove_server_config("api", dir.path()).unwrap_err();
        assert_eq!(err.to_string(), "Service 'api' not found in configuration");
    }

    #[test]
    fn set_config_log_eviction_number() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{\n  \"services\": []\n}").unwrap();
        let msg = handle_set_config("logEviction.maxLogsPerService", "5000", dir.path()).unwrap();
        assert_eq!(msg, "Set 'logEviction.maxLogsPerService' to '5000' in .candle.json");
        let contents = read_to_string(&dir.path().join(".candle.json"));
        assert_eq!(
            contents,
            "{\n  \"services\": [],\n  \"logEviction\": {\n    \"maxLogsPerService\": 5000\n  }\n}"
        );
    }

    #[test]
    fn set_config_unknown_key() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{\n  \"services\": []\n}").unwrap();
        let err = handle_set_config("bogus", "1", dir.path()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Unknown config key 'bogus'. Valid keys: logEviction.maxLogsPerService, logEviction.maxRetentionSeconds"
        );
        assert!(err.is_usage_error());
    }

    #[test]
    fn set_config_retired_log_collector_key() {
        // `logCollector` picked between the old Node and Rust collector sidecars.
        // Both are gone (supervision is `candle --monitor`), so the key is retired.
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{\n  \"services\": []\n}").unwrap();
        let err = handle_set_config("logCollector", "rust", dir.path()).unwrap_err();
        assert!(err.to_string().starts_with("Unknown config key 'logCollector'"));
        assert!(err.is_usage_error());
    }

    #[test]
    fn set_config_invalid_integer() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{\n  \"services\": []\n}").unwrap();
        let err = handle_set_config("logEviction.maxRetentionSeconds", "3.5", dir.path()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid value for 'logEviction.maxRetentionSeconds': expected a positive integer"
        );
    }
}
