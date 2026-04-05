use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use rusqlite::Connection;
use serde::Deserialize;

// -- Log types matching ProcessLogType.ts --

const LOG_TYPE_STDOUT: i32 = 1;
const LOG_TYPE_STDERR: i32 = 2;
#[allow(dead_code)]
const LOG_TYPE_PROCESS_START_INITIATED: i32 = 3;
const LOG_TYPE_PROCESS_START_FAILED: i32 = 4;
const LOG_TYPE_PROCESS_STARTED: i32 = 5;
const LOG_TYPE_PROCESS_EXITED: i32 = 6;

// -- Launch info (matches LogCollectorLaunchInfo.ts) --

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchInfo {
    command_name: String,
    project_dir: String,
    shell: String,
    root: Option<String>,
    enable_stdin: Option<bool>,
    database_path: String,
}

// -- CLI args (alternative to stdin JSON) --

#[derive(Parser, Debug)]
#[command(name = "candle-log-collector")]
struct Args {
    #[arg(long)]
    command_name: Option<String>,

    #[arg(long)]
    project_dir: Option<String>,

    #[arg(long)]
    shell: Option<String>,

    #[arg(long)]
    root: Option<String>,

    #[arg(long)]
    enable_stdin: Option<bool>,

    #[arg(long)]
    database_path: Option<String>,
}

// -- Constants --

const CLEANUP_INTERVAL_SECS: i64 = 10 * 60;
const DEFAULT_MAX_LOGS_PER_SERVICE: i64 = 1000;
const DEFAULT_MAX_RETENTION_SECS: i64 = 24 * 60 * 60;
const GRACE_PERIOD_MS: u64 = 500;
const STDIN_POLL_INTERVAL_MS: u64 = 500;

// -- Database helpers --

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn open_database(db_path: &str) -> Connection {
    if !Path::new(db_path).exists() {
        eprintln!("Error: database file does not exist: {}", db_path);
        std::process::exit(1);
    }

    let conn = Connection::open(db_path).unwrap_or_else(|e| {
        eprintln!("Error: failed to open database at {}: {}", db_path, e);
        std::process::exit(1);
    });

    conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
    conn.execute_batch("PRAGMA busy_timeout=30000;").ok();

    // Verify that the expected tables exist
    let required_tables = ["processes", "process_output", "process_last_cleanup", "stdin_messages"];
    for table in &required_tables {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !exists {
            eprintln!("Error: expected table '{}' not found in database at {}", table, db_path);
            std::process::exit(1);
        }
    }

    conn
}

fn save_process_log(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    log_type: i32,
    content: Option<&str>,
) {
    conn.execute(
        "INSERT INTO process_output(command_name, project_dir, content, log_type, timestamp) VALUES(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![command_name, project_dir, content, log_type, now_unix()],
    )
    .ok();
}

fn create_process_entry(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    pid: u32,
    log_collector_pid: u32,
    shell: &str,
    root: Option<&str>,
) {
    conn.execute(
        "INSERT INTO processes(command_name, project_dir, pid, log_collector_pid, start_time, shell, root) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![command_name, project_dir, pid, log_collector_pid, now_unix(), shell, root],
    )
    .ok();
}

fn delete_process_entry(conn: &Connection, command_name: &str, project_dir: &str, pid: u32) {
    conn.execute(
        "DELETE FROM processes WHERE command_name = ?1 AND project_dir = ?2 AND pid = ?3",
        rusqlite::params![command_name, project_dir, pid],
    )
    .ok();
}

// -- Stdin message polling --

