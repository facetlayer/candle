//! Config models preserving unknown keys and original key order on write-back.
//! Separate key ordering avoids serde(flatten) grouping known and unknown keys.

use serde_json::{Map, Value};

pub const CONFIG_FILENAME: &str = ".candle.json";

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
    /// Original order of known and unknown top-level keys.
    pub(crate) key_order: Vec<String>,
    /// Unknown top-level keys, preserved verbatim on round-trip.
    pub(crate) extra: Map<String, Value>,
    /// Original service objects, preserving unknown keys and their order.
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
    /// Serialize new services in name/shell/root/enableStdin order, omitting falsy options.
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

    /// Update known fields in place, preserving unknown keys and their order.
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

    /// Append a missing key to the write-back order.
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
