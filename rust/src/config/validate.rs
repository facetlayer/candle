//! Config validation and normalization.
//!
//! Ported from `validateConfig` in `src/configFile.ts`. Takes a parsed JSON
//! value and returns a normalized, validated [`CandleSetupConfig`], with the
//! exact `ConfigFileError` messages from the Node implementation.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::config::model::{CandleSetupConfig, LogEvictionConfig, ServiceConfig};
use crate::config::paths::is_valid_root_path;
use crate::errors::CandleError;

const KNOWN_TOP_LEVEL_KEYS: [&str; 2] = ["services", "logEviction"];

/// JS truthiness for a JSON value (used to mirror `config.services || []`).
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        // Arrays and objects are always truthy in JS.
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Validate and normalize a parsed config object.
///
/// Mirrors `validateConfig` plus the `config.services ||= []` normalization that
/// `readConfigFile` applies just before calling it.
pub fn validate_config(value: Value) -> Result<CandleSetupConfig, CandleError> {
    let obj: Map<String, Value> = match value {
        Value::Object(m) => m,
        other => {
            // The Node code operates directly on the parsed value; a non-object
            // top level is not exercised in practice. Treat it as invalid
            // `services` for a clear, deterministic error.
            return Err(CandleError::ConfigFileError(format!(
                "Config file error: Invalid value for 'services': {}",
                serde_json::to_string(&other).unwrap_or_default()
            )));
        }
    };

    // Normalize `services`: missing or falsy -> []. (JS: `config.services || []`.)
    let services_value = match obj.get("services") {
        Some(v) if is_truthy(v) => v.clone(),
        _ => Value::Array(Vec::new()),
    };

    let services = parse_services(services_value)?;
    let log_eviction = parse_log_eviction(obj.get("logEviction"))?;

    // Capture top-level key order. JS's `config.services ||= []` adds a
    // `services` key when absent, so reproduce that here.
    let mut key_order: Vec<String> = obj.keys().cloned().collect();
    if !key_order.iter().any(|k| k == "services") {
        key_order.push("services".to_string());
    }

    // Preserve unknown top-level keys verbatim.
    let mut extra = Map::new();
    for (k, v) in &obj {
        if !KNOWN_TOP_LEVEL_KEYS.contains(&k.as_str()) {
            extra.insert(k.clone(), v.clone());
        }
    }

    Ok(CandleSetupConfig {
        services,
        log_eviction,
        key_order,
        extra,
    })
}

fn parse_services(services_value: Value) -> Result<Vec<ServiceConfig>, CandleError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<ServiceConfig> = Vec::new();

    match services_value {
        Value::Array(arr) => {
            for entry in arr {
                out.push(parse_service(&entry, &mut seen)?);
            }
        }
        Value::Object(map) => {
            // Object-map form: each [key, value] becomes { name: key, ...value }.
            // The spread comes *after* `name`, so a `name` inside the value wins.
            for (key, value) in map {
                let mut merged = Map::new();
                merged.insert("name".to_string(), Value::String(key));
                if let Value::Object(vmap) = value {
                    for (k, v) in vmap {
                        merged.insert(k, v);
                    }
                }
                out.push(parse_service(&Value::Object(merged), &mut seen)?);
            }
        }
        other => {
            return Err(CandleError::ConfigFileError(format!(
                "Config file error: Invalid value for 'services': {}",
                serde_json::to_string(&other).unwrap_or_default()
            )));
        }
    }

    Ok(out)
}

fn parse_service(
    value: &Value,
    seen: &mut HashSet<String>,
) -> Result<ServiceConfig, CandleError> {
    // name: must be a non-empty string.
    let name = match value.get("name") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => {
            return Err(CandleError::ConfigFileError(
                "Config file error: Each service must have a \"name\" string".to_string(),
            ));
        }
    };

    // shell: must be a non-empty string.
    let shell = match value.get("shell") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => {
            return Err(CandleError::ConfigFileError(format!(
                "Config file error: Service \"{name}\" must have a \"shell\" string"
            )));
        }
    };

    if seen.contains(&name) {
        return Err(CandleError::ConfigFileError(format!(
            "Config file error: Duplicate service name: \"{name}\""
        )));
    }
    seen.insert(name.clone());

    // root: only validate when present and truthy (non-empty string).
    let root = match value.get("root") {
        Some(Value::String(s)) => {
            if !s.is_empty() && !is_valid_root_path(s) {
                return Err(CandleError::ConfigFileError(format!(
                    "Service \"{name}\" has invalid root path: \"{s}\""
                )));
            }
            Some(s.clone())
        }
        _ => None,
    };

    let enable_stdin = value.get("enableStdin").and_then(Value::as_bool);

    Ok(ServiceConfig {
        name,
        shell,
        root,
        enable_stdin,
    })
}

