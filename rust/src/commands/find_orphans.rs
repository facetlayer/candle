//! Find live services whose project directory, config, or service entry is gone.
//! Cleanup uses `candle kill --project-dir <dir>`, even for deleted directories.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::config::{find_service_by_name, read_config_file, CONFIG_FILENAME};
use crate::db::process_table::find_all_running_processes;
use crate::errors::CandleError;
use crate::process_alive::filter_alive_processes;

/// Why a running process counts as orphaned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrphanReason {
    /// The project directory no longer exists on disk.
    MissingProjectDir,
    /// The directory is still there, but holds no candle config file.
    MissingConfigFile,
    /// The config file is there, but no longer defines this service.
    ServiceNotInConfig,
}

impl OrphanReason {
    /// One-line explanation, shown in the report.
    pub fn describe(self) -> &'static str {
        match self {
            OrphanReason::MissingProjectDir => "project directory no longer exists",
            OrphanReason::MissingConfigFile => "no candle config file in project directory",
            OrphanReason::ServiceNotInConfig => "service is no longer listed in the config file",
        }
    }
}

/// One orphaned process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanedProcess {
    pub service_name: String,
    pub project_dir: String,
    pub pid: i64,
    pub reason: OrphanReason,
}

/// Result of [`handle_find_orphans`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindOrphansOutput {
    pub orphans: Vec<OrphanedProcess>,
}

/// Classify a service against its project. Unreadable or invalid configs do
/// not establish orphanhood. Transient services need no config entry.
fn classify(project_dir: &str, service_name: &str, transient: bool) -> Option<OrphanReason> {
    let dir = Path::new(project_dir);
    if !dir.is_dir() {
        return Some(OrphanReason::MissingProjectDir);
    }

    // An ancestor config belongs to another project.
    let config_path = dir.join(CONFIG_FILENAME);
    if !config_path.exists() {
        return Some(OrphanReason::MissingConfigFile);
    }

    if transient {
        return None;
    }

    let config = read_config_file(&config_path).ok()?;

    match find_service_by_name(&config, service_name) {
        Some(_) => None,
        None => Some(OrphanReason::ServiceNotInConfig),
    }
}

/// Find live orphaned services; leave dead rows for stale cleanup.
pub fn handle_find_orphans(conn: &Connection) -> Result<FindOrphansOutput, CandleError> {
    let running = find_all_running_processes(conn)?;
    let alive = filter_alive_processes(conn, running)?;

    let orphans = alive
        .into_iter()
        .filter_map(|entry| {
            classify(&entry.project_dir, &entry.command_name, entry.transient).map(|reason| {
                OrphanedProcess {
                    service_name: entry.command_name,
                    project_dir: entry.project_dir,
                    pid: entry.pid,
                    reason,
                }
            })
        })
        .collect();

    Ok(FindOrphansOutput { orphans })
}

/// Render the report for humans.
pub fn format_find_orphans(output: &FindOrphansOutput) -> String {
    if output.orphans.is_empty() {
        return "No orphaned processes found".to_string();
    }

    let mut lines = vec![format!(
        "Found {} orphaned process{}:",
        output.orphans.len(),
        if output.orphans.len() == 1 { "" } else { "es" }
    )];

    for orphan in &output.orphans {
        lines.push(String::new());
        lines.push(format!("{} (PID {})", orphan.service_name, orphan.pid));
        lines.push(format!("  Project:  {}", orphan.project_dir));
        lines.push(format!("  Orphaned: {}", orphan.reason.describe()));
    }

    lines.push(String::new());
    lines.push("Clean up with: candle kill --project-dir <project> <service>".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempDir;

    fn write_config(dir: &Path, services: &str) {
        std::fs::write(
            dir.join(".candle.json"),
            format!("{{\"services\": [{services}]}}"),
        )
        .unwrap();
    }

    #[test]
    fn missing_project_dir_is_an_orphan() {
        let reason = classify("/definitely/not/a/real/directory", "svc", false);
        assert_eq!(reason, Some(OrphanReason::MissingProjectDir));
    }

    #[test]
    fn directory_without_config_is_an_orphan() {
        let dir = TempDir::new();
        let reason = classify(&dir.path().display().to_string(), "svc", false);
        assert_eq!(reason, Some(OrphanReason::MissingConfigFile));
    }

    #[test]
    fn config_without_the_service_is_an_orphan() {
        let dir = TempDir::new();
        write_config(dir.path(), r#"{"name": "other", "shell": "true"}"#);

        let reason = classify(&dir.path().display().to_string(), "svc", false);
        assert_eq!(reason, Some(OrphanReason::ServiceNotInConfig));
    }

    #[test]
    fn transient_process_missing_from_config_is_not_an_orphan() {
        let dir = TempDir::new();
        write_config(dir.path(), r#"{"name": "other", "shell": "true"}"#);

        assert_eq!(
            classify(&dir.path().display().to_string(), "svc", true),
            None
        );
    }

    #[test]
    fn transient_process_without_its_project_is_an_orphan() {
        let dir = TempDir::new();
        assert_eq!(
            classify(&dir.path().display().to_string(), "svc", true),
            Some(OrphanReason::MissingConfigFile)
        );
        assert_eq!(
            classify("/definitely/not/a/real/directory", "svc", true),
            Some(OrphanReason::MissingProjectDir)
        );
    }

    #[test]
    fn configured_service_is_not_an_orphan() {
        let dir = TempDir::new();
        write_config(dir.path(), r#"{"name": "svc", "shell": "true"}"#);

        assert_eq!(
            classify(&dir.path().display().to_string(), "svc", false),
            None
        );
    }

    #[test]
    fn ancestor_config_does_not_rescue_a_child_project() {
        // An ancestor config cannot vouch for the child project.
        let parent = TempDir::new();
        write_config(parent.path(), r#"{"name": "svc", "shell": "true"}"#);
        let child = parent.path().join("child");
        std::fs::create_dir_all(&child).unwrap();

        assert_eq!(
            classify(&child.display().to_string(), "svc", false),
            Some(OrphanReason::MissingConfigFile)
        );
    }

    #[test]
    fn unreadable_config_is_not_reported_as_an_orphan() {
        // Invalid JSON does not establish that the service was removed.
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{ not json").unwrap();

        assert_eq!(
            classify(&dir.path().display().to_string(), "svc", false),
            None
        );
    }

    #[test]
    fn empty_report_says_so() {
        let output = FindOrphansOutput { orphans: vec![] };
        assert_eq!(format_find_orphans(&output), "No orphaned processes found");
    }

    #[test]
    fn report_lists_each_orphan_with_its_reason() {
        let output = FindOrphansOutput {
            orphans: vec![OrphanedProcess {
                service_name: "api".to_string(),
                project_dir: "/gone/project".to_string(),
                pid: 4242,
                reason: OrphanReason::MissingProjectDir,
            }],
        };

        let text = format_find_orphans(&output);
        assert!(text.contains("Found 1 orphaned process:"));
        assert!(text.contains("api (PID 4242)"));
        assert!(text.contains("/gone/project"));
        assert!(text.contains("project directory no longer exists"));
        assert!(text.contains("candle kill --project-dir"));
    }

    #[test]
    fn report_pluralizes_multiple_orphans() {
        let orphan = OrphanedProcess {
            service_name: "api".to_string(),
            project_dir: "/gone".to_string(),
            pid: 1,
            reason: OrphanReason::MissingConfigFile,
        };
        let output = FindOrphansOutput {
            orphans: vec![orphan.clone(), orphan],
        };
        assert!(format_find_orphans(&output).contains("Found 2 orphaned processes:"));
    }
}
