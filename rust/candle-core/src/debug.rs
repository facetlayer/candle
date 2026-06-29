//! Debug logging.
//!
//! Ported from `src/debug.ts`. When `CANDLE_ENABLE_LOGS` is set (to a non-empty
//! value), appends messages to a `candle.log` file in the current working
//! directory.

use std::fs::OpenOptions;
use std::io::Write;

/// Append `msg` plus a newline to `<cwd>/candle.log` when logging is enabled.
///
/// Enabled when `CANDLE_ENABLE_LOGS` is set to a non-empty value (matching JS
/// truthiness; the value is not parsed as a boolean). IO errors are swallowed so
/// a read-only cwd never crashes the CLI.
pub fn debug_log(msg: &str) {
    let enabled = std::env::var("CANDLE_ENABLE_LOGS")
        .map(|v| !v.is_empty())
        .unwrap_or(false);

    if !enabled {
        return;
    }

    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let log_path = cwd.join("candle.log");

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that mutate env + cwd to avoid cross-test races.
    static GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn writes_when_enabled() {
        let _lock = GUARD.lock().unwrap();

        let unique = format!(
            "candle-debug-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();

        let prev_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::env::set_var("CANDLE_ENABLE_LOGS", "1");

        debug_log("hello");
        debug_log("world");

        let contents = std::fs::read_to_string(dir.join("candle.log")).unwrap();

        // Restore before asserting so a failure does not leave a bad cwd.
        std::env::remove_var("CANDLE_ENABLE_LOGS");
        std::env::set_current_dir(&prev_cwd).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(contents, "hello\nworld\n");
    }

    #[test]
    fn no_file_when_disabled() {
        let _lock = GUARD.lock().unwrap();

        let unique = format!(
            "candle-debug-test-off-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();

        let prev_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::env::remove_var("CANDLE_ENABLE_LOGS");

        debug_log("should not write");

        let exists = dir.join("candle.log").exists();

        std::env::set_current_dir(&prev_cwd).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!exists);
    }
}
