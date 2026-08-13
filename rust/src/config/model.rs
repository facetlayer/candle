//! Config data model and order-preserving serialization.
//!
//! Ported from the `CandleSetupConfig` / `ServiceConfig` / `LogEvictionConfig`
//! interfaces in `src/configFile.ts`.
//!
//! Design note: rather than relying on `#[serde(flatten)]` (whose serialization
//! order is fixed by struct-field order and therefore cannot reproduce JS object
//! insertion order for interspersed unknown keys), this module keeps the parsed
//! config as typed fields *plus* a `key_order` list and an `extra` map. The
//! canonical write-back path (`to_value` / `to_json_string`) reconstructs the
//! object in the original insertion order, matching `JSON.stringify(config, null, 2)`
//! byte-for-byte for the common cases (2-space indent, no trailing newline,
//! falsy `root` / `enableStdin` omitted, unknown top-level keys preserved).

use serde_json::{Map, Value};

/// Config filenames in priority order (first match wins). `.candle-setup.json`
/// is deprecated but still supported. Mirrors `CONFIG_FILENAMES` in configFile.ts.
pub const CONFIG_FILENAMES: [&str; 2] = [".candle.json", ".candle-setup.json"];

/// Default filename used when creating a new config file.
pub const DEFAULT_CONFIG_FILENAME: &str = ".candle.json";

/// Defaults applied at read time by `get_log_eviction_config` (not written to disk).
pub const LOG_EVICTION_DEFAULTS: ResolvedLogEvictionConfig = ResolvedLogEvictionConfig {
    max_logs_per_service: 1000,
    max_retention_seconds: 24 * 60 * 60,
};

/// A single configured service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub name: String,
    pub shell: String,
    /// Working directory relative to the config file dir, or absolute.
    pub root: Option<String>,
    /// Enables stdin message polling from the DB.
    pub enable_stdin: Option<bool>,
}

/// The `logEviction` nested object.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogEvictionConfig {
    pub max_logs_per_service: Option<u64>,
    pub max_retention_seconds: Option<u64>,
}

/// Fully-resolved log-eviction settings (defaults applied).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedLogEvictionConfig {
    pub max_logs_per_service: u64,
    pub max_retention_seconds: u64,
}

/// Top-level `.candle.json` contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandleSetupConfig {
    pub services: Vec<ServiceConfig>,
    pub log_eviction: Option<LogEvictionConfig>,
    /// Top-level key insertion order, used to reproduce JS object ordering on
    /// write-back. Known keys (`services` / `logEviction`) and unknown keys (held
    /// in `extra`) both appear here.
    pub(crate) key_order: Vec<String>,
    /// Unknown top-level keys, preserved verbatim on round-trip.
    pub(crate) extra: Map<String, Value>,
}

impl Default for CandleSetupConfig {
    /// An empty config: `{ "services": [] }`.
    fn default() -> Self {
        CandleSetupConfig {
            services: Vec::new(),
            log_eviction: None,
            key_order: vec!["services".to_string()],
            extra: Map::new(),
        }
    }
}

impl LogEvictionConfig {
    fn to_value(&self) -> Value {
        let mut m = Map::new();
        if let Some(v) = self.max_logs_per_service {
            m.insert("maxLogsPerService".to_string(), Value::from(v));
        }
        if let Some(v) = self.max_retention_seconds {
            m.insert("maxRetentionSeconds".to_string(), Value::from(v));
        }
        Value::Object(m)
    }
}

impl ServiceConfig {
    /// Serialize with fields in the canonical insertion order used by
    /// `addServerConfig`: `name`, `shell`, then `root` / `enableStdin` only when
    /// truthy.
    fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("name".to_string(), Value::String(self.name.clone()));
        m.insert("shell".to_string(), Value::String(self.shell.clone()));
        if let Some(root) = &self.root {
            if !root.is_empty() {
                m.insert("root".to_string(), Value::String(root.clone()));
            }
        }
        if self.enable_stdin == Some(true) {
            m.insert("enableStdin".to_string(), Value::Bool(true));
        }
        Value::Object(m)
    }
}

impl CandleSetupConfig {
    /// Reconstruct the JSON object in original insertion order.
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        for key in &self.key_order {
            match key.as_str() {
                "services" => {
                    let arr: Vec<Value> = self.services.iter().map(ServiceConfig::to_value).collect();
                    map.insert("services".to_string(), Value::Array(arr));
                }
                "logEviction" => {
                    if let Some(le) = &self.log_eviction {
                        map.insert("logEviction".to_string(), le.to_value());
                    }
                }
                other => {
                    if let Some(v) = self.extra.get(other) {
                        map.insert(other.to_string(), v.clone());
                    }
                }
            }
        }
        Value::Object(map)
    }

    /// Serialize to a 2-space pretty JSON string with NO trailing newline,
    /// matching `JSON.stringify(config, null, 2)`.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(&self.to_value())
            .expect("config Value is always serializable")
    }

    /// Ensure a top-level key is present in `key_order` (appending it at the end
    /// if missing), matching JS where assigning a new key appends it.
    pub(crate) fn ensure_key(&mut self, key: &str) {
        if !self.key_order.iter().any(|k| k == key) {
            self.key_order.push(key.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_serializes_to_empty_services() {
        let cfg = CandleSetupConfig::default();
        assert_eq!(cfg.to_json_string(), "{\n  \"services\": []\n}");
    }

    #[test]
    fn service_omits_falsy_root_and_enable_stdin() {
        let svc = ServiceConfig {
            name: "api".to_string(),
            shell: "npm run dev".to_string(),
            root: Some(String::new()),
            enable_stdin: Some(false),
        };
        let v = svc.to_value();
        let obj = v.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("shell"));
        assert!(!obj.contains_key("root"));
        assert!(!obj.contains_key("enableStdin"));
    }

    #[test]
    fn service_field_insertion_order() {
        let svc = ServiceConfig {
            name: "api".to_string(),
            shell: "cmd".to_string(),
            root: Some("packages/api".to_string()),
            enable_stdin: Some(true),
        };
        let s = serde_json::to_string(&svc.to_value()).unwrap();
        assert_eq!(s, "{\"name\":\"api\",\"shell\":\"cmd\",\"root\":\"packages/api\",\"enableStdin\":true}");
    }
}
