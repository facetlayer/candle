//! Process killing: signalling a process tree, the per-entry kill state machine,
//! and the `kill` / `kill-all` command handlers.
//!
//! Ports `src/kill/killProcessTree.ts`, `src/kill/killOneRunningProcess.ts`,
//! `src/kill-command.ts`, and `src/kill-all-command.ts`. See
//! `rust/docs/architecture/kill-restart.md`.
//!
//! Kill is a *mark*, not a hard delete: the normal success path only sets
//! `killed_at` so `candle list` immediately stops reporting the row as RUNNING;
//! final deletion is done by the stale-cleanup reaper. The not-found and
//! 5-minute-stale paths delete the row outright.

use std::collections::HashSet;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::db::process_table::{
    delete_process_entry, find_all_processes, find_processes_by_command_name_and_project_dir,
    find_running_processes_by_project_dir, update_process_killed_at, ProcessEntry,
};
use crate::output;
use crate::process_alive::is_process_alive;
use crate::process_tree::get_process_tree;

/// How long a signalled process gets to exit on `SIGTERM` before the tree is
/// escalated to `SIGKILL`.
pub const KILL_GRACE_PERIOD: Duration = Duration::from_secs(5);
/// How long to wait for a `SIGKILL`ed root to disappear before giving up.
const SIGKILL_WAIT: Duration = Duration::from_secs(1);
/// Poll interval while waiting for a signalled process to exit.
const KILL_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// How long after `killed_at` an entry is considered stale and hard-deleted on a
/// repeat kill (5 minutes), matching `killOneRunningProcess`.
const STALE_ENTRY_SECONDS: i64 = 5 * 60;

/// Outcome of signalling a process tree. Mirrors the Node string union
/// `'success' | 'process_not_found' | 'error'`.
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

