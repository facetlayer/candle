//! The handshake contract between the CLI launcher and monitor mode.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Service launch handshake, serialized by the launcher and read by the monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorLaunchInfo {
    pub command_name: String,
    pub project_dir: String,
    pub shell: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub enable_stdin: bool,
    pub database_path: PathBuf,
    /// Launch marker id stamped on every log row; legacy launches omit it.
    #[serde(default)]
    pub run_id: Option<i64>,
    /// Transient --shell launch, excluded from config-based orphan checks.
    #[serde(default)]
    pub transient: bool,
}
