//! Last command implementation.
//!
//! The `bivvy last` command shows information about the most recent run
//! by reading the most recent JSONL event log file.
//!
//! # Reading step outcomes
//!
//! Per-step outcomes are read from `step_outcome` events emitted by the
//! runner. Each step produces exactly one such event, with a typed
//! [`StepOutcomeKind`] that matches the visual state shown by `bivvy run`.
//! The older fine-grained events (`step_completed`, `step_skipped`,
//! `step_filtered_out`, `step_decided`, `dependency_blocked`) are still
//! emitted for consumers that need timing or decision-reasoning detail,
//! but `bivvy last` ignores them — `step_outcome` is the single source
//! of truth here.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::LastArgs;
use crate::error::Result;
use crate::logging::StepOutcomeKind;
use crate::state::ProjectId;
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

/// A single step's terminal state, reconstructed from a `step_outcome` event.
#[derive(Debug, Clone, serde::Serialize)]
struct LoggedStep {
    /// Step name.
    name: String,
    /// Outcome variant as the snake_case JSON tag (`completed`, `failed`,
    /// `satisfied`, `declined`, `filtered_out`, `blocked`).
    outcome: String,
    /// Human-readable detail (skip reason, error message, block reason).
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    /// Execution duration in milliseconds, only present for steps that ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

impl LoggedStep {
    fn outcome_kind(&self) -> Option<StepOutcomeKind> {
        use std::str::FromStr;
        StepOutcomeKind::from_str(&self.outcome).ok()
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
    /// Number of steps that ran (from `WorkflowCompleted.steps_run`).
    steps_run_count: usize,
    /// Number of steps skipped (from `WorkflowCompleted.steps_skipped`).
    steps_skipped_count: usize,
    /// Total duration in milliseconds.
    duration_ms: u64,
    /// Typed terminal outcomes, in emission order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    steps: Vec<LoggedStep>,
    /// Error from the first failed step.
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
/// Reads `step_outcome` events into a typed step list and the
/// `workflow_completed` event for run-level summary fields. Returns
/// `None` if the file has no `workflow_completed` event — that filters
/// out non-`run` sessions like `bivvy status`.
fn parse_log_file(path: &Path) -> Option<LogLastRun> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut workflow_completed: Option<serde_json::Value> = None;
    let mut steps: Vec<LoggedStep> = Vec::new();
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
            "step_outcome" => {
                let name = match value.get("name").and_then(|n| n.as_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let outcome = match value.get("outcome").and_then(|o| o.as_str()) {
                    Some(o) => o.to_string(),
                    None => continue,
                };
                let detail = value
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                let duration_ms = value.get("duration_ms").and_then(|d| d.as_u64());

                if first_error.is_none() && outcome == StepOutcomeKind::Failed.as_str() {
                    first_error = detail.clone();
                }

                steps.push(LoggedStep {
                    name,
                    outcome,
                    detail,
                    duration_ms,
                });
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
        steps,
        error: if success { None } else { first_error },
    })
}

/// Map a [`StepOutcomeKind`] to the [`StatusKind`] used to render its glyph
/// in `bivvy last`. Matches what `bivvy run` displays for each outcome.
fn outcome_status_kind(kind: StepOutcomeKind) -> StatusKind {
    match kind {
        StepOutcomeKind::Completed | StepOutcomeKind::Satisfied => StatusKind::Success,
        StepOutcomeKind::Failed => StatusKind::Failed,
        StepOutcomeKind::Declined | StepOutcomeKind::FilteredOut => StatusKind::Skipped,
        StepOutcomeKind::Blocked => StatusKind::Blocked,
    }
}

/// Default human-readable label for each outcome when no detail is recorded.
fn outcome_default_label(kind: StepOutcomeKind) -> &'static str {
    match kind {
        StepOutcomeKind::Completed => "completed",
        StepOutcomeKind::Failed => "failed",
        StepOutcomeKind::Satisfied => "already satisfied",
        StepOutcomeKind::Declined => "skipped",
        StepOutcomeKind::FilteredOut => "filtered out",
        StepOutcomeKind::Blocked => "blocked",
    }
}

impl LastCommand {
    /// Display a single run record with styled output.
    fn display_run(
        &self,
        ui: &mut dyn UserInterface,
        run: &LogLastRun,
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

        // Steps section — apply --step filter if provided
        let step_filter = self.args.step.as_deref();
        let steps: Vec<&LoggedStep> = if let Some(filter) = step_filter {
            run.steps.iter().filter(|s| s.name == filter).collect()
        } else {
            run.steps.iter().collect()
        };

        if !steps.is_empty() {
            ui.message("");
            ui.message(&format!("  {}", theme.key.apply_to("Steps:")));

            for step in &steps {
                let kind = step.outcome_kind();
                let status_kind = kind.map(outcome_status_kind).unwrap_or(StatusKind::Pending);
                let label = step.detail.clone().unwrap_or_else(|| {
                    kind.map(outcome_default_label)
                        .unwrap_or(step.outcome.as_str())
                        .to_string()
                });
                let duration_info = step
                    .duration_ms
                    .map(|ms| {
                        theme
                            .duration
                            .apply_to(format_duration(Duration::from_millis(ms)))
                            .to_string()
                    })
                    .unwrap_or_default();

                ui.message(&format!(
                    "    {} {:<20} {} {}",
                    status_kind.styled(theme),
                    step.name,
                    theme.dim.apply_to(&label),
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
                self.display_run(ui, run, &theme, &label);
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
            let in_run = last_run.steps.iter().any(|s| s.name == *step_name);
            if !in_run {
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
        self.display_run(ui, last_run, &theme, "Last Run");

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

    /// Build a `step_outcome` JSON value with the given fields.
    fn outcome_event(
        name: &str,
        outcome: StepOutcomeKind,
        detail: Option<&str>,
        duration_ms: Option<u64>,
    ) -> serde_json::Value {
        let mut v = serde_json::json!({
            "ts": "2026-04-25T10:00:01.000Z",
            "session": "sess_test",
            "type": "step_outcome",
            "name": name,
            "outcome": outcome.as_str(),
        });
        if let Some(d) = detail {
            v["detail"] = serde_json::Value::String(d.to_string());
        }
        if let Some(ms) = duration_ms {
            v["duration_ms"] = serde_json::Value::Number(ms.into());
        }
        v
    }

    fn workflow_completed(success: bool, run: u64, skipped: u64) -> serde_json::Value {
        serde_json::json!({
            "ts": "2026-04-25T10:00:02.000Z",
            "session": "sess_test",
            "type": "workflow_completed",
            "name": "default",
            "success": success,
            "aborted": false,
            "steps_run": run,
            "steps_skipped": skipped,
            "duration_ms": 2000
        })
    }

    fn write_log(path: &Path, lines: &[serde_json::Value]) {
        let mut content = String::new();
        for v in lines {
            content.push_str(&v.to_string());
            content.push('\n');
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn parse_log_reads_workflow_completed_summary() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("test.jsonl");

        let session = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": ["default"],
            "version": "1.10.0"
        });
        write_log(
            &log_file,
            &[
                session,
                outcome_event("setup", StepOutcomeKind::Completed, None, Some(500)),
                workflow_completed(true, 1, 0),
            ],
        );

        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.workflow, "default");
        assert!(run.success);
        assert!(!run.aborted);
        assert_eq!(run.steps_run_count, 1);
        assert_eq!(run.duration_ms, 2000);
        assert_eq!(run.steps.len(), 1);
        assert_eq!(run.steps[0].name, "setup");
        assert_eq!(run.steps[0].outcome, "completed");
        assert!(run.error.is_none());
    }

    #[test]
    fn parse_log_returns_none_without_workflow_completed() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("no_wc.jsonl");

        let session = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "lint",
            "args": [],
            "version": "1.10.0"
        });

        std::fs::write(&log_file, format!("{}\n", session)).unwrap();

        assert!(parse_log_file(&log_file).is_none());
    }

    /// Per-variant round trip: every StepOutcomeKind written to the log
    /// is parsed back into a typed step entry preserving outcome, detail,
    /// and duration_ms.
    #[test]
    fn parse_log_round_trips_completed() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("c.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event("build", StepOutcomeKind::Completed, None, Some(1234)),
                workflow_completed(true, 1, 0),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(
            run.steps[0].outcome_kind(),
            Some(StepOutcomeKind::Completed)
        );
        assert_eq!(run.steps[0].duration_ms, Some(1234));
    }

    #[test]
    fn parse_log_round_trips_failed_and_captures_error() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("f.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event(
                    "build",
                    StepOutcomeKind::Failed,
                    Some("exit code 1"),
                    Some(500),
                ),
                workflow_completed(false, 1, 0),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.steps[0].outcome_kind(), Some(StepOutcomeKind::Failed));
        assert_eq!(run.error.as_deref(), Some("exit code 1"));
    }

