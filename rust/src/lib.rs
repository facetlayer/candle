//! Candle implementation shared by the binary and integration tests.
//! See `rust/docs/architecture/` for subsystem references.

pub mod cli;
pub mod commands;
pub mod config;
pub mod db;
pub mod debug;
pub mod dirs;
pub mod doc_files;
pub mod errors;
pub mod kill;
pub mod listening_ports;
pub mod log_filters;
pub mod logs;
pub mod mcp;
pub mod monitor;
pub mod output;
pub mod process_alive;
pub mod process_identity;
pub mod process_tree;
pub mod project_scope;
pub mod run_context;
pub mod start;
