//! `candle open-dashboard`: make sure the dashboard is running as a
//! Candle-managed service in the global namespace, then open it.

use std::thread;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::commands::list_ports::handle_list_ports;
use crate::commands::open_browser::open_service_port;
use crate::dirs::global_project_dir;
use crate::errors::CandleError;
use crate::process_alive::find_running_service;
use crate::start::start_one_service::{start_one_service, IfRunning, RunOptions};

/// Service name of the dashboard in the global namespace.
pub const DASHBOARD_SERVICE_NAME: &str = "dashboard";

/// How long to wait for a freshly started dashboard to start listening.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenDashboardOutput {
    pub url: String,
    pub pid: i64,
    /// False when an already-running dashboard was reused.
    pub started: bool,
    pub opened_browser: bool,
}

/// Quote a word for `sh -c`.
fn shell_quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+=:@%,".contains(c))
    {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// The service's shell command: this binary, run as `candle dashboard`.
fn dashboard_shell(exe: &str, port: Option<u16>) -> String {
    let mut shell = format!("{} dashboard", shell_quote(exe));
    if let Some(port) = port {
        shell.push_str(&format!(" --port {port}"));
    }
    shell
}

/// Reuse a running dashboard launched from this same binary, unless a
/// different port was asked for. Anything else (an upgraded binary, a new
/// port) replaces it.
fn can_reuse(running_shell: &str, exe: &str, port: Option<u16>) -> bool {
    match port {
        Some(_) => running_shell == dashboard_shell(exe, port),
        None => {
            let base = dashboard_shell(exe, None);
            running_shell == base || running_shell.starts_with(&format!("{base} --port "))
        }
    }
}

/// The lowest port the global dashboard service listens on.
fn dashboard_port(conn: &Connection, project_dir: &str) -> Result<Option<i64>, CandleError> {
    Ok(handle_list_ports(
        conn,
        std::path::Path::new(project_dir),
        true,
        &[DASHBOARD_SERVICE_NAME.to_string()],
    )?
    .ports
    .into_iter()
    .filter(|p| p.project_dir == project_dir)
    .map(|p| p.port)
    .min())
}

/// Start (or reuse) the global dashboard service and optionally open it.
pub fn handle_open_dashboard(
    conn: &Connection,
    port: Option<u16>,
    open_browser: bool,
) -> Result<OpenDashboardOutput, CandleError> {
    let project_dir = global_project_dir();
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| CandleError::Generic(format!("Could not create {project_dir}: {e}")))?;

    let exe = std::env::current_exe()
        .map_err(|e| CandleError::Generic(format!("Could not locate the candle binary: {e}")))?
        .display()
        .to_string();

    let running = find_running_service(conn, &project_dir, DASHBOARD_SERVICE_NAME)?;
    let reuse = running
        .as_ref()
        .is_some_and(|entry| can_reuse(entry.shell.as_deref().unwrap_or(""), &exe, port));

    if !reuse {
        // start_one_service prints the generic start banner; this command
        // reports in its own words instead.
        let (result, _) = crate::output::capture(|| {
            start_one_service(
                conn,
                RunOptions {
                    command_name: DASHBOARD_SERVICE_NAME.to_string(),
                    project_dir: project_dir.clone(),
                    shell: Some(dashboard_shell(&exe, port)),
                    root: None,
                    enable_stdin: false,
                    if_running: if running.is_some() {
                        IfRunning::Replace
                    } else {
                        IfRunning::Skip
                    },
                },
            )
        });
        result?;
    }

    let entry =
        find_running_service(conn, &project_dir, DASHBOARD_SERVICE_NAME)?.ok_or_else(|| {
            CandleError::Generic(format!(
                "The dashboard exited right after starting. See its logs with: \
                 candle logs --project-dir {} {DASHBOARD_SERVICE_NAME}",
                shell_quote(&project_dir)
            ))
        })?;

    // The server binds before printing anything, but the port shows up in the
    // OS socket table only once the process has gotten that far.
    let deadline = Instant::now() + LISTEN_TIMEOUT;
    let listening_port = loop {
        if let Some(port) = dashboard_port(conn, &project_dir)? {
            break port;
        }
        if Instant::now() >= deadline {
            return Err(CandleError::Generic(
                "The dashboard started but never began listening on a port.".to_string(),
            ));
        }
        thread::sleep(POLL_INTERVAL);
    };

    let url = if open_browser {
        open_service_port(
            conn,
            &project_dir,
            DASHBOARD_SERVICE_NAME,
            Some(listening_port),
        )?
        .url
    } else {
        format!("http://localhost:{listening_port}")
    };

    Ok(OpenDashboardOutput {
        url,
        pid: entry.pid,
        started: !reuse,
        opened_browser: open_browser,
    })
}

pub fn format_open_dashboard_output(out: &OpenDashboardOutput) -> String {
    let mut lines = vec![if out.started {
        format!(
            "Started the Candle dashboard at {} (pid {})",
            out.url, out.pid
        )
    } else {
        format!(
            "The Candle dashboard is running at {} (pid {})",
            out.url, out.pid
        )
    }];
    if out.opened_browser {
        lines.push(format!("Opened {} in browser", out.url));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(
            shell_quote("/usr/local/bin/candle"),
            "/usr/local/bin/candle"
        );
        assert_eq!(shell_quote("/My Apps/candle"), "'/My Apps/candle'");
        assert_eq!(shell_quote("/a'b/candle"), r"'/a'\''b/candle'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn builds_the_service_command() {
        assert_eq!(
            dashboard_shell("/bin/candle", None),
            "/bin/candle dashboard"
        );
        assert_eq!(
            dashboard_shell("/bin/candle", Some(8000)),
            "/bin/candle dashboard --port 8000"
        );
    }

    #[test]
    fn reuse_rules() {
        let exe = "/bin/candle";
        // No port requested: any port from the same binary will do.
        assert!(can_reuse("/bin/candle dashboard", exe, None));
        assert!(can_reuse("/bin/candle dashboard --port 8000", exe, None));
        // A specific port must match.
        assert!(can_reuse(
            "/bin/candle dashboard --port 8000",
            exe,
            Some(8000)
        ));
        assert!(!can_reuse(
            "/bin/candle dashboard --port 8000",
            exe,
            Some(9000)
        ));
        assert!(!can_reuse("/bin/candle dashboard", exe, Some(8000)));
        // A different binary (e.g. after an upgrade) is replaced.
        assert!(!can_reuse("/old/candle dashboard", exe, None));
    }
}
