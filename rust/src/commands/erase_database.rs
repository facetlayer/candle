//! `erase-database` command — delete the candle SQLite database and its WAL/SHM
//! sidecar files from the resolved state directory.
//!
//! Ported from `src/clear-database-command.ts`. Output strings (including the
//! leading U+2713 check marks and the blank line before "Database cleared
//! successfully!") match the Node implementation byte-for-byte.

use std::io::ErrorKind;
use std::path::Path;

use crate::db::get_database;
use crate::db::process_table::{find_all_running_processes, ProcessEntry};
use crate::dirs::get_state_directory;
use crate::output;
use crate::process_alive::is_process_alive;
use crate::start::service_lock::acquire_database_lock_in;

/// Services whose rows say they're running and whose shell or monitor is alive.
///
/// Read-only: unlike `filter_alive_processes` it deletes nothing, since the
/// database may be about to go away anyway. A missing database has none.
pub fn live_processes_in(state_dir: &Path) -> rusqlite::Result<Vec<ProcessEntry>> {
    if !state_dir.join("candle.db").exists() {
        return Ok(Vec::new());
    }
    let conn = get_database(Some(state_dir))?;
    let entries = find_all_running_processes(&conn)?;
    Ok(entries
        .into_iter()
        .filter(|e| is_process_alive(e.pid) || e.log_collector_pid.is_some_and(is_process_alive))
        .collect())
}

/// Outcome of [`erase_database_guarded`].
#[derive(Debug)]
pub enum EraseOutcome {
    Erased,
    /// Nothing was touched because these services are still running.
    RefusedLiveProcesses(Vec<ProcessEntry>),
}

/// Erase the database unless Candle-managed processes are still running.
///
/// Erasing under live processes orphans them: they keep running and nothing in
/// Candle can see or stop them. So without `force` this refuses. If the
/// database can't be read (corruption is a main reason to erase it), the check
/// is skipped with a warning rather than blocking the erase.
pub fn erase_database_guarded(state_dir: &Path, force: bool) -> std::io::Result<EraseOutcome> {
    // Hold the database lock exclusively across the check and the erase. Every
    // start holds it shared, so a start can't slip a new process in between
    // "nothing is running" and the files going away.
    let _lock = acquire_database_lock_in(state_dir, true)?;
    if !force {
        match live_processes_in(state_dir) {
            Ok(live) if !live.is_empty() => return Ok(EraseOutcome::RefusedLiveProcesses(live)),
            Ok(_) => {}
            Err(e) => output::err(&format!(
                "Warning: could not read the database to check for running processes ({e}); erasing anyway."
            )),
        }
    }
    erase_database_in(state_dir)?;
    Ok(EraseOutcome::Erased)
}

/// Stderr text for a refused erase.
pub fn format_refusal(live: &[ProcessEntry]) -> String {
    let noun = if live.len() == 1 {
        "process is"
    } else {
        "processes are"
    };
    let mut msg = format!(
        "Refusing to erase the database: {} Candle-managed {noun} still running.\n\
         Erasing now would leave them running with no way for Candle to stop them.\n",
        live.len()
    );
    for e in live {
        msg.push_str(&format!(
            "  {} (pid {}) in {}\n",
            e.command_name, e.pid, e.project_dir
        ));
    }
    msg.push_str("Run 'candle kill-all' first, or pass --force to erase anyway.");
    msg
}

/// `candle erase-database [--force]` against the resolved state directory.
pub fn handle_erase_database_command(force: bool) -> std::io::Result<EraseOutcome> {
    erase_database_guarded(&get_state_directory(), force)
}

/// Core logic, operating on an explicit state directory.
///
/// Missing files are reported but not an error; an unexpected I/O failure
/// returns `Err` so the CLI can print `Error clearing database: <e>` and exit 1.
///
/// A file can also disappear between the `exists()` check and the removal — a
/// monitor process shutting down lets SQLite checkpoint away its own WAL/SHM —
/// so a `NotFound` from the removal itself counts as "already gone" too.
pub fn erase_database_in(state_dir: &Path) -> std::io::Result<()> {
    let db_path = state_dir.join("candle.db");
    let wal_path = state_dir.join("candle.db-wal");
    let shm_path = state_dir.join("candle.db-shm");

    output::out(&format!("Clearing database at: {}", db_path.display()));

    // Main database file: report whether it was present.
    if remove_if_present(&db_path)? {
        output::out("\u{2713} Removed database file");
    } else {
        output::out("- Database file not found");
    }

    // WAL / shared-memory sidecars: only reported when present.
    if remove_if_present(&wal_path)? {
        output::out("\u{2713} Removed WAL file");
    }
    if remove_if_present(&shm_path)? {
        output::out("\u{2713} Removed shared memory file");
    }

    output::out("\nDatabase cleared successfully!");
    output::out("A new database will be created on next use.");
    Ok(())
}

