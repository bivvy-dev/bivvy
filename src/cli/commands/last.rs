//! Last command implementation.
//!
//! The `bivvy last` command shows information about the most recent run
//! by reading the most recent JSONL event log file.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::LastArgs;
use crate::error::Result;
use crate::state::{ProjectId, StateStore, StepStatus};
use crate::ui::theme::BivvyTheme;
use crate::ui::{format_duration, format_relative_time, StatusKind, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The last command implementation.
pub struct LastCommand {
    project_root: PathBuf,
    args: LastArgs,
}

impl LastCommand {
    /// Create a new last command.
    pub fn new(project_root: &Path, args: LastArgs) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            args,
        }
    }

    /// Get the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the command arguments.
    pub fn args(&self) -> &LastArgs {
        &self.args
    }
}

/// A run record reconstructed from JSONL event log data.
#[derive(Debug, Clone, serde::Serialize)]
struct LogLastRun {
    /// When the run occurred.
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Workflow name.
    workflow: String,
    /// Whether the workflow succeeded.
    success: bool,
    /// Whether the user aborted.
    aborted: bool,
    /// Number of steps that ran.
    steps_run_count: usize,
    /// Number of steps skipped.
    steps_skipped_count: usize,
    /// Total duration in milliseconds.
    duration_ms: u64,
    /// Step names that ran (from StepCompleted events).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    steps_run: Vec<String>,
    /// Step names that were skipped (from StepSkipped events).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    steps_skipped: Vec<String>,
    /// Error from failed steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Scan a log directory for runs belonging to `canonical_project`.
///
/// Iterates JSONL files newest-first (by filename) and returns runs
/// reconstructed from `WorkflowCompleted` events. Logs from other projects
/// (or logs without a recognizable working directory) are skipped.
///
/// When `all` is false, returns at most one run — the most recent one
/// containing a `WorkflowCompleted` event. The previous implementation
/// truncated the file list to a single entry up front, which caused
/// `bivvy last` to silently report "No runs recorded" whenever the
/// most recent log was from a non-`run` command (e.g. `bivvy status`).
fn scan_log_dir(log_dir: &Path, all: bool, canonical_project: &Path) -> Vec<LogLastRun> {
    if !log_dir.exists() {
        return Vec::new();
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(log_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    files.reverse();

    let mut runs = Vec::new();
    for path in &files {
        if !crate::logging::log_belongs_to_project(path, canonical_project) {
            continue;
        }
        if let Some(run) = parse_log_file(path) {
            runs.push(run);
            if !all {
                break;
            }
        }
    }

    runs
}

/// Read runs from the default log directory, scoped to `canonical_project`.
fn read_last_runs(all: bool, canonical_project: &Path) -> Vec<LogLastRun> {
    scan_log_dir(&crate::logging::default_log_dir(), all, canonical_project)
}

/// Parse a single JSONL log file into a run record.
///
/// Extracts `WorkflowCompleted`, `StepCompleted`, and `StepSkipped` events
/// to reconstruct a complete run record with step names.
fn parse_log_file(path: &Path) -> Option<LogLastRun> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut workflow_completed: Option<serde_json::Value> = None;
    let mut steps_run = Vec::new();
    let mut steps_skipped = Vec::new();
    let mut first_error: Option<String> = None;
    let mut session_timestamp: Option<chrono::DateTime<chrono::Utc>> = None;

    for line in content.lines() {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "session_started" => {
                if session_timestamp.is_none() {
                    session_timestamp = value
                        .get("ts")
                        .and_then(|t| t.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&chrono::Utc));
                }
            }
            "step_completed" => {
                if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                    steps_run.push(name.to_string());
                    // Capture first error
                    if first_error.is_none() {
                        if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
                            first_error = Some(err.to_string());
                        }
                    }
                }
            }
            "step_skipped" => {
                if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                    steps_skipped.push(name.to_string());
                }
            }
            "workflow_completed" => {
                workflow_completed = Some(value);
            }
            _ => {}
        }
    }

    let wc = workflow_completed?;

    let timestamp = wc
        .get("ts")
        .and_then(|t| t.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or(session_timestamp)
        .unwrap_or_else(chrono::Utc::now);

    let success = wc.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

    Some(LogLastRun {
        timestamp,
        workflow: wc
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        success,
        aborted: wc.get("aborted").and_then(|v| v.as_bool()).unwrap_or(false),
        steps_run_count: wc.get("steps_run").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        steps_skipped_count: wc
            .get("steps_skipped")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        duration_ms: wc.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        steps_run,
        steps_skipped,
        error: if success { None } else { first_error },
    })
}

