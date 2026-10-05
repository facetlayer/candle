//! Verify monitor lifecycle rows and output in a real SQLite database.

use candle::db::get_database;
use candle::db::process_table::find_all_processes;
use candle::logs::process_logs::{get_process_logs, LogSearchOptions};
use candle::logs::ProcessLogType;
use candle::monitor::{self, MonitorLaunchInfo};

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

    let db_path = {
        let conn = get_database(Some(&dir)).unwrap();
        let p = dir.join("candle.db");
        drop(conn);
        p
    };

    let launch_info = MonitorLaunchInfo {
        command_name: "echo-svc".to_string(),
        project_dir: dir.to_string_lossy().into_owned(),
        // Stay alive past startup grace so started/exited rows are deterministic.
        shell: "echo hello && sleep 1".to_string(),
        root: None,
        enable_stdin: false,
        database_path: db_path,
        run_id: None,
        transient: false,
    };

    let code = monitor::run(launch_info);
    assert_eq!(code, Some(0));

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

    assert!(
        logs.iter()
            .any(|l| l.log_type == ProcessLogType::Stdout.as_i64()
                && l.content.as_deref() == Some("hello")),
        "expected an stdout 'hello' row; got {logs:?}"
    );
    assert!(
        logs.iter()
            .any(|l| l.log_type == ProcessLogType::ProcessStarted.as_i64()),
        "expected a process_started row; got {logs:?}"
    );
    assert!(
        logs.iter()
            .any(|l| l.log_type == ProcessLogType::ProcessExited.as_i64()
                && l.content.as_deref() == Some("Process exited with code 0")),
        "expected a process_exited row; got {logs:?}"
    );

    let procs = find_all_processes(&conn).unwrap();
    assert!(
        procs.is_empty(),
        "expected no lingering processes rows; got {procs:?}"
    );

    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}
