//! The handshake contract between the CLI launcher and monitor mode.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Describes the service that a monitor-mode process should supervise.
///
/// Field names are camelCase to match the JSON the launcher writes to the
/// monitor's stdin. `Serialize` is used by the launcher ([`crate::start::launch`]);
/// `Deserialize` is used by monitor mode to read it back.
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
}