fn pop_stdin_message(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
) -> Option<String> {
    let result: Result<(i64, String), _> = conn.query_row(
        "SELECT id, data FROM stdin_messages WHERE command_name = ?1 AND project_dir = ?2 ORDER BY id ASC LIMIT 1",
        rusqlite::params![command_name, project_dir],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    if let Ok((id, data)) = result {
        conn.execute("DELETE FROM stdin_messages WHERE id = ?1", [id]).ok();
        Some(data)
    } else {
        None
    }
}

fn clear_stdin_messages(conn: &Connection, command_name: &str, project_dir: &str) {
    conn.execute(
        "DELETE FROM stdin_messages WHERE command_name = ?1 AND project_dir = ?2",
        rusqlite::params![command_name, project_dir],
    )
    .ok();
}

// -- Cleanup --

fn maybe_run_cleanup(conn: &Connection) {
    let now = now_unix();
    let last_cleanup: Option<i64> = conn
        .query_row(
            "SELECT timestamp FROM process_last_cleanup",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(ts) = last_cleanup {
        if now - ts < CLEANUP_INTERVAL_SECS {
            return;
        }
    }

    run_cleanup(conn);
}

fn run_cleanup(conn: &Connection) {
    let now = now_unix();

    // Time-based eviction
    let log_cutoff = now - DEFAULT_MAX_RETENTION_SECS;
    conn.execute("DELETE FROM process_output WHERE timestamp < ?1", [log_cutoff])
        .ok();

    // Stale process cleanup - check if PIDs are still alive
    {
        let mut stmt = conn
            .prepare("SELECT id, pid, log_collector_pid FROM processes WHERE killed_at IS NULL")
            .unwrap();
        let stale: Vec<i64> = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let pid: i64 = row.get(1)?;
                let lc_pid: Option<i64> = row.get(2)?;
                Ok((id, pid, lc_pid))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .filter(|(_, pid, lc_pid)| {
                let pid_dead = !is_process_alive(*pid as u32);
                let lc_dead = lc_pid.map_or(true, |p| !is_process_alive(p as u32));
                pid_dead && lc_dead
            })
            .map(|(id, _, _)| id)
            .collect();

        for id in stale {
            conn.execute("DELETE FROM processes WHERE id = ?1", [id]).ok();
        }
    }

    // Per-service eviction
    {
        let mut svc_stmt = conn
            .prepare(
                "SELECT project_dir, command_name, COUNT(*) as log_count
                 FROM process_output
                 GROUP BY project_dir, command_name
                 HAVING COUNT(*) > ?1",
            )
            .unwrap();
        let services: Vec<(String, String)> = svc_stmt
            .query_map([DEFAULT_MAX_LOGS_PER_SERVICE], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for (proj_dir, cmd_name) in services {
            let cutoff_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM process_output WHERE project_dir = ?1 AND command_name = ?2 ORDER BY timestamp DESC, id DESC LIMIT 1 OFFSET ?3",
                    rusqlite::params![proj_dir, cmd_name, DEFAULT_MAX_LOGS_PER_SERVICE],
                    |row| row.get(0),
                )
                .ok();

            if let Some(cutoff) = cutoff_id {
                conn.execute(
                    "DELETE FROM process_output WHERE project_dir = ?1 AND command_name = ?2 AND id <= ?3",
                    rusqlite::params![proj_dir, cmd_name, cutoff],
                )
                .ok();
            }
        }
    }

    conn.execute_batch("VACUUM;").ok();

    // Upsert cleanup timestamp
    let existing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM process_last_cleanup",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if existing > 0 {
        conn.execute("UPDATE process_last_cleanup SET timestamp = ?1", [now_unix()])
            .ok();
    } else {
        conn.execute(
            "INSERT INTO process_last_cleanup(timestamp) VALUES(?1)",
            [now_unix()],
        )
        .ok();
    }
}

fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

// -- Debug logging --

fn debug_log(msg: &str) {
    if env::var("CANDLE_ENABLE_LOGS").is_ok() {
        if let Ok(cwd) = env::current_dir() {
            let log_path = cwd.join("candle.log");
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log_path) {
                writeln!(f, "{}", msg).ok();
            }
        }
    }
}

// -- Line event from child process --

enum LineEvent {
    Stdout(String),
    Stderr(String),
    Exit(Option<i32>),
}

// -- Main --

fn get_launch_info() -> LaunchInfo {
    let args = Args::parse();

    // If CLI args are provided, use them
    if let (Some(command_name), Some(project_dir), Some(shell), Some(database_path)) =
        (args.command_name, args.project_dir, args.shell, args.database_path)
    {
        return LaunchInfo {
            command_name,
            project_dir: fs::canonicalize(&project_dir)
                .unwrap_or_else(|_| PathBuf::from(&project_dir))
                .to_string_lossy()
                .into_owned(),
            shell,
            root: args.root,
            enable_stdin: args.enable_stdin,
            database_path,
        };
    }

    // Otherwise, read from stdin
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("Failed to read stdin");
    serde_json::from_str(&input).expect("Failed to parse launch info from stdin")
}

