//! candle-core: shared library for the candle CLI and log-collector sidecar.
//!
//! This implementation mirrors the original Node/TypeScript code in `../../src`; see
//! `rust/docs/architecture/` for the per-subsystem reference docs.

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