/// Send `SIGTERM` to every process in the tree rooted at `pid`, children first.
///
/// Mirrors `killProcessTree`:
/// - The whole tree is snapshotted up front; newly-forked grandchildren are not
///   pursued.
/// - Order is deepest-descendant-first, root shell last.
/// - `ESRCH` (no such process) is ignored; any other errno is a warning + error.
/// - There is **no wait/timeout** here; see [`kill_process_tree_and_wait`] for
///   the grace-period + `SIGKILL` escalation used by `kill`.
///
/// # Panics
/// Panics on `pid <= 0` (an internal-invariant violation; callers guard a
/// falsy/zero PID before reaching here).
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

    // Children first (reverse of discovery order), root last.
    for child_pid in pids.into_iter().rev() {
        let result = unsafe { libc::kill(child_pid as libc::pid_t, libc::SIGTERM) };
        if result == 0 {
            all_not_found = false;
            continue;
        }

        let os_err = std::io::Error::last_os_error();
        if os_err.raw_os_error() == Some(libc::ESRCH) {
            // Process already gone; ignore and continue.
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

/// Wait until every PID in `pids` is gone or `timeout` elapses. Returns
/// whether they all exited.
fn wait_for_all_to_exit(pids: &[i64], timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !pids.iter().copied().any(is_process_alive) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(KILL_POLL_INTERVAL);
    }
}

/// `SIGTERM` the tree rooted at `pid`, wait up to `grace` for every process in
/// it to exit, and `SIGKILL` whatever is left.
///
/// A service that traps or ignores `SIGTERM` would otherwise keep running while
/// Candle's records said it was dead. The whole tree is waited on, not just the
/// root: when the root shell exits on `SIGTERM`, a child that ignored it gets
/// reparented to init and would drop out of any re-snapshot taken from the
/// root. Returns `Escalated` when `SIGKILL` was needed, so callers can tell the
/// user. Other outcomes match [`kill_process_tree`].
pub fn kill_process_tree_and_wait(pid: i64, grace: Duration) -> KillOutcome {
    let snapshot = get_process_tree(pid);

    match kill_process_tree(pid) {
        KillResult::Success => {}
        KillResult::ProcessNotFound => return KillOutcome::ProcessNotFound,
        KillResult::Error => return KillOutcome::Error,
    }

    if wait_for_all_to_exit(&snapshot, grace) {
        return KillOutcome::Terminated;
    }

    // Survivors from the snapshot, plus anything they've forked since. Kill
    // children before parents so a dying parent can't respawn them.
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

    if wait_for_all_to_exit(&targets, SIGKILL_WAIT) {
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

/// Kill one process entry and update its database row accordingly.
///
/// Mirrors `killOneRunningProcess`. A falsy (zero) PID is a no-op. Otherwise:
/// - **Success**: print `[Killed ...]` (unless `quiet`); then if the row was
///   already marked killed over 5 minutes ago, warn + hard-delete it; otherwise
///   mark `killed_at = now`.
/// - **ProcessNotFound**: hard-delete the row (the OS process is gone). The
///   warning is printed only for a row that still claimed to be running; a row
///   already marked killed is expected to be gone (e.g. the second kill inside
///   `restart`), so sweeping it is silent.
/// - **Error**: print `Error killing process ...` (to stdout, matching Node) and
///   leave the row unchanged.
///
/// Returns whether this was a real kill — i.e. there was a live process to
/// signal. Sweeping a leftover row whose OS process had already exited returns
/// `false`, so callers do not mistake garbage collection for a kill.
pub fn kill_one_running_process(
    conn: &Connection,
    entry: &ProcessEntry,
    quiet: bool,
) -> rusqlite::Result<bool> {
    // Falsy PID (Node `if (process.pid)`): nothing to kill.
    if entry.pid == 0 {
        return Ok(false);
    }

    let killed = match kill_process_tree_and_wait(entry.pid, KILL_GRACE_PERIOD) {
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
                // Note: Node emits this on stdout (console.log), not stderr.
                output::out(&format!(
                    "Error killing process '{}' with PID: {}",
                    entry.command_name, entry.pid
                ));
            }

            // The process was there but would not die; the error message above
            // already told the user, so this counts as handled.
            true
        }
    };

    Ok(killed)
}

/// Handle `candle kill [name...]`.
///
/// Mirrors `handleKillCommand`:
/// - With names: dedupe (first-occurrence order) and kill each name's entries,
///   querying **all** matching rows (including already-killed). A name with no
///   *live* process prints the per-service "No running processes" message unless
///   `quiet_failure` — leftover rows swept along the way do not count as kills,
///   so the message does not depend on how promptly the reaper has run.
/// - Without names: kill every running row in the project; if none, print the
///   project-wide "No running processes" message unless `quiet_failure`.
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

/// Handle `candle kill-all`: kill every row across every project, system-wide.
///
/// Mirrors `handleKillAll` — uses `find_all_processes` (no project/killed filter)
/// so already-killed-but-unreaped rows are cleaned up as a side effect. Prints
/// `No running processes found` when there were no rows at all.
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

    #[test]
    fn kill_one_dead_process_deletes_row() {
        let dir = temp_db_dir("kill-one-dead");
        let conn = get_database(Some(&dir)).unwrap();
        insert(&conn, "svc", 2_000_000_000);

        let entry = find_all_processes(&conn).unwrap().pop().unwrap();
        let (_, captured) = capture(|| kill_one_running_process(&conn, &entry, false).unwrap());

        // Dead PID -> process_not_found -> row deleted, "Cleaning up stale" on stderr.
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
        // A leftover row for an already-exited process is swept, but the sweep is
        // garbage collection, not a kill: the user still gets told nothing was
        // running. Regression test for a flake where `kill <name>` printed nothing
        // at all when a prior kill's row had not been reaped yet.
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

        // "ghost" repeated: should only be processed once. Its single (dead) row
        // is deleted on the first pass; the second pass would otherwise re-report.
        let names = vec!["ghost".to_string(), "ghost".to_string()];
        let (_, captured) =
            capture(|| handle_kill_command(&conn, "/proj", &names, false, true).unwrap());
        // quiet=true suppresses the cleanup warning, so the only line is the
        // "nothing was running" report — emitted once, not once per duplicate.
        assert_eq!(
            captured.stdout,
            vec!["No running processes found for service 'ghost' in project '/proj'".to_string()]
        );
        assert_eq!(find_all_processes(&conn).unwrap().len(), 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
