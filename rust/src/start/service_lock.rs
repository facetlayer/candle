//! Serialize launches with per-service flock files to prevent duplicate starts.
//! Hold through startup; drop or process exit releases the lock. Close-on-exec
//! prevents the monitor from inheriting it.

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::dirs::get_state_directory;

/// Hold the service lock and shared database lock until dropped, preventing
/// concurrent starts and database erasure.
pub struct ServiceStartLock {
    _database: DatabaseLock,
    _file: File,
}

/// A shared or exclusive hold on the database lock file. Released on drop.
pub struct DatabaseLock {
    _file: File,
}

/// Database lock path: shared for starts, exclusive for erasure.
pub fn database_lock_path(state_dir: &Path) -> PathBuf {
    state_dir.join("locks").join("database.lock")
}

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        // Lock acquisition can precede database creation.
        crate::db::create_private_dir(parent)?;
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
    // Acquire database before service locks to prevent deadlocks.
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
            // Separate opens contend even within the same process.
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