impl LastCommand {
    /// Display a single run record with styled output.
    fn display_run(
        &self,
        ui: &mut dyn UserInterface,
        run: &LogLastRun,
        state: &StateStore,
        theme: &BivvyTheme,
        header_label: &str,
    ) {
        // Header
        ui.message(&format!(
            "\n  {} {}\n",
            theme.header.apply_to("⛺"),
            theme.highlight.apply_to(header_label),
        ));

        // Key-value metadata
        ui.message(&format!(
            "  {}  {}",
            theme.key.apply_to("Workflow:"),
            run.workflow,
        ));

        ui.message(&format!(
            "  {}      {} {}",
            theme.key.apply_to("When:"),
            format_relative_time(run.timestamp),
            theme
                .dim
                .apply_to(format!("({})", run.timestamp.format("%Y-%m-%d %H:%M:%S"))),
        ));

        ui.message(&format!(
            "  {}  {}",
            theme.key.apply_to("Duration:"),
            theme
                .duration
                .apply_to(format_duration(Duration::from_millis(run.duration_ms))),
        ));

        let status_kind = if run.success {
            StatusKind::Success
        } else {
            StatusKind::Failed
        };
        let status_label = if run.success {
            "Success"
        } else if run.aborted {
            "Interrupted"
        } else {
            "Failed"
        };
        ui.message(&format!(
            "  {}    {} {}",
            theme.key.apply_to("Status:"),
            status_kind.styled(theme),
            status_label,
        ));

        // Steps section - apply --step filter if provided
        let step_filter = self.args.step.as_deref();

        let steps_run: Vec<&String> = if let Some(filter) = step_filter {
            run.steps_run
                .iter()
                .filter(|s| s.as_str() == filter)
                .collect()
        } else {
            run.steps_run.iter().collect()
        };

        let steps_skipped: Vec<&String> = if let Some(filter) = step_filter {
            run.steps_skipped
                .iter()
                .filter(|s| s.as_str() == filter)
                .collect()
        } else {
            run.steps_skipped.iter().collect()
        };

        if !steps_run.is_empty() || !steps_skipped.is_empty() {
            ui.message("");
            ui.message(&format!("  {}", theme.key.apply_to("Steps:")));

            for step_name in &steps_run {
                let status = state
                    .get_step(step_name.as_str())
                    .map(|s| s.status)
                    .unwrap_or(StepStatus::NeverRun);
                let kind = StatusKind::from(status);

                let duration_info = state
                    .get_step(step_name.as_str())
                    .and_then(|s| s.duration_ms)
                    .map(|ms| {
                        theme
                            .duration
                            .apply_to(format_duration(Duration::from_millis(ms)))
                            .to_string()
                    })
                    .unwrap_or_default();

                ui.message(&format!(
                    "    {} {:<20} {}",
                    kind.styled(theme),
                    step_name,
                    duration_info,
                ));

                // --output: show captured output if available
                if self.args.output {
                    ui.message(&format!(
                        "      {}",
                        theme
                            .dim
                            .apply_to("(no captured output available in run history)")
                    ));
                }
            }

            for step_name in &steps_skipped {
                ui.message(&format!(
                    "    {} {:<20} {}",
                    StatusKind::Skipped.styled(theme),
                    step_name,
                    theme.dim.apply_to("skipped"),
                ));
            }
        }

        // Error detail
        if let Some(ref error) = run.error {
            ui.message("");
            ui.error(&format!("  Error: {}", error));
        }
    }
}

