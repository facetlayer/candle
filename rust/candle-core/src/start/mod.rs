//! The `start` / `run` / `check-start` command path.
//!
//! Ports `src/start-command.ts`, `src/start/startOneService.ts`, and the
//! Rust-sidecar half of `src/log-collector/launchWithLogCollector.ts`. See
//! `rust/docs/porting/map-start-flow.md`.

pub mod launch;
pub mod start_command;
pub mod start_one_service;

pub use launch::{launch_with_log_collector, resolve_log_collector_path};
pub use start_command::{handle_start_command, StartCommandOptions};
pub use start_one_service::{start_one_service, RunOptions, StartResult};
