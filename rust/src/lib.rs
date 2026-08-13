//! candle: the whole implementation, as one crate.
//!
//! The `candle` binary (`src/main.rs`) is the only executable Candle ships. It has two
//! modes: the normal CLI, and the monitor mode (`candle --monitor`) that supervises one
//! service subprocess — see [`monitor`]. This library target exists so the integration
//! tests under `tests/` can drive those internals directly.
//!
//! See `rust/docs/architecture/` for the per-subsystem reference docs.

pub mod cli;
pub mod commands;
pub mod config;
pub mod db;
pub mod debug;
pub mod dirs;
pub mod doc_files;
pub mod errors;
pub mod kill;
pub mod log_filters;
pub mod logs;
pub mod mcp;
pub mod monitor;
pub mod output;
pub mod process_alive;
pub mod process_tree;
pub mod run_context;
pub mod start;
