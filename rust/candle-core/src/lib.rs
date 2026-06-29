//! candle-core: shared library for the candle CLI and log-collector sidecar.
//!
//! Modules are ported from the Node/TypeScript implementation in `../../src`. See
//! `rust/PORTING_PLAN.md` for the architecture and `rust/docs/porting/` for per-subsystem specs.
//!
//! Modules are added incrementally as the port progresses (see milestones M1+).

pub mod db;
pub mod debug;
pub mod dirs;
pub mod errors;
pub mod logs;
pub mod run_context;