fn parse_log_eviction(
    value: Option<&Value>,
) -> Result<Option<LogEvictionConfig>, CandleError> {
    let value = match value {
        None => return Ok(None),
        Some(v) => v,
    };

    let map = match value {
        Value::Object(m) => m,
        _ => {
            return Err(CandleError::ConfigFileError(
                "Config file error: Invalid value for 'logEviction': expected an object".to_string(),
            ));
        }
    };

    let max_logs_per_service =
        parse_optional_positive_int(map.get("maxLogsPerService"), "maxLogsPerService")?;
    let max_retention_seconds =
        parse_optional_positive_int(map.get("maxRetentionSeconds"), "maxRetentionSeconds")?;

    Ok(Some(LogEvictionConfig {
        max_logs_per_service,
        max_retention_seconds,
    }))
}

fn parse_optional_positive_int(
    value: Option<&Value>,
    field: &str,
) -> Result<Option<u64>, CandleError> {
    let value = match value {
        None => return Ok(None),
        Some(v) => v,
    };

    let as_int = value
        .as_f64()
        .filter(|f| f.is_finite() && f.fract() == 0.0 && *f >= 1.0)
        .map(|f| f as u64);

    match as_int {
        Some(n) => Ok(Some(n)),
        None => Err(CandleError::ConfigFileError(format!(
            "Config file error: 'logEviction.{field}' must be a positive integer"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_map_services_are_coerced_to_array() {
        let value = json!({
            "services": {
                "api": { "shell": "npm run dev" },
                "worker": { "shell": "node worker.js", "root": "pkg/worker" }
            }
        });
        let cfg = validate_config(value).unwrap();
        assert_eq!(cfg.services.len(), 2);
        assert_eq!(cfg.services[0].name, "api");
        assert_eq!(cfg.services[0].shell, "npm run dev");
        assert_eq!(cfg.services[1].name, "worker");
        assert_eq!(cfg.services[1].root.as_deref(), Some("pkg/worker"));
    }

    #[test]
    fn missing_services_normalizes_to_empty() {
        let cfg = validate_config(json!({})).unwrap();
        assert!(cfg.services.is_empty());
    }

    #[test]
    fn missing_name_error() {
        let err = validate_config(json!({ "services": [ { "shell": "x" } ] })).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: Each service must have a \"name\" string"
        );
    }

    #[test]
    fn missing_shell_error() {
        let err = validate_config(json!({ "services": [ { "name": "api" } ] })).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: Service \"api\" must have a \"shell\" string"
        );
    }

    #[test]
    fn duplicate_name_error() {
        let err = validate_config(json!({ "services": [
            { "name": "api", "shell": "a" },
            { "name": "api", "shell": "b" }
        ] }))
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: Duplicate service name: \"api\""
        );
    }

    #[test]
    fn invalid_root_error() {
        let err = validate_config(json!({ "services": [
            { "name": "api", "shell": "a", "root": "../escape" }
        ] }))
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Service \"api\" has invalid root path: \"../escape\""
        );
    }

    #[test]
    fn services_string_is_invalid() {
        let err = validate_config(json!({ "services": "nope" })).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: Invalid value for 'services': \"nope\""
        );
    }

    #[test]
    fn log_eviction_must_be_object() {
        let err = validate_config(json!({ "services": [], "logEviction": [] })).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: Invalid value for 'logEviction': expected an object"
        );
    }

    #[test]
    fn log_eviction_positive_integer() {
        let err = validate_config(json!({
            "services": [],
            "logEviction": { "maxLogsPerService": 0 }
        }))
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: 'logEviction.maxLogsPerService' must be a positive integer"
        );

        let err = validate_config(json!({
            "services": [],
            "logEviction": { "maxRetentionSeconds": 3.5 }
        }))
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Config file error: 'logEviction.maxRetentionSeconds' must be a positive integer"
        );
    }

    #[test]
    fn obsolete_log_collector_key_is_preserved_not_rejected() {
        // `logCollector` chose between the old Node and Rust collector sidecars.
        // Neither exists now (supervision runs as `candle --monitor`), so the key
        // is no longer known — but an existing config that sets it must still load,
        // with the value round-tripped like any other unknown key.
        let cfg = validate_config(json!({ "services": [], "logCollector": "rust" })).unwrap();
        assert_eq!(
            cfg.to_value(),
            json!({ "services": [], "logCollector": "rust" })
        );
    }

    #[test]
    fn unknown_top_level_keys_preserved_round_trip() {
        let input = "{\n  \"services\": [\n    {\n      \"name\": \"api\",\n      \"shell\": \"npm run dev\"\n    }\n  ],\n  \"customKey\": \"customValue\"\n}";
        let value: Value = serde_json::from_str(input).unwrap();
        let cfg = validate_config(value).unwrap();
        assert_eq!(cfg.to_json_string(), input);
    }

    #[test]
    fn unknown_key_before_services_preserves_order() {
        let input =
            "{\n  \"customKey\": 1,\n  \"services\": [\n    {\n      \"name\": \"api\",\n      \"shell\": \"x\"\n    }\n  ]\n}";
        let value: Value = serde_json::from_str(input).unwrap();
        let cfg = validate_config(value).unwrap();
        assert_eq!(cfg.to_json_string(), input);
    }
}
