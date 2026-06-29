//! End-to-end test for the log-collector supervision loop.
//!
//! Drives `monitor::run` against a trivial shell command and asserts the
//! lifecycle rows it writes to a real SQLite database.

use candle_core::db::get_database;
use candle_core::db::process_table::find_all_processes;
use candle_core::log_collector::{monitor, LogCollectorLaunchInfo};
use candle_core::logs::process_logs::{get_process_logs, LogSearchOptions};
use candle_core::logs::ProcessLogType;

fn temp_dir(label: &str) -> std::path::PathBuf {
    let unique = format!(
        "candle-monitor-it-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn collector_records_full_lifecycle() {
    let dir = temp_dir("lifecycle");

    // Create the DB + schema, then close the handle before the collector opens it.
    let db_path = {
        let conn = get_database(Some(&dir)).unwrap();
        let p = dir.join("candle.db");
        drop(conn);
        p
    };

    let launch_info = LogCollectorLaunchInfo {
        command_name: "echo-svc".to_string(),
        project_dir: dir.to_string_lossy().into_owned(),
        // Stay alive past the 500ms grace period so the line + started/exited
        // rows are all recorded deterministically.
        shell: "echo hello && sleep 1".to_string(),
        root: None,
        enable_stdin: false,
        database_path: db_path,
    };

    let code = monitor::run(launch_info);
    assert_eq!(code, Some(0));

    // Reopen and inspect what the collector wrote.
    let conn = get_database(Some(&dir)).unwrap();

    let logs = get_process_logs(
        &conn,
        &LogSearchOptions {
            project_dir: Some(dir.to_string_lossy().into_owned()),
            command_names: vec!["echo-svc".to_string()],
            ..Default::default()
        },
    )
    .unwrap();

    // A stdout line "hello".
    assert!(
        logs.iter().any(|l| l.log_type == ProcessLogType::Stdout.as_i64()
            && l.content.as_deref() == Some("hello")),
        "expected an stdout 'hello' row; got {logs:?}"
    );
    // process_started (no content).
    assert!(
        logs.iter()
            .any(|l| l.log_type == ProcessLogType::ProcessStarted.as_i64()),
        "expected a process_started row; got {logs:?}"
    );
    // process_exited with the exact exit-code message.
    assert!(
        logs.iter().any(|l| l.log_type == ProcessLogType::ProcessExited.as_i64()
            && l.content.as_deref() == Some("Process exited with code 0")),
        "expected a process_exited row; got {logs:?}"
    );

    // The processes row was created during the run and deleted on exit.
    let procs = find_all_processes(&conn).unwrap();
    assert!(
        procs.is_empty(),
        "expected no lingering processes rows; got {procs:?}"
    );

    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}
