//! The `start` / `run` / `restart` launch path.
//!
//! See `rust/docs/architecture/start-flow.md`.

pub mod launch;
pub mod service_lock;
pub mod start_command;
pub mod start_one_service;

pub use launch::{launch_monitor, resolve_monitor_path};
pub use start_command::{handle_start_command, start_each, StartCommandOptions};
pub use start_one_service::{start_one_service, IfRunning, RunOptions, StartResult};
