//! Starting a single service.
//!
//! Ports `startOneService` from `src/start/startOneService.ts`. The flow:
//! check-start dedup → resolve the service config (transient or from file) →
//! kill any existing instance → seed a log cursor → record
//! `process_start_initiated` → launch the sidecar → race the log table against a
//! 10s timeout for `process_started` / `process_start_failed` → print the banner.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::config::model::ServiceConfig;
use crate::config::{get_service_config_by_name, is_valid_root_path};
use crate::db::process_table::find_processes_by_command_name_and_project_dir;
use crate::dirs::candle_db_path;
use crate::errors::CandleError;
use crate::kill::handle_kill_command;
use crate::monitor::MonitorLaunchInfo;
use crate::logs::process_logs::save_process_log;
use crate::logs::{LogIterator, ProcessLogType};
use crate::output;
use crate::process_alive::{filter_alive_processes, is_process_alive};
use crate::start::launch::launch_monitor;

/// How long the CLI watches the log table for a start result before giving up.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll interval while watching the log table (matches Node's `setTimeout(100)`).
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// How long to wait for a killed previous instance to finish exiting before
/// recording the new launch. Bounded so a service that ignores SIGTERM can't
/// block a start indefinitely.
const PREVIOUS_INSTANCE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
/// Poll interval while draining the previous instance.
const DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Options for [`start_one_service`], mirroring the relevant fields of Node's
/// `RunOptions`.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub command_name: String,
    pub project_dir: String,
    pub shell: Option<String>,
    pub root: Option<String>,
    pub enable_stdin: bool,
    pub check_start: bool,
}

/// Result of a successful (or skipped) start. Mirrors `StartResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartResult {
    pub project_dir: String,
    pub service_name: String,
}

fn db_err(e: rusqlite::Error) -> CandleError {
    CandleError::Generic(format!("database error: {e}"))
}

/// PIDs belonging to the currently-running instance of `command_name`: the
/// supervised shell and the monitor sidecar that writes its log rows. Both must
/// be gone before the old instance can be considered fully drained.
fn previous_instance_pids(
    conn: &Connection,
    project_dir: &str,
    command_name: &str,
) -> Result<Vec<i64>, CandleError> {
    let entries = find_processes_by_command_name_and_project_dir(conn, command_name, project_dir)
        .map_err(db_err)?;

    let mut pids = Vec::new();
    for entry in entries.iter().filter(|e| e.killed_at.is_none()) {
        if entry.pid > 0 {
            pids.push(entry.pid);
        }
        if let Some(collector_pid) = entry.log_collector_pid {
            if collector_pid > 0 {
                pids.push(collector_pid);
            }
        }
    }

    Ok(pids)
}

/// Block until none of `pids` is alive, or until `timeout` elapses. Returns
/// whether every PID exited within the timeout.
fn wait_for_pids_to_exit(pids: &[i64], timeout: Duration) -> bool {
    if pids.is_empty() {
        return true;
    }

    let deadline = Instant::now() + timeout;
    loop {
        if !pids.iter().copied().any(is_process_alive) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(DRAIN_POLL_INTERVAL);
    }
}

