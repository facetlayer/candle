//! candle-core: shared library for the candle CLI and log-collector sidecar.
//!
//! Modules are ported from the Node/TypeScript implementation in `../../src`. See
//! `rust/PORTING_PLAN.md` for the architecture and `rust/docs/porting/` for per-subsystem specs.
//!
//! Modules are added incrementally as the port progresses (see milestones M1+).

pub mod commands;
pub mod config;
pub mod db;
pub mod debug;
pub mod dirs;
pub mod doc_files;
pub mod errors;
pub mod kill;
pub mod log_collector;
pub mod log_filters;
pub mod logs;
pub mod mcp;
pub mod output;
pub mod process_alive;
pub mod process_tree;
pub mod run_context;
pub mod start;
