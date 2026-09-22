//! Per-service start lock.
//!
//! `start` is kill-then-launch. Without serialization, two concurrent starts of
//! the same service each see "nothing running" (or each kill the same old
//! instance) and each launch a new one, leaving duplicate instances that only
//! `list-all` can see. Coding agents running parallel subagents hit this easily.
//!
//! The lock is an advisory `flock` on a file under `<state dir>/locks/`, keyed by
//! project directory + service name. It is held for the whole start sequence and
//! released when the guard drops, or by the kernel if the CLI dies. Rust opens
//! files close-on-exec, so the detached monitor never inherits it.

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::dirs::get_state_directory;

/// Holds the lock until dropped.
///
/// Also holds a shared lock on the database lock file, so `erase-database`
/// (which takes it exclusively) can't erase while a start is in progress.
pub struct ServiceStartLock {
    _database: DatabaseLock,
    _file: File,
}

/// A shared or exclusive hold on the database lock file. Released on drop.
pub struct DatabaseLock {
    _file: File,
}

/// Path of the lock file that `erase-database` takes exclusively and every
/// start takes shared.
pub fn database_lock_path(state_dir: &Path) -> PathBuf {
    state_dir.join("locks").join("database.lock")
}

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

/// Block until `flock(operation)` succeeds on `file`, retrying on EINTR.
fn flock_blocking(file: &File, operation: libc::c_int) -> std::io::Result<()> {
    loop {
        let rc = unsafe { libc::flock(file.as_raw_fd(), operation) };
        if rc == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

/// Take the database lock shared (starts) or exclusive (`erase-database`).
pub fn acquire_database_lock_in(
    state_dir: &Path,
    exclusive: bool,
) -> std::io::Result<DatabaseLock> {
    let file = open_lock_file(&database_lock_path(state_dir))?;
    let op = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    };
    flock_blocking(&file, op)?;
    Ok(DatabaseLock { _file: file })
}

/// Stable 64-bit FNV-1a, so every candle build maps a service to the same file.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Lock-file path for a service inside `state_dir`.
pub fn lock_path(state_dir: &Path, project_dir: &str, service_name: &str) -> PathBuf {
    let mut key = Vec::with_capacity(project_dir.len() + service_name.len() + 1);
    key.extend_from_slice(project_dir.as_bytes());
    key.push(0);
    key.extend_from_slice(service_name.as_bytes());
    state_dir
        .join("locks")
        .join(format!("start-{:016x}.lock", fnv1a(&key)))
}

/// Block until this process holds the start lock for the service.
pub fn acquire_in(
    state_dir: &Path,
    project_dir: &str,
    service_name: &str,
) -> std::io::Result<ServiceStartLock> {
    // Always database lock first, then the service lock. Erase only ever takes
    // the database lock, so this fixed order can't deadlock.
    let database = acquire_database_lock_in(state_dir, false)?;
    let file = open_lock_file(&lock_path(state_dir, project_dir, service_name))?;
    flock_blocking(&file, libc::LOCK_EX)?;
    Ok(ServiceStartLock {
        _database: database,
        _file: file,
    })
}

/// [`acquire_in`] using the resolved state directory.
pub fn acquire(project_dir: &str, service_name: &str) -> std::io::Result<ServiceStartLock> {
    acquire_in(&get_state_directory(), project_dir, service_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn distinct_services_get_distinct_files() {
        let dir = PathBuf::from("/state");
        assert_ne!(lock_path(&dir, "/p", "a"), lock_path(&dir, "/p", "b"));
        assert_ne!(lock_path(&dir, "/p1", "a"), lock_path(&dir, "/p2", "a"));
        // The separator keeps ("/p", "ab") and ("/pa", "b") apart.
        assert_ne!(lock_path(&dir, "/p", "ab"), lock_path(&dir, "/pa", "b"));
        assert_eq!(lock_path(&dir, "/p", "a"), lock_path(&dir, "/p", "a"));
    }

    #[test]
    fn second_acquire_blocks_until_first_drops() {
        let dir = temp_db_dir("service-lock");
        let first = acquire_in(&dir, "/proj", "svc").unwrap();

        let acquired = Arc::new(AtomicBool::new(false));
        let flag = acquired.clone();
        let dir2 = dir.clone();
        let handle = std::thread::spawn(move || {
            // flock locks belong to the open file description, so a second
            // open in the same process contends like another process would.
            let _second = acquire_in(&dir2, "/proj", "svc").unwrap();
            flag.store(true, Ordering::SeqCst);
        });

        std::thread::sleep(Duration::from_millis(150));
        assert!(
            !acquired.load(Ordering::SeqCst),
            "second lock acquired while first held"
        );

        drop(first);
        handle.join().unwrap();
        assert!(acquired.load(Ordering::SeqCst));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