fn main() {
    let launch_info = get_launch_info();
    let enable_stdin = launch_info.enable_stdin.unwrap_or(false);

    debug_log(&format!(
        "[rust-log-collector] Got launchInfo: {:?}",
        launch_info
    ));

    let conn = open_database(&launch_info.database_path);

    // Determine launch directory
    let launch_dir = if let Some(ref root) = launch_info.root {
        Path::new(&launch_info.project_dir).join(root)
    } else {
        PathBuf::from(&launch_info.project_dir)
    };

    // Spawn the monitored service
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&launch_info.shell)
        .current_dir(&launch_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if enable_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()
        .expect("Failed to spawn child process");

    let child_pid = child.id();
    let my_pid = std::process::id();

    debug_log(&format!(
        "[rust-log-collector] Launched subprocess, pid={}",
        child_pid
    ));

    // Register in process table
    create_process_entry(
        &conn,
        &launch_info.command_name,
        &launch_info.project_dir,
        child_pid,
        my_pid,
        &launch_info.shell,
        launch_info.root.as_deref(),
    );

    // Set up line-by-line capture via channels
    let (tx, rx) = mpsc::channel::<LineEvent>();

    // Stdout reader thread
    let stdout = child.stdout.take().unwrap();
    let tx_out = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx_out.send(LineEvent::Stdout(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Stderr reader thread
    let stderr = child.stderr.take().unwrap();
    let tx_err = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx_err.send(LineEvent::Stderr(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Stdin polling thread (if enabled)
    let stdin_handle = if enable_stdin {
        let mut child_stdin = child.stdin.take();
        clear_stdin_messages(&conn, &launch_info.command_name, &launch_info.project_dir);

        let cmd_name = launch_info.command_name.clone();
        let proj_dir = launch_info.project_dir.clone();
        let db_path = launch_info.database_path.clone();

        Some(thread::spawn(move || {
            let poll_conn = open_database(&db_path);
            loop {
                thread::sleep(Duration::from_millis(STDIN_POLL_INTERVAL_MS));
                if let Some(ref mut stdin) = child_stdin {
                    if let Some(data) = pop_stdin_message(&poll_conn, &cmd_name, &proj_dir) {
                        debug_log(&format!(
                            "[rust-log-collector] writing stdin message: {}",
                            data
                        ));
                        if stdin.write_all(data.as_bytes()).is_err() {
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        }))
    } else {
        None
    };

    // Wait thread - waits for child to exit and sends Exit event
    let tx_exit = tx;
    thread::spawn(move || {
        let status = child.wait();
        let code = status.ok().and_then(|s| s.code());
        tx_exit.send(LineEvent::Exit(code)).ok();
    });

    // Grace period: collect output for a bit, then check if process is still alive
    let grace_deadline = SystemTime::now() + Duration::from_millis(GRACE_PERIOD_MS);
    let mut exited_during_grace = false;
    let mut exit_code: Option<i32> = None;

    loop {
        let remaining = grace_deadline
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO);

        if remaining.is_zero() {
            break;
        }

        match rx.recv_timeout(remaining) {
            Ok(LineEvent::Stdout(line)) => {
                debug_log(&format!("[rust-log-collector] stdout: {}", line));
                save_process_log(
                    &conn,
                    &launch_info.command_name,
                    &launch_info.project_dir,
                    LOG_TYPE_STDOUT,
                    Some(&line),
                );
            }
            Ok(LineEvent::Stderr(line)) => {
                debug_log(&format!("[rust-log-collector] stderr: {}", line));
                save_process_log(
                    &conn,
                    &launch_info.command_name,
                    &launch_info.project_dir,
                    LOG_TYPE_STDERR,
                    Some(&line),
                );
            }
            Ok(LineEvent::Exit(code)) => {
                exited_during_grace = true;
                exit_code = code;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if exited_during_grace && exit_code.unwrap_or(-1) != 0 {
        debug_log(&format!(
            "[rust-log-collector] Process failed during grace period, pid={}, code={:?}",
            child_pid, exit_code
        ));
        save_process_log(
            &conn,
            &launch_info.command_name,
            &launch_info.project_dir,
            LOG_TYPE_PROCESS_START_FAILED,
            Some(&format!("Process failed to start: {:?}", exit_code)),
        );
        delete_process_entry(
            &conn,
            &launch_info.command_name,
            &launch_info.project_dir,
            child_pid,
        );
        return;
    }

    // Process started successfully
    debug_log(&format!(
        "[rust-log-collector] Process started, pid={}",
        child_pid
    ));
    save_process_log(
        &conn,
        &launch_info.command_name,
        &launch_info.project_dir,
        LOG_TYPE_PROCESS_STARTED,
        None,
    );

    // If already exited during grace with code 0, handle that
    if exited_during_grace {
        save_process_log(
            &conn,
            &launch_info.command_name,
            &launch_info.project_dir,
            LOG_TYPE_PROCESS_EXITED,
            Some(&format!("Process exited with code {:?}", exit_code)),
        );
        delete_process_entry(
            &conn,
            &launch_info.command_name,
            &launch_info.project_dir,
            child_pid,
        );
        return;
    }

    // Main loop: collect output until process exits
    let mut last_cleanup = SystemTime::now();

    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(LineEvent::Stdout(line)) => {
                save_process_log(
                    &conn,
                    &launch_info.command_name,
                    &launch_info.project_dir,
                    LOG_TYPE_STDOUT,
                    Some(&line),
                );
            }
            Ok(LineEvent::Stderr(line)) => {
                save_process_log(
                    &conn,
                    &launch_info.command_name,
                    &launch_info.project_dir,
                    LOG_TYPE_STDERR,
                    Some(&line),
                );
            }
            Ok(LineEvent::Exit(code)) => {
                exit_code = code;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        // Check for cleanup periodically
        if last_cleanup
            .elapsed()
            .unwrap_or(Duration::ZERO)
            .as_secs()
            >= 60
        {
            maybe_run_cleanup(&conn);
            last_cleanup = SystemTime::now();
        }
    }

    debug_log(&format!(
        "[rust-log-collector] Process exited, pid={}, code={:?}",
        child_pid, exit_code
    ));

    save_process_log(
        &conn,
        &launch_info.command_name,
        &launch_info.project_dir,
        LOG_TYPE_PROCESS_EXITED,
        Some(&format!("Process exited with code {:?}", exit_code)),
    );

    delete_process_entry(
        &conn,
        &launch_info.command_name,
        &launch_info.project_dir,
        child_pid,
    );

    // Wait for stdin thread to finish
    if let Some(handle) = stdin_handle {
        handle.join().ok();
    }
}
