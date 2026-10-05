//! Spawn a detached `candle --monitor` and send launch JSON over stdin.
//! Closing stdin completes the handshake; the CLI does not wait for the monitor.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::monitor::MonitorLaunchInfo;

/// Use the current binary, unless tests override `CANDLE_MONITOR_PATH`.
pub fn resolve_monitor_path() -> PathBuf {
    if let Ok(path) = std::env::var("CANDLE_MONITOR_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }

    std::env::current_exe().expect("failed to resolve current executable path")
}

/// Launch the monitor in a new session so it survives CLI exit.
/// Return after writing the JSON handshake and closing stdin.
pub fn launch_monitor(info: &MonitorLaunchInfo) -> std::io::Result<()> {
    let exe = resolve_monitor_path();
    let json = serde_json::to_string(info).expect("launch info is always serializable");

    let mut command = Command::new(&exe);
    command
        .arg("--monitor")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // SAFETY: `setsid` only mutates the calling (child) process state between
    // fork and exec; it does not touch the parent's address space.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let mut child = command.spawn()?;

    // Closing stdin completes the monitor's read-to-EOF handshake.
    {
        let mut stdin = child.stdin.take().expect("stdin was configured as piped");
        stdin.write_all(json.as_bytes())?;
    }

    // Dropping Child neither waits nor kills; leave the monitor running.
    Ok(())
}
