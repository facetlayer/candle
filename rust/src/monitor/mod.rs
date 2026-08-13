//! Monitor mode: supervises one service subprocess and records its lifecycle +
//! output to the database.
//!
//! This runs inside the same `candle` executable as the CLI, reached via the
//! `candle --monitor` flag (see [`crate::cli::monitor_mode`]). The CLI launches it
//! detached for each service it starts; see [`crate::start::launch`].
//!
//! The supervision loop itself lives in [`run`].

mod run;

pub mod launch_info;

pub use launch_info::MonitorLaunchInfo;
pub use run::run;