impl Command for LastCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let project_id = ProjectId::from_path(&self.project_root)?;
        let (state, _) = StateStore::load(&project_id)?;
        let canonical_project = project_id.path();

        // --all: show all runs instead of just the last one
        if self.args.all {
            let runs = read_last_runs(true, canonical_project);

            if runs.is_empty() {
                ui.message("No runs recorded for this project.");
                return Ok(CommandResult::success());
            }

            // --json with --all: serialize all runs
            if self.args.json {
                let json = serde_json::to_string_pretty(&runs)
                    .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?;
                ui.message(&json);
                return Ok(CommandResult::success());
            }

            let theme = BivvyTheme::new();
            for (i, run) in runs.iter().enumerate() {
                let label = format!("Run {} of {}", i + 1, runs.len());
                self.display_run(ui, run, &state, &theme, &label);
            }

            return Ok(CommandResult::success());
        }

        let runs = read_last_runs(false, canonical_project);
        let last_run = match runs.first() {
            Some(r) => r,
            None => {
                ui.message("No runs recorded for this project.");
                return Ok(CommandResult::success());
            }
        };

        // --step: validate that the step exists in the run
        if let Some(ref step_name) = self.args.step {
            let in_run = last_run.steps_run.contains(step_name);
            let in_skipped = last_run.steps_skipped.contains(step_name);
            if !in_run && !in_skipped {
                ui.error(&format!(
                    "Step '{}' was not part of the last run.",
                    step_name
                ));
                return Ok(CommandResult::failure(1));
            }
        }

        // --json: serialize run data as JSON
        if self.args.json {
            let json = serde_json::to_string_pretty(last_run)
                .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?;
            ui.message(&json);
            return Ok(CommandResult::success());
        }

        let theme = BivvyTheme::new();
        self.display_run(ui, last_run, &state, &theme, "Last Run");

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use tempfile::TempDir;

    #[test]
    fn last_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn last_no_runs() {
        let temp = TempDir::new().unwrap();
        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn last_args_accessor() {
        let temp = TempDir::new().unwrap();
        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);

        // Just ensure it doesn't panic
        let _ = cmd.args();
    }

    #[test]
    fn parse_log_file_extracts_workflow_completed() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("test.jsonl");

        let session_line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": ["default"],
            "version": "1.9.0"
        });
        let step_completed = serde_json::json!({
            "ts": "2026-04-25T10:00:01.000Z",
            "session": "sess_test",
            "type": "step_completed",
            "name": "setup",
            "success": true,
            "exit_code": 0,
            "duration_ms": 500
        });
        let step_skipped = serde_json::json!({
            "ts": "2026-04-25T10:00:01.000Z",
            "session": "sess_test",
            "type": "step_skipped",
            "name": "deploy",
            "reason": "check passed"
        });
        let wc = serde_json::json!({
            "ts": "2026-04-25T10:00:02.000Z",
            "session": "sess_test",
            "type": "workflow_completed",
            "name": "default",
            "success": true,
            "aborted": false,
            "steps_run": 1,
            "steps_skipped": 1,
            "duration_ms": 2000
        });

        let content = format!(
            "{}\n{}\n{}\n{}\n",
            session_line, step_completed, step_skipped, wc
        );
        std::fs::write(&log_file, content).unwrap();

        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.workflow, "default");
        assert!(run.success);
        assert!(!run.aborted);
        assert_eq!(run.steps_run_count, 1);
        assert_eq!(run.steps_skipped_count, 1);
        assert_eq!(run.duration_ms, 2000);
        assert_eq!(run.steps_run, vec!["setup"]);
        assert_eq!(run.steps_skipped, vec!["deploy"]);
        assert!(run.error.is_none());
    }

    #[test]
    fn parse_log_file_captures_error() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("test_fail.jsonl");

        let step_failed = serde_json::json!({
            "ts": "2026-04-25T10:00:01.000Z",
            "session": "sess_test",
            "type": "step_completed",
            "name": "build",
            "success": false,
            "exit_code": 1,
            "duration_ms": 500,
            "error": "Build failed: missing dependency"
        });
        let wc = serde_json::json!({
            "ts": "2026-04-25T10:00:02.000Z",
            "session": "sess_test",
            "type": "workflow_completed",
            "name": "default",
            "success": false,
            "aborted": false,
            "steps_run": 1,
            "steps_skipped": 0,
            "duration_ms": 1000
        });

        let content = format!("{}\n{}\n", step_failed, wc);
        std::fs::write(&log_file, content).unwrap();

        let run = parse_log_file(&log_file).unwrap();
        assert!(!run.success);
        assert_eq!(
            run.error,
            Some("Build failed: missing dependency".to_string())
        );
    }

    #[test]
    fn parse_log_file_returns_none_without_workflow_completed() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("no_wc.jsonl");

        let session = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "lint",
            "args": [],
            "version": "1.9.0"
        });

        std::fs::write(&log_file, format!("{}\n", session)).unwrap();

        assert!(parse_log_file(&log_file).is_none());
    }

    #[test]
    fn log_last_run_serializes_to_json() {
        let run = LogLastRun {
            timestamp: chrono::Utc::now(),
            workflow: "default".to_string(),
            success: true,
            aborted: false,
            steps_run_count: 2,
            steps_skipped_count: 0,
            duration_ms: 3000,
            steps_run: vec!["setup".to_string(), "build".to_string()],
            steps_skipped: vec![],
            error: None,
        };

        let json = serde_json::to_string_pretty(&run).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["workflow"], "default");
        assert_eq!(parsed["success"], true);
    }

    /// Helper: write a `session_started` + `workflow_completed` log file for `project`.
    fn write_run_log(log_dir: &Path, name: &str, project: &Path, workflow: &str, success: bool) {
        let session_started = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [workflow],
            "version": "1.9.0",
            "working_directory": project.display().to_string()
        });
        let workflow_completed = serde_json::json!({
            "ts": "2026-04-25T10:00:02.000Z",
            "session": "sess_test",
            "type": "workflow_completed",
            "name": workflow,
            "success": success,
            "aborted": false,
            "steps_run": 1,
            "steps_skipped": 0,
            "duration_ms": 1000
        });
        std::fs::write(
            log_dir.join(name),
            format!("{}\n{}\n", session_started, workflow_completed),
        )
        .unwrap();
    }

    /// Helper: write a status-style log (session_started only, no workflow_completed).
    fn write_status_log(log_dir: &Path, name: &str, project: &Path) {
        let session = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "status",
            "args": [],
            "version": "1.9.0",
            "working_directory": project.display().to_string()
        });
        std::fs::write(log_dir.join(name), format!("{}\n", session)).unwrap();
    }

    #[test]
    fn scan_log_dir_skips_logs_without_workflow_completed() {
        // Reproduces the bug: when the most recent log (by filename) is from
        // a non-`run` command like `bivvy status`, `bivvy last` should still
        // surface the most recent actual run rather than reporting "no runs".
        let project = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-03_aaaaaaaa.jsonl",
            &canonical,
            "default",
            true,
        );
        write_status_log(
            log_dir.path(),
            "2026-04-28T22-26-03_status.jsonl",
            &canonical,
        );

        let runs = scan_log_dir(log_dir.path(), false, &canonical);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].workflow, "default");
        assert!(runs[0].success);
    }

    #[test]
    fn scan_log_dir_filters_other_projects() {
        let project_a = TempDir::new().unwrap();
        let project_b = TempDir::new().unwrap();
        let canonical_a = project_a.path().canonicalize().unwrap();
        let canonical_b = project_b.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-03_aaaa.jsonl",
            &canonical_a,
            "default",
            true,
        );
        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-04_bbbb.jsonl",
            &canonical_b,
            "default",
            true,
        );

        let runs_a = scan_log_dir(log_dir.path(), true, &canonical_a);
        assert_eq!(runs_a.len(), 1);

        let runs_b = scan_log_dir(log_dir.path(), true, &canonical_b);
        assert_eq!(runs_b.len(), 1);
    }

    #[test]
    fn scan_log_dir_returns_empty_when_no_logs_match_project() {
        let project = TempDir::new().unwrap();
        let other = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();
        let canonical_other = other.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-03_aaaa.jsonl",
            &canonical_other,
            "default",
            true,
        );

        let runs = scan_log_dir(log_dir.path(), false, &canonical);
        assert!(runs.is_empty());
    }

    #[test]
    fn scan_log_dir_with_all_returns_runs_newest_first() {
        let project = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-01_aaaa.jsonl",
            &canonical,
            "first",
            true,
        );
        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-05_bbbb.jsonl",
            &canonical,
            "second",
            true,
        );

        let runs = scan_log_dir(log_dir.path(), true, &canonical);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].workflow, "second");
        assert_eq!(runs[1].workflow, "first");
    }
}
