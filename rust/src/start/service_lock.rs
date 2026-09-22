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
pub struct ServiceStartLock {
    _file: File,
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
    let path = lock_path(state_dir, project_dir, service_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)?;

    loop {
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if rc == 0 {
            return Ok(ServiceStartLock { _file: file });
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
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
