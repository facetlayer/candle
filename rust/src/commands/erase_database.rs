//! `erase-database` command — delete the candle SQLite database and its WAL/SHM
//! sidecar files from the resolved state directory.
//!
//! Ported from `src/clear-database-command.ts`. Output strings (including the
//! leading U+2713 check marks and the blank line before "Database cleared
//! successfully!") match the Node implementation byte-for-byte.

use std::path::Path;

use crate::dirs::get_state_directory;
use crate::output;

/// Remove `candle.db` (+ `-wal` / `-shm`) from the resolved state directory.
///
/// Mirrors `handleClearDatabaseCommand`. See [`erase_database_in`] for the core
/// logic (parameterized on the state dir for testability).
pub fn handle_erase_database_command() -> std::io::Result<()> {
    erase_database_in(&get_state_directory())
}

/// Core logic, operating on an explicit state directory.
///
/// Missing files are reported but not an error; an unexpected I/O failure
/// returns `Err` so the CLI can print `Error clearing database: <e>` and exit 1.
pub fn erase_database_in(state_dir: &Path) -> std::io::Result<()> {
    let db_path = state_dir.join("candle.db");
    let wal_path = state_dir.join("candle.db-wal");
    let shm_path = state_dir.join("candle.db-shm");

    output::out(&format!("Clearing database at: {}", db_path.display()));

    // Main database file: report whether it was present.
    if db_path.exists() {
        std::fs::remove_file(&db_path)?;
        output::out("\u{2713} Removed database file");
    } else {
        output::out("- Database file not found");
    }

    // WAL / shared-memory sidecars: only reported when present.
    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
        output::out("\u{2713} Removed WAL file");
    }
    if shm_path.exists() {
        std::fs::remove_file(&shm_path)?;
        output::out("\u{2713} Removed shared memory file");
    }

    output::out("\nDatabase cleared successfully!");
    output::out("A new database will be created on next use.");
    Ok(())
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
        assert!(captured.stdout.iter().any(|l| l == "\u{2713} Removed database file"));
        assert!(captured.stdout.iter().any(|l| l == "\nDatabase cleared successfully!"));
        // No stderr on success.
        assert!(captured.stderr.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_db_is_not_an_error() {
        let dir = temp_db_dir("erase-database-empty");

        let (res, captured) = capture(|| erase_database_in(&dir));
        res.unwrap();
        assert!(captured.stdout.iter().any(|l| l == "- Database file not found"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
