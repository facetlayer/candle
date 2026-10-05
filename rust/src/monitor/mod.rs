//! Detached `candle --monitor` supervision; see [`run`] for the lifecycle.

mod run;

pub mod launch_info;

pub use launch_info::MonitorLaunchInfo;
pub use run::run;