/// Launch a single service as a detached subprocess and wait for it to report a
/// start result. See module docs for the full sequence.
pub fn start_one_service(conn: &Connection, opts: RunOptions) -> Result<StartResult, CandleError> {
    // 1. check-start dedup — runs BEFORE config resolution so it works for
    //    transient names that aren't in the config file.
    if opts.check_start {
        if opts.command_name.is_empty() {
            return Err(CandleError::UsageError("Command name is required".to_string()));
        }
        let existing = find_processes_by_command_name_and_project_dir(
            conn,
            &opts.command_name,
            &opts.project_dir,
        )
        .map_err(db_err)?;
        // Only entries with no killed_at AND a live PID count as running.
        // `filter_alive_processes` also deletes the dead rows it finds.
        let not_killed: Vec<_> = existing
            .into_iter()
            .filter(|p| p.killed_at.is_none())
            .collect();
        let running = filter_alive_processes(conn, not_killed).map_err(db_err)?;
        if !running.is_empty() {
            output::out(&format!(
                "[Service '{}' is already running]",
                opts.command_name
            ));
            return Ok(StartResult {
                project_dir: opts.project_dir,
                service_name: opts.command_name,
            });
        }
    }

    // 2. Resolve the service config.
    let service: ServiceConfig = if let Some(shell) = &opts.shell {
        // Transient process.
        if opts.command_name.is_empty() {
            return Err(CandleError::UsageError("Command name is required".to_string()));
        }
        if let Some(root) = &opts.root {
            if !is_valid_root_path(root) {
                return Err(CandleError::UsageError(format!(
                    "Invalid root path: \"{root}\". Root must be an absolute path or a relative path within the project."
                )));
            }
        }
        ServiceConfig {
            name: opts.command_name.clone(),
            shell: shell.clone(),
            root: opts.root.clone(),
            enable_stdin: Some(opts.enable_stdin),
        }
    } else {
        // Configured process.
        let found =
            get_service_config_by_name(&opts.command_name, Some(Path::new(&opts.project_dir)))?;
        found.service_config
    };

    // 3. Kill any existing instance (start == restart). quiet_failure suppresses
    //    "no running processes" noise.
    let previous_pids = previous_instance_pids(conn, &opts.project_dir, &service.name)?;

    handle_kill_command(
        conn,
        &opts.project_dir,
        std::slice::from_ref(&service.name),
        true,
        false,
    )
    .map_err(db_err)?;

    // `kill_process_tree` only fires SIGTERM; it does not wait. If we recorded the
    // new launch while the old instance was still shutting down, the old shell's
    // dying output and its monitor's `process_exited` row would be written to the
    // log table *after* the new `process_start_initiated` row — and log consumers,
    // which treat that row as the launch boundary, would replay them as if they
    // belonged to the new instance. Wait for the old shell and its monitor to be
    // gone first so every stale row lands before the boundary.
    wait_for_pids_to_exit(&previous_pids, PREVIOUS_INSTANCE_DRAIN_TIMEOUT);

    // 4. Seed the log watch position, then record process_start_initiated.
    let mut log_iterator = LogIterator::new(opts.project_dir.clone(), vec![service.name.clone()]);
    log_iterator
        .reset_to_latest_log_message(conn)
        .map_err(db_err)?;
    let mut initial_log_position = log_iterator.copy();

    save_process_log(
        conn,
        &service.name,
        &opts.project_dir,
        ProcessLogType::ProcessStartInitiated,
        None,
    )
    .map_err(db_err)?;

    // 5. Launch the detached monitor process (`candle --monitor`).
    let info = MonitorLaunchInfo {
        command_name: service.name.clone(),
        project_dir: opts.project_dir.clone(),
        shell: service.shell.clone(),
        root: service.root.clone(),
        enable_stdin: service.enable_stdin.unwrap_or(false),
        database_path: candle_db_path(),
    };
    launch_monitor(&info)
        .map_err(|e| CandleError::Generic(format!("Failed to launch monitor process: {e}")))?;

    // 6. Success / failure race against a 10s timeout.
    let deadline = Instant::now() + START_TIMEOUT;
    let mut started = false;
    'watch: loop {
        let logs = log_iterator.get_next_logs(conn, None).map_err(db_err)?;
        for log in &logs {
            if log.log_type == ProcessLogType::ProcessStarted.as_i64() {
                started = true;
                break 'watch;
            }
            if log.log_type == ProcessLogType::ProcessStartFailed.as_i64() {
                let recent = initial_log_position.get_next_logs(conn, None).map_err(db_err)?;
                let recent_logs = recent
                    .iter()
                    .map(|l| l.content.clone().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(CandleError::ProcessStartFailed {
                    command_name: service.name.clone(),
                    recent_logs,
                });
            }
        }

        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(POLL_INTERVAL);
    }

    if !started {
        return Err(CandleError::Generic(
            "Process failed to start (timed out while waiting)".to_string(),
        ));
    }

    // 7. Success banner. Note the absolute-root special-case here diverges from
    //    the sidecar's cwd (which joins unconditionally) — preserved from Node.
    //    Shared with `list` so the two always report the same directory.
    let launch_dir = crate::dirs::resolve_launch_dir(&opts.project_dir, service.root.as_deref());

    output::out(&format!(
        "[Started process '{}'] $ {}",
        service.name, service.shell
    ));
    output::out(&format!("[With root directory: {launch_dir}]"));

    Ok(StartResult {
        project_dir: opts.project_dir,
        service_name: service.name,
    })
}
