//! Log-collector sidecar: supervises one service subprocess and records its
//! lifecycle + output to the database.
//!
//! Ports `src/main-log-collector.ts` and `src/log-collector/*.ts`. The reusable
//! supervision loop lives in [`monitor::run`]; the thin binary in
//! `rust/log-collector/src/main.rs` only parses [`LogCollectorLaunchInfo`] and
//! delegates here.

pub mod monitor;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Handshake contract describing the service to supervise.
///
/// Mirrors `LogCollectorLaunchInfo` in `src/log-collector/LogCollectorLaunchInfo.ts`.
/// Field names are camelCase to match the JSON written by the launcher
/// (`commandName`, `projectDir`, `shell`, `root`, `enableStdin`, `databasePath`).
/// `Serialize` is used by the CLI launcher ([`crate::start::launch`]) to write the
/// handshake; `Deserialize` is used by the sidecar binary to read it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogCollectorLaunchInfo {
    pub command_name: String,
    pub project_dir: String,
    pub shell: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub enable_stdin: bool,
    pub database_path: PathBuf,
}
