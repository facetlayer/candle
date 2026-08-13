//! Process-tree discovery.
//!
//! Ported from `src/process-tree.ts`. Collects a root PID plus all of its
//! transitive descendants by repeatedly querying for child PIDs with a
//! platform-specific command:
//! - macOS: `pgrep -P <pid>`
//! - Linux: `ps -o pid --no-headers --ppid <pid>`
//! - other platforms: no descendants (returns just the root).

use std::process::{Command, Stdio};

/// Get every PID in the process tree rooted at `root_pid`.
///
/// The root appears first, followed by descendants in discovery order. Callers
/// that need children-first ordering (e.g. signalling) should reverse the result.
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
            &["-o", "pid", "--no-headers", "--ppid", &parent_pid.to_string()],
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = parent_pid;
        Vec::new()
    }
}

/// Run `command args`, parsing stdout as a newline-separated list of PIDs.
///
/// stdin and stderr are silenced; stdout is captured. Non-numeric and blank
/// lines are dropped. On any spawn failure the result is an empty list (matching
/// the Node `error` event handler).
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux")),
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
        .trim()
        .split('\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.trim().parse::<i64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_includes_root() {
        // The current process always exists; its tree contains at least itself.
        let me = std::process::id() as i64;
        let tree = get_process_tree(me);
        assert!(tree.contains(&me));
        assert_eq!(tree[0], me, "root should be first");
    }

    #[test]
    fn child_pids_of_unallocated_pid_is_empty() {
        // A PID that is essentially never allocated has no children.
        assert!(get_child_pids(2_000_000_000).is_empty());
    }
}
