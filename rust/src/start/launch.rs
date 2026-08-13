//! Spawning the detached monitor process.
//!
//! Each service Candle starts is supervised by a second `candle` process running
//! in monitor mode (`candle --monitor`) — the same executable, re-invoked. There is
//! no separate sidecar binary to install or locate.
//!
//! The handshake: spawn the monitor in its own session so it outlives the CLI,
//! write the [`MonitorLaunchInfo`] as a single line of JSON with NO trailing
//! newline, then close stdin so the monitor's read-to-EOF completes. The child is
//! never waited on.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::monitor::MonitorLaunchInfo;

/// Resolve the executable to re-invoke for monitor mode: this very binary.
///
/// `CANDLE_MONITOR_PATH` overrides it (used by tests that need to point at a
/// specific build).
pub fn resolve_monitor_path() -> PathBuf {
    if let Ok(path) = std::env::var("CANDLE_MONITOR_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }

    std::env::current_exe().expect("failed to resolve current executable path")
}

/// Launch the monitor process, detached, and hand it the launch info over stdin.
///
/// The monitor is placed in a new session (`setsid`) so that it is not part of the
/// CLI's process group / controlling terminal and survives the CLI exiting.
/// Returns once the JSON handshake has been written and stdin closed; the child is
/// intentionally not waited on.
pub fn launch_monitor(info: &MonitorLaunchInfo) -> std::io::Result<()> {
    let exe = resolve_monitor_path();
    let json = serde_json::to_string(info).expect("launch info is always serializable");

    let mut command = Command::new(&exe);
    command
        .arg("--monitor")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Detach into a new session so the monitor outlives this CLI process.
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
    // stdin by dropping it, so the monitor's read-to-EOF completes.
    {
        let mut stdin = child
            .stdin
            .take()
            .expect("stdin was configured as piped");
        stdin.write_all(json.as_bytes())?;
    }

    // Do NOT wait on the child: it must outlive us. `std::process::Child`'s drop
    // neither waits nor kills, so the monitor keeps running and is reparented to
    // init once we exit.
    Ok(())
}