    #[test]
    fn parse_log_round_trips_satisfied() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("s.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event(
                    "rust",
                    StepOutcomeKind::Satisfied,
                    Some("✓ rustc --version succeeded"),
                    None,
                ),
                workflow_completed(true, 0, 1),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(
            run.steps[0].outcome_kind(),
            Some(StepOutcomeKind::Satisfied)
        );
        assert_eq!(
            run.steps[0].detail.as_deref(),
            Some("✓ rustc --version succeeded")
        );
    }

    #[test]
    fn parse_log_round_trips_declined() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("d.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event(
                    "deploy",
                    StepOutcomeKind::Declined,
                    Some("user_declined"),
                    None,
                ),
                workflow_completed(true, 0, 1),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.steps[0].outcome_kind(), Some(StepOutcomeKind::Declined));
    }

    #[test]
    fn parse_log_round_trips_filtered_out() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("fo.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event(
                    "version-bump",
                    StepOutcomeKind::FilteredOut,
                    Some("skip_flag"),
                    None,
                ),
                workflow_completed(true, 0, 1),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(
            run.steps[0].outcome_kind(),
            Some(StepOutcomeKind::FilteredOut)
        );
    }

    #[test]
    fn parse_log_round_trips_blocked() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("b.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event(
                    "release-tag",
                    StepOutcomeKind::Blocked,
                    Some("Blocked (dependency 'build' failed)"),
                    None,
                ),
                workflow_completed(false, 0, 0),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.steps[0].outcome_kind(), Some(StepOutcomeKind::Blocked));
    }

    #[test]
    fn parse_log_preserves_step_order() {
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("order.jsonl");
        write_log(
            &log_file,
            &[
                outcome_event("a", StepOutcomeKind::Completed, None, Some(10)),
                outcome_event("b", StepOutcomeKind::Satisfied, None, None),
                outcome_event("c", StepOutcomeKind::Blocked, None, None),
                workflow_completed(false, 1, 1),
            ],
        );
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.steps.len(), 3);
        assert_eq!(run.steps[0].name, "a");
        assert_eq!(run.steps[1].name, "b");
        assert_eq!(run.steps[2].name, "c");
    }

    #[test]
    fn parse_log_skips_unknown_outcome_strings() {
        // A future runner adding a new variant should not crash older bivvy
        // last installs — unknown outcomes are kept as raw strings on the
        // step and outcome_kind() returns None.
        let temp = TempDir::new().unwrap();
        let log_file = temp.path().join("unknown.jsonl");
        let event = serde_json::json!({
            "ts": "2026-04-25T10:00:01.000Z",
            "session": "sess_test",
            "type": "step_outcome",
            "name": "x",
            "outcome": "future_kind"
        });
        write_log(&log_file, &[event, workflow_completed(true, 0, 0)]);
        let run = parse_log_file(&log_file).unwrap();
        assert_eq!(run.steps[0].outcome, "future_kind");
        assert_eq!(run.steps[0].outcome_kind(), None);
    }

    #[test]
    fn outcome_status_kind_matches_run_ui() {
        // bivvy run uses these StatusKind glyphs for each outcome — bivvy
        // last must match.
        assert_eq!(
            outcome_status_kind(StepOutcomeKind::Completed),
            StatusKind::Success
        );
        assert_eq!(
            outcome_status_kind(StepOutcomeKind::Failed),
            StatusKind::Failed
        );
        assert_eq!(
            outcome_status_kind(StepOutcomeKind::Satisfied),
            StatusKind::Success
        );
        assert_eq!(
            outcome_status_kind(StepOutcomeKind::Declined),
            StatusKind::Skipped
        );
        assert_eq!(
            outcome_status_kind(StepOutcomeKind::FilteredOut),
            StatusKind::Skipped
        );
        assert_eq!(
            outcome_status_kind(StepOutcomeKind::Blocked),
            StatusKind::Blocked
        );
    }

    /// Helper: write a `session_started` + `workflow_completed` log file for `project`.
    fn write_run_log(log_dir: &Path, name: &str, project: &Path, workflow: &str, success: bool) {
        let session_started = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "run",
            "args": [workflow],
            "version": "1.10.0",
            "working_directory": project.display().to_string()
        });
        let wc = serde_json::json!({
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
        std::fs::write(log_dir.join(name), format!("{}\n{}\n", session_started, wc)).unwrap();
    }

    /// Helper: write a status-style log (session_started only, no workflow_completed).
    fn write_status_log(log_dir: &Path, name: &str, project: &Path) {
        let session = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "session_started",
            "command": "status",
            "args": [],
            "version": "1.10.0",
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
