//! Signal service trees/groups and update process records.
//! Successful kills mark killed_at; stale cleanup removes rows.
//! See `rust/docs/architecture/kill-restart.md`.

use std::collections::HashSet;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::db::process_table::{
    clear_process_killed_at, delete_process_entry, find_all_processes,
    find_processes_by_command_name_and_project_dir, find_running_processes_by_project_dir,
    update_process_killed_at, ProcessEntry,
};
use crate::output;
use crate::process_alive::{is_monitor_alive, is_process_alive, is_service_process_alive};
use crate::process_tree::get_process_tree;

/// SIGTERM grace period before SIGKILL.
pub const KILL_GRACE_PERIOD: Duration = Duration::from_secs(5);
/// How long to wait for a `SIGKILL`ed root to disappear before giving up.
const SIGKILL_WAIT: Duration = Duration::from_secs(1);
/// Poll interval while waiting for a signalled process to exit.
const KILL_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Age after which a repeat kill deletes a marked row.
const STALE_ENTRY_SECONDS: i64 = 5 * 60;

/// Outcome of signalling a process tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillResult {
    Success,
    ProcessNotFound,
    Error,
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Signal a snapshot of the tree, children first, without waiting. Ignore ESRCH;
/// report other signal errors. Newly forked descendants are outside the snapshot.
///
/// # Panics
/// Panics if `pid <= 0`.
pub fn kill_process_tree(pid: i64) -> KillResult {
    if pid <= 0 {
        panic!("internal error: kill_process_tree called with invalid PID: {pid}");
    }

    let pids = get_process_tree(pid);
    if pids.is_empty() {
        return KillResult::ProcessNotFound;
    }

    let mut has_error = false;
    let mut all_not_found = true;

    for child_pid in pids.into_iter().rev() {
        let result = unsafe { libc::kill(child_pid as libc::pid_t, libc::SIGTERM) };
        if result == 0 {
            all_not_found = false;
            continue;
        }

        let os_err = std::io::Error::last_os_error();
        if os_err.raw_os_error() == Some(libc::ESRCH) {
            continue;
        }

        output::err(&format!(
            "Warning: Could not kill process {child_pid}: {os_err}"
        ));
        has_error = true;
    }

    if all_not_found {
        KillResult::ProcessNotFound
    } else if has_error {
        KillResult::Error
    } else {
        KillResult::Success
    }
}

/// Return the group if pid is its live leader. Checking leadership avoids
/// signalling the monitor group shared by legacy services.
fn led_process_group(pid: i64) -> Option<i64> {
    let pgid = unsafe { libc::getpgid(pid as libc::pid_t) };
    (pgid as i64 == pid).then_some(pid)
}

