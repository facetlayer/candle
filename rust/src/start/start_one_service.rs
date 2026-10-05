//! Resolve and launch one service, then await its run's startup result.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::commands::list::{format_entry_uptime, has_config_drift, latest_run, LatestRun};
use crate::config::model::ServiceConfig;
use crate::config::{get_service_config_by_name, is_valid_root_path};
use crate::db::process_table::ProcessEntry;
use crate::dirs::candle_db_path;
use crate::errors::CandleError;
use crate::kill::handle_kill_command;
use crate::logs::process_logs::{
    get_process_logs, get_process_logs_with_eviction_info, printable_log_types, save_run_log,
    start_run, LogSearchOptions,
};
use crate::logs::ProcessLogType;
use crate::monitor::MonitorLaunchInfo;
use crate::output;
use crate::process_alive::{find_running_service, is_service_running};
use crate::start::launch::launch_monitor;

/// How long the CLI watches the log table for a start result before giving up.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll interval while watching the log table.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Most log lines a failed start prints; the rest are in `candle logs`.
const FAILED_START_LOG_LINES: i64 = 20;

/// What [`start_one_service`] does when the service is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfRunning {
    /// Leave it running and report that (`start`).
    Skip,
    /// Kill it and launch a new instance (`restart`).
    Replace,
}

/// Options for [`start_one_service`].
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub command_name: String,
    pub project_dir: String,
    pub shell: Option<String>,
    pub root: Option<String>,
    pub enable_stdin: bool,
    pub if_running: IfRunning,
}

/// Result of a successful (or skipped) start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartResult {
    pub project_dir: String,
    pub service_name: String,
}

/// (device, inode) of the file at the connection's database path, or `None`
/// for an in-memory database or a path that no longer exists.
fn database_file_identity(conn: &Connection) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let path = conn.path().filter(|p| !p.is_empty())?;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.dev(), meta.ino()))
}

/// Record pre-monitor failures for list/logs. Skip running instances so a new
/// launch marker does not hide their output.
fn record_start_failure_if_idle(
    conn: &Connection,
    project_dir: &str,
    service_name: &str,
    reason: &str,
) -> Result<(), CandleError> {
    if is_service_running(conn, project_dir, service_name)? {
        return Ok(());
    }
    let run_id = start_run(conn, service_name, project_dir)?;
    save_run_log(
        conn,
        Some(run_id),
        service_name,
        project_dir,
        ProcessLogType::ProcessStartFailed,
        Some(&format!("Process failed to start: {reason}")),
    )?;
    Ok(())
}

/// Leave an existing service running; reject a different transient shell.
fn report_already_running(
    opts: &RunOptions,
    entry: &ProcessEntry,
) -> Result<StartResult, CandleError> {
    let name = &opts.command_name;
    let expected = match &opts.shell {
        Some(shell) => Some(ServiceConfig {
            name: name.clone(),
            shell: shell.clone(),
            root: opts.root.clone(),
            enable_stdin: None,
        }),
        // A running transient that isn't in the config has nothing to compare.
        None => get_service_config_by_name(name, Some(Path::new(&opts.project_dir)))
            .ok()
            .map(|found| found.service_config),
    };
    let drifted = has_config_drift(entry, expected.as_ref());
    let status = format!("pid {}, up {}", entry.pid, format_entry_uptime(entry));

    if drifted && opts.shell.is_some() {
        return Err(CandleError::UsageError(format!(
            "Service '{name}' is already running with a different command: $ {}\n\
             To replace it, run 'candle restart {name} --shell <cmd>'.",
            entry.shell.as_deref().unwrap_or("")
        )));
    }
    if drifted {
        output::out(&format!(
            "[Service '{name}' is already running ({status}) with an outdated command; \
             use 'candle restart {name}' to apply the .candle.json changes]"
        ));
    } else {
        output::out(&format!(
            "[Service '{name}' is already running ({status}); use 'candle restart {name}' to restart it]"
        ));
    }
    Ok(StartResult {
        project_dir: opts.project_dir.clone(),
        service_name: name.clone(),
    })
}

