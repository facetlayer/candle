//! Spawning the detached log-collector sidecar.
//!
//! Ports `launchWithLogCollector` from `src/log-collector/launchWithLogCollector.ts`,
//! reduced to the Rust-sidecar path only (the Node implementation has a
//! node-vs-rust switch; the pure-Rust port always launches the Rust binary).
//!
//! The handshake mirrors the Node launcher exactly: spawn the sidecar in its own
//! session so it outlives the CLI, write the [`LogCollectorLaunchInfo`] as a
//! single line of JSON with NO trailing newline, then close stdin so the
//! sidecar's read-to-EOF completes. The child is never waited on.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::log_collector::LogCollectorLaunchInfo;

/// Resolve the path to the `log-collector` binary.
///
/// Uses `CANDLE_LOG_COLLECTOR_PATH` when set (and non-empty); otherwise the
/// sibling of the current executable, since the CLI and the sidecar are built
/// into the same target directory.
pub fn resolve_log_collector_path() -> PathBuf {
    if let Ok(path) = std::env::var("CANDLE_LOG_COLLECTOR_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }

    let exe = std::env::current_exe().expect("failed to resolve current executable path");
    let dir = exe
        .parent()
        .expect("current executable has no parent directory");
    dir.join("log-collector")
}

/// Launch the log-collector sidecar, detached, and hand it the launch info over
/// stdin.
///
/// The sidecar is placed in a new session (`setsid`) so that it is not part of
/// the CLI's process group / controlling terminal and survives the CLI exiting.
/// Returns once the JSON handshake has been written and stdin closed; the child
/// is intentionally not waited on.
pub fn launch_with_log_collector(info: &LogCollectorLaunchInfo) -> std::io::Result<()> {
    let collector_path = resolve_log_collector_path();
    let json = serde_json::to_string(info).expect("launch info is always serializable");

    let mut command = Command::new(&collector_path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Detach into a new session so the sidecar outlives this CLI process.
    // SAFETY: `setsid` only mutates the calling (child) process state between
    // fork and exec; it does not touch the parent's address space.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let mut child = command.spawn()?;

    // Write the handshake JSON (single line, no trailing newline) and close
    // stdin by dropping it, so the sidecar's read-to-EOF completes.
    {
        let mut stdin = child
            .stdin
            .take()
            .expect("stdin was configured as piped");
        stdin.write_all(json.as_bytes())?;
    }

    // Do NOT wait on the child: it must outlive us. `std::process::Child`'s drop
    // neither waits nor kills, so the sidecar keeps running and is reparented to
    // init once we exit.
    Ok(())
}
