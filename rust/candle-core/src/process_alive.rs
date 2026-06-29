//! Process liveness probing.
//!
//! Ported from `src/process-alive.ts`. Uses a signal-0 `kill` to test whether a
//! PID is alive without actually signalling it.

use rusqlite::Connection;

use crate::db::process_table::{delete_process_entry, ProcessEntry};

/// Check whether a process with the given PID is currently alive.
///
/// Uses `kill(pid, 0)`, which sends no signal but performs the permission and
/// existence checks. Matching `isProcessAlive`:
/// - success (errno 0) -> alive
/// - `EPERM` -> the process exists but is owned by another user -> treated as alive
/// - `ESRCH` / anything else -> dead
pub fn is_process_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }

    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return true;
    }

    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Filter out process entries whose PIDs are no longer alive, deleting the stale
/// rows from the database.
///
/// Mirrors `filterAliveProcesses`: an entry is kept if its `log_collector_pid`
/// is alive OR its `pid` is alive (checked in that order). Otherwise the row is
/// deleted (keyed on command_name/project_dir/pid) and dropped from the result.
pub fn filter_alive_processes(
    conn: &Connection,
    entries: Vec<ProcessEntry>,
) -> rusqlite::Result<Vec<ProcessEntry>> {
    let mut alive = Vec::new();

    for entry in entries {
        let collector_alive = match entry.log_collector_pid {
            Some(pid) => is_process_alive(pid),
            None => false,
        };

        if collector_alive || is_process_alive(entry.pid) {
            alive.push(entry);
            continue;
        }

        delete_process_entry(conn, &entry.command_name, &entry.project_dir, entry.pid)?;
    }

    Ok(alive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::process_table::{create_process_entry, find_all_processes, CreateProcessEntry};
    use crate::db::{get_database, temp_db_dir};

    #[test]
    fn current_process_is_alive() {
        let me = std::process::id() as i64;
        assert!(is_process_alive(me));
    }

    #[test]
    fn nonexistent_pid_is_dead() {
        // Very high pid that is essentially never allocated.
        assert!(!is_process_alive(2_000_000_000));
        assert!(!is_process_alive(0));
        assert!(!is_process_alive(-1));
    }

    #[test]
    fn filter_keeps_alive_deletes_dead() {
        let dir = temp_db_dir("process-alive");
        let conn = get_database(Some(&dir)).unwrap();

        let me = std::process::id() as i64;

        // Alive: pid is the current process.
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "alive".to_string(),
                project_dir: "/proj".to_string(),
                pid: me,
                log_collector_pid: None,
                shell: None,
                root: None,
            },
        )
        .unwrap();

        // Dead: both pids point at an unallocated pid.
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "dead".to_string(),
                project_dir: "/proj".to_string(),
                pid: 2_000_000_000,
                log_collector_pid: Some(2_000_000_001),
                shell: None,
                root: None,
            },
        )
        .unwrap();

        let entries = find_all_processes(&conn).unwrap();
        let kept = filter_alive_processes(&conn, entries).unwrap();

        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].command_name, "alive");
        // Dead row deleted from the DB.
        assert_eq!(find_all_processes(&conn).unwrap().len(), 1);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collector_alive_keeps_row_even_if_pid_dead() {
        let dir = temp_db_dir("process-alive-collector");
        let conn = get_database(Some(&dir)).unwrap();

        let me = std::process::id() as i64;
        create_process_entry(
            &conn,
            &CreateProcessEntry {
                command_name: "svc".to_string(),
                project_dir: "/proj".to_string(),
                pid: 2_000_000_000, // dead
                log_collector_pid: Some(me), // alive
                shell: None,
                root: None,
            },
        )
        .unwrap();

        let entries = find_all_processes(&conn).unwrap();
        let kept = filter_alive_processes(&conn, entries).unwrap();
        assert_eq!(kept.len(), 1);

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