/// Delete `path`, returning whether it was actually there to delete.
///
/// Treats a `NotFound` as "already gone" rather than an error.
fn remove_if_present(path: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;
    use crate::output::capture;

    #[test]
    fn removes_db_and_reports_missing_sidecars() {
        let dir = temp_db_dir("erase-database");
        std::fs::write(dir.join("candle.db"), b"x").unwrap();

        let (res, captured) = capture(|| erase_database_in(&dir));
        res.unwrap();

        assert!(!dir.join("candle.db").exists());
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "\u{2713} Removed database file"));
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "\nDatabase cleared successfully!"));
        // No stderr on success.
        assert!(captured.stderr.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_state_dir_is_not_an_error() {
        // The state dir itself never existing surfaces as ENOENT from the
        // removal, the same way a concurrently-checkpointed WAL file does.
        let dir = temp_db_dir("erase-database-missing").join("never-created");

        let (res, captured) = capture(|| erase_database_in(&dir));
        res.unwrap();
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "- Database file not found"));
        assert!(captured.stderr.is_empty());
    }

    fn insert_process(dir: &Path, pid: i64) {
        let conn = get_database(Some(dir)).unwrap();
        crate::db::process_table::create_process_entry(
            &conn,
            &crate::db::process_table::CreateProcessEntry {
                command_name: "svc".to_string(),
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
    fn refuses_while_a_tracked_process_is_alive() {
        let dir = temp_db_dir("erase-database-live");
        // Our own PID is guaranteed alive.
        insert_process(&dir, std::process::id() as i64);

        let (res, _) = capture(|| erase_database_guarded(&dir, false));
        match res.unwrap() {
            EraseOutcome::RefusedLiveProcesses(live) => {
                assert_eq!(live.len(), 1);
                let msg = format_refusal(&live);
                assert!(msg.contains("svc (pid"));
                assert!(msg.contains("--force"));
            }
            other => panic!("expected refusal, got {other:?}"),
        }
        assert!(
            dir.join("candle.db").exists(),
            "refused erase must not delete anything"
        );

        let (res, _) = capture(|| erase_database_guarded(&dir, true));
        assert!(matches!(res.unwrap(), EraseOutcome::Erased));
        assert!(!dir.join("candle.db").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_rows_do_not_block_erase() {
        let dir = temp_db_dir("erase-database-dead");
        // PIDs this large don't exist on macOS or Linux.
        insert_process(&dir, 99_999_999);

        let (res, _) = capture(|| erase_database_guarded(&dir, false));
        assert!(matches!(res.unwrap(), EraseOutcome::Erased));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_db_warns_and_erases() {
        let dir = temp_db_dir("erase-database-corrupt");
        std::fs::write(
            dir.join("candle.db"),
            b"this is not a sqlite file at all, not even close",
        )
        .unwrap();

        let (res, captured) = capture(|| erase_database_guarded(&dir, false));
        assert!(matches!(res.unwrap(), EraseOutcome::Erased));
        assert!(captured.stderr.iter().any(|l| l.contains("erasing anyway")));
        assert!(!dir.join("candle.db").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn erase_waits_for_an_in_progress_start() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        let dir = temp_db_dir("erase-database-lock");
        std::fs::write(dir.join("candle.db"), b"x").unwrap();
        let start_lock = crate::start::service_lock::acquire_in(&dir, "/proj", "svc").unwrap();

        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let dir2 = dir.clone();
        let handle = std::thread::spawn(move || {
            let res = capture(|| erase_database_guarded(&dir2, true)).0;
            flag.store(true, Ordering::SeqCst);
            res.unwrap();
        });

        std::thread::sleep(Duration::from_millis(150));
        assert!(!done.load(Ordering::SeqCst), "erase ran during a start");
        assert!(dir.join("candle.db").exists());

        drop(start_lock);
        handle.join().unwrap();
        assert!(!dir.join("candle.db").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_waits_for_an_in_progress_erase() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        let dir = temp_db_dir("erase-database-lock-2");
        let erase_lock = crate::start::service_lock::acquire_database_lock_in(&dir, true).unwrap();

        let acquired = Arc::new(AtomicBool::new(false));
        let flag = acquired.clone();
        let dir2 = dir.clone();
        let handle = std::thread::spawn(move || {
            let _l = crate::start::service_lock::acquire_in(&dir2, "/proj", "svc").unwrap();
            flag.store(true, Ordering::SeqCst);
        });

        std::thread::sleep(Duration::from_millis(150));
        assert!(
            !acquired.load(Ordering::SeqCst),
            "start ran during an erase"
        );
        drop(erase_lock);
        handle.join().unwrap();
        assert!(acquired.load(Ordering::SeqCst));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_db_is_not_an_error() {
        let dir = temp_db_dir("erase-database-empty");

        let (res, captured) = capture(|| erase_database_in(&dir));
        res.unwrap();
        assert!(captured
            .stdout
            .iter()
            .any(|l| l == "- Database file not found"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
