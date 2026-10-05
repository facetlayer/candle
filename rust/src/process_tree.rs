//! Discover process descendants with platform tools; unavailable tools yield
//! only the root PID.

use std::process::{Command, Stdio};

/// Return root then descendants in discovery order; reverse for children-first signalling.
pub fn get_process_tree(root_pid: i64) -> Vec<i64> {
    let mut all_pids = vec![root_pid];
    let mut to_visit = vec![root_pid];

    while let Some(pid) = to_visit.pop() {
        for child in get_child_pids(pid) {
            all_pids.push(child);
            to_visit.push(child);
        }
    }

    all_pids
}

/// Group members via pgrep -g, including children surviving an exited shell.
/// Return empty if the tool is missing or the group is empty.
pub fn get_process_group_members(pgid: i64) -> Vec<i64> {
    if pgid <= 1 {
        return Vec::new();
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        run_command_for_pids("pgrep", &["-g", &pgid.to_string()])
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// Get the direct child PIDs of `parent_pid` using the platform's process tool.
pub fn get_child_pids(parent_pid: i64) -> Vec<i64> {
    #[cfg(target_os = "macos")]
    {
        run_command_for_pids("pgrep", &["-P", &parent_pid.to_string()])
    }
    #[cfg(target_os = "linux")]
    {
        run_command_for_pids(
            "ps",
            &[
                "-o",
                "pid",
                "--no-headers",
                "--ppid",
                &parent_pid.to_string(),
            ],
        )
    }
    #[cfg(target_os = "windows")]
    {
        // CIM replaces deprecated wmic; NoProfile avoids startup configuration.
        let script = format!(
            "Get-CimInstance Win32_Process -Filter \"ParentProcessId={parent_pid}\" | \
             Select-Object -ExpandProperty ProcessId"
        );
        run_command_for_pids(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = parent_pid;
        Vec::new()
    }
}

/// Parse tool stdout as PIDs, ignoring invalid lines; spawn failure yields none.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
    allow(dead_code)
)]
fn run_command_for_pids(command: &str, args: &[&str]) -> Vec<i64> {
    let output = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let output = match output {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| line.trim().parse::<i64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_includes_root() {
        let me = std::process::id() as i64;
        let tree = get_process_tree(me);
        assert!(tree.contains(&me));
        assert_eq!(tree[0], me, "root should be first");
    }

    #[test]
    fn child_pids_of_unallocated_pid_is_empty() {
        assert!(get_child_pids(2_000_000_000).is_empty());
    }
}
