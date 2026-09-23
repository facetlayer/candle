//! `find-orphans` command — report tracked processes whose project has gone
//! away underneath them.
//!
//! Candle keys every `processes` row on the project directory it was started
//! from. Nothing stops that project from being deleted, reconfigured, or having
//! the service removed from its config while the process keeps running, and
//! when that happens the row becomes unreachable from the project it belongs to:
//! `candle list` in the old directory can no longer find it, because there is no
//! longer a directory to run `candle list` in.
//!
//! This is a rarely-used diagnostic in the same family as `kill-all` — it looks
//! across every project rather than the current one. The companion cleanup is
//! `candle kill --project-dir <dir>`, which accepts a directory that no longer
//! exists.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::config::{find_service_by_name, read_config_file, CONFIG_FILENAMES};
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

/// Classify one running process against the project it claims to belong to.
///
/// Returns `None` when the project still vouches for the service. A config file
/// that exists but cannot be read or parsed is *not* treated as orphaning: the
/// service is very likely still configured, and reporting it as an orphan would
/// invite killing a healthy process over a typo in the JSON.
fn classify(project_dir: &str, service_name: &str) -> Option<OrphanReason> {
    let dir = Path::new(project_dir);
    if !dir.is_dir() {
        return Some(OrphanReason::MissingProjectDir);
    }

    // Only the project directory itself counts. A config file in some ancestor
    // describes a different project, not this one.
    let config_path = CONFIG_FILENAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists());

    let Some(config_path) = config_path else {
        return Some(OrphanReason::MissingConfigFile);
    };

    // A config that will not parse is left alone — see the doc comment.
    let config = read_config_file(&config_path).ok()?;

    match find_service_by_name(&config, service_name) {
        Some(_) => None,
        None => Some(OrphanReason::ServiceNotInConfig),
    }
}

/// Find every live tracked process whose project no longer accounts for it.
///
/// Only processes that are actually alive are considered — a dead row is stale
/// bookkeeping for the reaper to clear, not an orphan anyone needs to kill.
pub fn handle_find_orphans(conn: &Connection) -> Result<FindOrphansOutput, CandleError> {
    let running = find_all_running_processes(conn)?;
    let alive = filter_alive_processes(conn, running)?;

    let orphans = alive
        .into_iter()
        .filter_map(|entry| {
            classify(&entry.project_dir, &entry.command_name).map(|reason| OrphanedProcess {
                service_name: entry.command_name,
                project_dir: entry.project_dir,
                pid: entry.pid,
                reason,
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

    // The cleanup is not obvious — `kill` normally works off the CWD, and these
    // projects may have no directory left to cd into.
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
        let reason = classify("/definitely/not/a/real/directory", "svc");
        assert_eq!(reason, Some(OrphanReason::MissingProjectDir));
    }

    #[test]
    fn directory_without_config_is_an_orphan() {
        let dir = TempDir::new();
        let reason = classify(&dir.path().display().to_string(), "svc");
        assert_eq!(reason, Some(OrphanReason::MissingConfigFile));
    }

    #[test]
    fn config_without_the_service_is_an_orphan() {
        let dir = TempDir::new();
        write_config(dir.path(), r#"{"name": "other", "shell": "true"}"#);

        let reason = classify(&dir.path().display().to_string(), "svc");
        assert_eq!(reason, Some(OrphanReason::ServiceNotInConfig));
    }

    #[test]
    fn configured_service_is_not_an_orphan() {
        let dir = TempDir::new();
        write_config(dir.path(), r#"{"name": "svc", "shell": "true"}"#);

        assert_eq!(classify(&dir.path().display().to_string(), "svc"), None);
    }

    #[test]
    fn ancestor_config_does_not_rescue_a_child_project() {
        // The row names <parent>/child as its project. A config in <parent>
        // describes the parent project, so it must not count for the child.
        let parent = TempDir::new();
        write_config(parent.path(), r#"{"name": "svc", "shell": "true"}"#);
        let child = parent.path().join("child");
        std::fs::create_dir_all(&child).unwrap();

        assert_eq!(
            classify(&child.display().to_string(), "svc"),
            Some(OrphanReason::MissingConfigFile)
        );
    }

    #[test]
    fn unreadable_config_is_not_reported_as_an_orphan() {
        // A malformed config almost certainly still lists the service; calling it
        // an orphan would invite killing a healthy process over a JSON typo.
        let dir = TempDir::new();
        std::fs::write(dir.path().join(".candle.json"), "{ not json").unwrap();

        assert_eq!(classify(&dir.path().display().to_string(), "svc"), None);
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