/// Launch a detached service and await its startup result.
pub fn start_one_service(conn: &Connection, opts: RunOptions) -> Result<StartResult, CandleError> {
    // Hold the lock through startup to prevent concurrent duplicate launches.
    let db_identity = database_file_identity(conn);
    let _start_lock = crate::start::service_lock::acquire(&opts.project_dir, &opts.command_name)
        .map_err(|e| CandleError::Generic(format!("Failed to acquire start lock: {e}")))?;
    // A connection opened before erase-database may point to a deleted file.
    if db_identity.is_some() && database_file_identity(conn) != db_identity {
        return Err(CandleError::Generic(
            "The database was erased while this start was waiting. Run the command again."
                .to_string(),
        ));
    }

    if opts.command_name.is_empty() && opts.shell.is_some() {
        return Err(CandleError::UsageError(
            "Command name is required".to_string(),
        ));
    }

    // Check before config lookup so unconfigured transient names still work.
    if opts.if_running == IfRunning::Skip {
        if let Some(entry) = find_running_service(conn, &opts.project_dir, &opts.command_name)? {
            return report_already_running(&opts, &entry);
        }
    }

    let service: ServiceConfig = if let Some(shell) = &opts.shell {
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
        let found =
            get_service_config_by_name(&opts.command_name, Some(Path::new(&opts.project_dir)))?;
        found.service_config
    };

    // Validate cwd before killing an existing service, and name the missing path
    // rather than reporting an ambiguous spawn error.
    let launch_dir = crate::dirs::resolve_launch_dir(&opts.project_dir, service.root.as_deref());
    if !Path::new(&launch_dir).is_dir() {
        let reason = format!("root directory does not exist: {launch_dir}");
        record_start_failure_if_idle(conn, &opts.project_dir, &service.name, &reason)?;
        return Err(CandleError::UsageError(format!(
            "Process '{}' failed to start: {reason}",
            service.name
        )));
    }

    // Capture the previous failure before this launch becomes the latest run.
    let previous_run = latest_run(conn, &opts.project_dir, &service.name)?;

    // Wait for the old tree. Its monitor may still write rows with the old run id.
    handle_kill_command(
        conn,
        &opts.project_dir,
        std::slice::from_ref(&service.name),
        true,
        false,
    )?;

    let run_id = start_run(conn, &service.name, &opts.project_dir)?;

    let info = MonitorLaunchInfo {
        command_name: service.name.clone(),
        project_dir: opts.project_dir.clone(),
        shell: service.shell.clone(),
        root: service.root.clone(),
        enable_stdin: service.enable_stdin.unwrap_or(false),
        database_path: candle_db_path(),
        run_id: Some(run_id),
        transient: opts.shell.is_some(),
    };
    launch_monitor(&info)
        .map_err(|e| CandleError::Generic(format!("Failed to launch monitor process: {e}")))?;

    let this_run = |log_types: Vec<i64>| {
        get_process_logs(
            conn,
            &LogSearchOptions {
                project_dir: Some(opts.project_dir.clone()),
                command_names: vec![service.name.clone()],
                run_id: Some(run_id),
                log_types,
                ..Default::default()
            },
        )
    };
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        let outcome = this_run(vec![
            ProcessLogType::ProcessStarted.as_i64(),
            ProcessLogType::ProcessStartFailed.as_i64(),
        ])?;
        match outcome.first() {
            Some(log) if log.log_type == ProcessLogType::ProcessStarted.as_i64() => break,
            Some(_) => {
                // The monitor writes output before the failure marker; show the tail in order.
                let tail = get_process_logs_with_eviction_info(
                    conn,
                    &LogSearchOptions {
                        project_dir: Some(opts.project_dir.clone()),
                        command_names: vec![service.name.clone()],
                        run_id: Some(run_id),
                        log_types: printable_log_types(),
                        limit: Some(FAILED_START_LOG_LINES),
                        ..Default::default()
                    },
                )?;
                let recent_logs = tail
                    .logs
                    .into_iter()
                    .filter_map(|l| l.content)
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(CandleError::ProcessStartFailed {
                    command_name: service.name.clone(),
                    recent_logs,
                    truncated: tail.logs_were_evicted,
                });
            }
            None if Instant::now() >= deadline => {
                return Err(CandleError::Generic(format!(
                    "Process '{}' failed to start (timed out while waiting)",
                    service.name
                )));
            }
            None => thread::sleep(POLL_INTERVAL),
        }
    }

    output::out(&format!(
        "[Started process '{}'] $ {}",
        service.name, service.shell
    ));
    output::out(&format!("[With root directory: {launch_dir}]"));
    let previous_outcome = match previous_run {
        LatestRun::Exited(code) => Some(format!("exited with code {code}")),
        LatestRun::Failed => Some("failed".to_string()),
        LatestRun::Unremarkable => None,
    };
    if let Some(outcome) = previous_outcome {
        output::out(&format!(
            "[The previous run {outcome}; see 'candle logs {} --previous']",
            service.name
        ));
    }

    Ok(StartResult {
        project_dir: opts.project_dir,
        service_name: service.name,
    })
}
