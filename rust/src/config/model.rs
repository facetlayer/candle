//! Config data model and order-preserving serialization.
//!
//! Design note: rather than relying on `#[serde(flatten)]` (whose serialization
//! order is fixed by struct-field order and therefore cannot reproduce the
//! file's original key order for interspersed unknown keys), this module keeps the parsed
//! config as typed fields *plus* a `key_order` list and an `extra` map. The
//! canonical write-back path (`to_value` / `to_json_string`) reconstructs the
//! object in the original insertion order, as 2-space-indented JSON (falsy
//! `root` / `enableStdin` omitted, unknown top-level and per-service keys
//! preserved) with a trailing newline.

use serde_json::{Map, Value};

/// Config filenames in priority order (first match wins). `.candle-setup.json`
/// is deprecated but still supported.
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
    /// Top-level key insertion order, used to preserve the file's key ordering on
    /// write-back. Known keys (`services` / `logEviction`) and unknown keys (held
    /// in `extra`) both appear here.
    pub(crate) key_order: Vec<String>,
    /// Unknown top-level keys, preserved verbatim on round-trip.
    pub(crate) extra: Map<String, Value>,
    /// Each service's object as read from disk, keyed by service name. Write-back
    /// starts from it, so per-service keys Candle doesn't know (and their order)
    /// survive `add-service` / `remove-service` / `set-config`.
    pub(crate) service_raw: Map<String, Value>,
}

impl Default for CandleSetupConfig {
    /// An empty config: `{ "services": [] }`.
    fn default() -> Self {
        CandleSetupConfig {
            services: Vec::new(),
            log_eviction: None,
            key_order: vec!["services".to_string()],
            extra: Map::new(),
            service_raw: Map::new(),
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
    /// `add-service`: `name`, `shell`, then `root` / `enableStdin` only when
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

    /// Serialize on top of the object this service was read from: known keys are
    /// updated in place, unknown keys keep their value and position.
    fn to_value_over(&self, raw: Option<&Value>) -> Value {
        let Some(Value::Object(raw)) = raw else {
            return self.to_value();
        };
        let mut m = raw.clone();
        m.insert("name".to_string(), Value::String(self.name.clone()));
        m.insert("shell".to_string(), Value::String(self.shell.clone()));
        match &self.root {
            Some(root) if !root.is_empty() => {
                m.insert("root".to_string(), Value::String(root.clone()));
            }
            _ => {
                m.shift_remove("root");
            }
        }
        if self.enable_stdin == Some(true) {
            m.insert("enableStdin".to_string(), Value::Bool(true));
        } else {
            m.shift_remove("enableStdin");
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
                    let arr: Vec<Value> = self
                        .services
                        .iter()
                        .map(|s| s.to_value_over(self.service_raw.get(&s.name)))
                        .collect();
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

    /// Serialize to a 2-space pretty JSON string ending in a newline.
    pub fn to_json_string(&self) -> String {
        let mut s = serde_json::to_string_pretty(&self.to_value())
            .expect("config Value is always serializable");
        s.push('\n');
        s
    }

    /// Ensure a top-level key is present in `key_order` (appending it at the end
    /// if missing), so newly set keys are written after existing ones.
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
        assert_eq!(cfg.to_json_string(), "{\n  \"services\": []\n}\n");
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
        assert_eq!(
            s,
            "{\"name\":\"api\",\"shell\":\"cmd\",\"root\":\"packages/api\",\"enableStdin\":true}"
        );
    }
}