/// Whether any process is left in process group `pgid`.
pub(crate) fn process_group_alive(pgid: i64) -> bool {
    // `kill(-1, ..)` and `kill(0, ..)` address every process and the caller's
    // own group, never a service's.
    if pgid <= 1 {
        return false;
    }
    let result = unsafe { libc::kill(-(pgid as libc::pid_t), 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Signal every process in group `pgid`, ignoring errors (the group may
/// already be empty).
fn signal_process_group(pgid: i64, signal: libc::c_int) {
    if pgid <= 1 {
        return;
    }
    unsafe {
        libc::kill(-(pgid as libc::pid_t), signal);
    }
}

/// Wait for all snapshot PIDs and optional group members to exit, or timeout.
fn wait_for_all_to_exit(pids: &[i64], group: Option<i64>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !pids.iter().copied().any(is_process_alive) && !group.is_some_and(process_group_alive) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(KILL_POLL_INTERVAL);
    }
}

/// SIGTERM the tree and its own process group, then SIGKILL survivors after grace.
/// Keep the original PID snapshot to reach children reparented when the shell
/// exits; the group also covers descendants reparented before the snapshot.
/// Return Escalated if SIGKILL was needed.
pub fn kill_process_tree_and_wait(pid: i64, grace: Duration) -> KillOutcome {
    let snapshot = get_process_tree(pid);
    // Before signalling: once the leader exits, getpgid on it fails.
    let group = led_process_group(pid);

    match kill_process_tree(pid) {
        KillResult::Success => {}
        KillResult::ProcessNotFound => return KillOutcome::ProcessNotFound,
        KillResult::Error => return KillOutcome::Error,
    }
    if let Some(pgid) = group {
        signal_process_group(pgid, libc::SIGTERM);
    }

    if wait_for_all_to_exit(&snapshot, group, grace) {
        return KillOutcome::Terminated;
    }

    // Include newly forked descendants; kill children before respawning parents.
    let mut targets: Vec<i64> = Vec::new();
    for survivor in snapshot.iter().copied().filter(|p| is_process_alive(*p)) {
        for p in get_process_tree(survivor) {
            if !targets.contains(&p) {
                targets.push(p);
            }
        }
    }
    for target in targets.iter().rev() {
        unsafe {
            libc::kill(*target as libc::pid_t, libc::SIGKILL);
        }
    }
    if let Some(pgid) = group {
        signal_process_group(pgid, libc::SIGKILL);
    }

    if wait_for_all_to_exit(&targets, group, SIGKILL_WAIT) {
        KillOutcome::Escalated
    } else {
        KillOutcome::Error
    }
}

/// Terminate a group whose shell has exited, escalating after grace.
/// The caller must verify the group belongs to this service via leader_exited.
pub fn kill_process_group_and_wait(pgid: i64, grace: Duration) -> KillOutcome {
    if !process_group_alive(pgid) {
        return KillOutcome::ProcessNotFound;
    }
    signal_process_group(pgid, libc::SIGTERM);
    if wait_for_all_to_exit(&[], Some(pgid), grace) {
        return KillOutcome::Terminated;
    }
    signal_process_group(pgid, libc::SIGKILL);
    if wait_for_all_to_exit(&[], Some(pgid), SIGKILL_WAIT) {
        KillOutcome::Escalated
    } else {
        KillOutcome::Error
    }
}

/// Outcome of [`kill_process_tree_and_wait`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillOutcome {
    /// The whole tree exited on `SIGTERM` within the grace period.
    Terminated,
    /// Some process in the tree ignored `SIGTERM` and needed `SIGKILL`.
    Escalated,
    ProcessNotFound,
    /// Could not be signalled, or survived even `SIGKILL`.
    Error,
}

/// Mark killed before signalling so the monitor recognizes deliberate stops.
/// Delete missing or stale rows; undo the mark on errors. Zero PID is a no-op.
/// Return whether a live process was handled, excluding stale-row cleanup.
pub fn kill_one_running_process(
    conn: &Connection,
    entry: &ProcessEntry,
    quiet: bool,
) -> rusqlite::Result<bool> {
    if entry.pid == 0 {
        return Ok(false);
    }

    // Mark before signalling so startup exit classification cannot race the kill.
    if entry.killed_at.is_none() {
        update_process_killed_at(
            conn,
            &entry.command_name,
            &entry.project_dir,
            entry.pid,
            now_unix_seconds(),
        )?;
    }

    let outcome = if is_service_process_alive(entry) {
        kill_process_tree_and_wait(entry.pid, KILL_GRACE_PERIOD)
    } else if entry.leader_exited && is_monitor_alive(entry) {
        // The monitor still supervises the exited shell's group.
        kill_process_group_and_wait(entry.pid, KILL_GRACE_PERIOD)
    } else {
        // A reused PID belongs to an unrelated process; never signal it.
        KillOutcome::ProcessNotFound
    };

    let killed = match outcome {
        outcome @ (KillOutcome::Terminated | KillOutcome::Escalated) => {
            if !quiet {
                if outcome == KillOutcome::Escalated {
                    output::err(&format!(
                        "[Process '{}' (PID {}) did not exit {}s after SIGTERM; sent SIGKILL]",
                        entry.command_name,
                        entry.pid,
                        KILL_GRACE_PERIOD.as_secs()
                    ));
                }
                output::out(&format!(
                    "[Killed '{}' process with PID: {}]",
                    entry.command_name, entry.pid
                ));
            }

            let now = now_unix_seconds();
            let is_stale = entry
                .killed_at
                .is_some_and(|killed_at| killed_at < now - STALE_ENTRY_SECONDS);

            if is_stale {
                if !quiet {
                    output::err(&format!(
                        "[Cleaning up stale process entry for '{}' with PID: {}]",
                        entry.command_name, entry.pid
                    ));
                }
                delete_process_entry(conn, &entry.command_name, &entry.project_dir, entry.pid)?;
            } else {
                update_process_killed_at(
                    conn,
                    &entry.command_name,
                    &entry.project_dir,
                    entry.pid,
                    now,
                )?;
            }

            true
        }
        KillOutcome::ProcessNotFound => {
            if !quiet && entry.killed_at.is_none() {
                output::err(&format!(
                    "[Cleaning up stale process entry for '{}' with PID: {}]",
                    entry.command_name, entry.pid
                ));
            }
            delete_process_entry(conn, &entry.command_name, &entry.project_dir, entry.pid)?;
            false
        }
        KillOutcome::Error => {
            if !quiet {
                output::error(&format!(
                    "Could not kill process '{}' with PID: {}",
                    entry.command_name, entry.pid
                ));
            }

            if entry.killed_at.is_none() {
                clear_process_killed_at(conn, &entry.command_name, &entry.project_dir, entry.pid)?;
            }

            // A live process that resisted the kill still counts as handled.
            true
        }
    };

    Ok(killed)
}

/// Kill named services (deduped), or all running services in the project.
/// Named queries include killed rows for cleanup; only live processes count
/// toward suppressing the "No running processes" message.
pub fn handle_kill_command(
    conn: &Connection,
    project_dir: &str,
    command_names: &[String],
    quiet_failure: bool,
    quiet: bool,
) -> rusqlite::Result<()> {
    if !command_names.is_empty() {
        let mut seen: HashSet<&str> = HashSet::new();
        for name in command_names {
            if !seen.insert(name.as_str()) {
                continue;
            }
            kill_by_command_name(conn, project_dir, name, quiet_failure, quiet)?;
        }
        return Ok(());
    }

    let running = find_running_processes_by_project_dir(conn, project_dir)?;
    let mut killed = 0usize;
    for entry in &running {
        if kill_one_running_process(conn, entry, quiet)? {
            killed += 1;
        }
    }

    if killed == 0 && !quiet_failure {
        output::out(&format!(
            "No running processes found in project '{project_dir}'"
        ));
    }

    Ok(())
}

fn kill_by_command_name(
    conn: &Connection,
    project_dir: &str,
    command_name: &str,
    quiet_failure: bool,
    quiet: bool,
) -> rusqlite::Result<()> {
    let processes =
        find_processes_by_command_name_and_project_dir(conn, command_name, project_dir)?;

    let mut killed = 0usize;
    for entry in &processes {
        if kill_one_running_process(conn, entry, quiet)? {
            killed += 1;
        }
    }

    if killed == 0 && !quiet_failure {
        output::out(&format!(
            "No running processes found for service '{command_name}' in project '{project_dir}'"
        ));
    }

    Ok(())
}

/// Kill all process rows system-wide, also sweeping unreaped killed rows.
pub fn handle_kill_all(conn: &Connection, quiet: bool) -> rusqlite::Result<()> {
    let processes = find_all_processes(conn)?;
    let mut killed = 0usize;
    for entry in &processes {
        if kill_one_running_process(conn, entry, quiet)? {
            killed += 1;
        }
    }

    if killed == 0 {
        output::out("No running processes found");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::process_table::{create_process_entry, find_all_processes, CreateProcessEntry};
    use crate::db::{get_database, temp_db_dir};
    use crate::output::capture;

    fn insert(conn: &Connection, name: &str, pid: i64) {
        create_process_entry(
            conn,
            &CreateProcessEntry {
                command_name: name.to_string(),
                project_dir: "/proj".to_string(),
                pid,
                log_collector_pid: None,
                shell: None,
                root: None,
                run_id: None,
                transient: false,
            },
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "invalid PID")]
    fn kill_process_tree_panics_on_zero() {
        kill_process_tree(0);
    }

    #[test]
    fn dead_pid_reports_not_found() {
        assert_eq!(
            kill_process_tree(2_000_000_000),
            KillResult::ProcessNotFound
        );
    }

    /// Reused PIDs must never be signalled.
    #[test]
    fn a_reused_pid_is_not_signalled() {
        let dir = temp_db_dir("kill-reused-pid");
        let conn = get_database(Some(&dir)).unwrap();

        let mut stranger = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = stranger.id() as i64;
        insert(&conn, "svc", pid);
        // As if the row had been written for an earlier process with this PID.
        conn.execute("update processes set pid_identity = pid_identity - 1", [])
            .unwrap();

        let entry = find_all_processes(&conn).unwrap().pop().unwrap();
        assert!(entry.pid_identity.is_some());
        assert!(!crate::process_alive::is_entry_alive(&entry));
        let (killed, _) = capture(|| kill_one_running_process(&conn, &entry, false).unwrap());

        assert!(!killed);
        assert!(
            stranger.try_wait().unwrap().is_none(),
            "stranger was killed"
        );
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        stranger.kill().unwrap();
        stranger.wait().unwrap();
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_recorded_process_is_killed() {
        let dir = temp_db_dir("kill-recorded-pid");
        let conn = get_database(Some(&dir)).unwrap();

        let mut service = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        insert(&conn, "svc", service.id() as i64);
        // Reap the child: zombies still answer signal 0.
        let reaper = std::thread::spawn(move || service.wait().unwrap());

        let entry = find_all_processes(&conn).unwrap().pop().unwrap();
        let (killed, _) = capture(|| kill_one_running_process(&conn, &entry, true).unwrap());

        assert!(killed);
        assert!(!reaper.join().unwrap().success());

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn group_kill_refuses_reserved_ids() {
        assert_eq!(
            kill_process_group_and_wait(0, Duration::from_millis(10)),
            KillOutcome::ProcessNotFound
        );
        assert_eq!(
            kill_process_group_and_wait(1, Duration::from_millis(10)),
            KillOutcome::ProcessNotFound
        );
    }

    #[test]
    fn kill_one_dead_process_deletes_row() {
        let dir = temp_db_dir("kill-one-dead");
        let conn = get_database(Some(&dir)).unwrap();
        insert(&conn, "svc", 2_000_000_000);

        let entry = find_all_processes(&conn).unwrap().pop().unwrap();
        let (_, captured) = capture(|| kill_one_running_process(&conn, &entry, false).unwrap());

        assert!(captured
            .stderr
            .iter()
            .any(|l| l.contains("Cleaning up stale process entry")));
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweeping_an_already_killed_row_is_silent() {
        let dir = temp_db_dir("kill-one-already-killed");
        let conn = get_database(Some(&dir)).unwrap();
        insert(&conn, "svc", 2_000_000_000);
        update_process_killed_at(&conn, "svc", "/proj", 2_000_000_000, now_unix_seconds()).unwrap();

        let entry = find_all_processes(&conn).unwrap().pop().unwrap();
        let (_, captured) = capture(|| kill_one_running_process(&conn, &entry, false).unwrap());

        assert!(captured.stderr.is_empty(), "{:?}", captured.stderr);
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_command_no_names_reports_empty_project() {
        let dir = temp_db_dir("kill-empty");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) =
            capture(|| handle_kill_command(&conn, "/proj", &[], false, false).unwrap());
        assert_eq!(
            captured.stdout,
            vec!["No running processes found in project '/proj'".to_string()]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_command_unknown_name_reports_per_service() {
        let dir = temp_db_dir("kill-unknown-name");
        let conn = get_database(Some(&dir)).unwrap();

        let names = vec!["ghost".to_string()];
        let (_, captured) =
            capture(|| handle_kill_command(&conn, "/proj", &names, false, false).unwrap());
        assert_eq!(
            captured.stdout,
            vec!["No running processes found for service 'ghost' in project '/proj'".to_string()]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_all_empty_reports_none() {
        let dir = temp_db_dir("kill-all-empty");
        let conn = get_database(Some(&dir)).unwrap();

        let (_, captured) = capture(|| handle_kill_all(&conn, false).unwrap());
        assert_eq!(
            captured.stdout,
            vec!["No running processes found".to_string()]
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_rows_do_not_count_as_kills() {
        // Sweeping a dead row must still report that nothing was running.
        let dir = temp_db_dir("kill-stale-row");
        let conn = get_database(Some(&dir)).unwrap();
        insert(&conn, "svc", 2_000_000_000);

        let names = vec!["svc".to_string()];
        let (_, captured) =
            capture(|| handle_kill_command(&conn, "/proj", &names, false, true).unwrap());

        assert_eq!(
            captured.stdout,
            vec!["No running processes found for service 'svc' in project '/proj'".to_string()]
        );
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kill_command_dedupes_names() {
        let dir = temp_db_dir("kill-dedupe");
        let conn = get_database(Some(&dir)).unwrap();
        insert(&conn, "ghost", 2_000_000_000);

        // A duplicate name must not repeat the no-running-process report.
        let names = vec!["ghost".to_string(), "ghost".to_string()];
        let (_, captured) =
            capture(|| handle_kill_command(&conn, "/proj", &names, false, true).unwrap());
        assert_eq!(
            captured.stdout,
            vec!["No running processes found for service 'ghost' in project '/proj'".to_string()]
        );
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
