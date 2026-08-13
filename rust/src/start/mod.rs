//! The `start` / `run` / `check-start` command path.
//!
//! See `rust/docs/architecture/start-flow.md`.

pub mod launch;
pub mod start_command;
pub mod start_one_service;

pub use launch::{launch_monitor, resolve_monitor_path};
pub use start_command::{handle_start_command, StartCommandOptions};
pub use start_one_service::{start_one_service, RunOptions, StartResult};
