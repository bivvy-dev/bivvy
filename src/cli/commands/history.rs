//! History command implementation.
//!
//! The `bivvy history` command shows execution history by scanning
//! JSONL event log files for `WorkflowCompleted` events.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::HistoryArgs;
use crate::error::Result;
use crate::ui::theme::BivvyTheme;
use crate::ui::{format_duration, format_relative_time, OutputWriter, StatusKind, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The history command implementation.
pub struct HistoryCommand {
    project_root: PathBuf,
    args: HistoryArgs,
}

impl HistoryCommand {
    /// Create a new history command.
    pub fn new(project_root: &Path, args: HistoryArgs) -> Self {
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
    pub fn args(&self) -> &HistoryArgs {
        &self.args
    }
}

impl HistoryCommand {
    /// Parse a duration string like "1h", "7d", "30m" into a chrono Duration.
    fn parse_since(s: &str) -> Option<chrono::Duration> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        let (num_str, unit) = s.split_at(s.len() - 1);
        let num: i64 = num_str.parse().ok()?;

        match unit {
            "m" => Some(chrono::Duration::minutes(num)),
            "h" => Some(chrono::Duration::hours(num)),
            "d" => Some(chrono::Duration::days(num)),
            "w" => Some(chrono::Duration::weeks(num)),
            _ => None,
        }
    }

    /// Format a single run entry line with theme styling.
    fn format_run_line(run: &LogRunRecord, theme: &BivvyTheme) -> String {
        let kind = if run.success {
            StatusKind::Success
        } else {
            StatusKind::Failed
        };

        format!(
            "    {}  {:<18} {:<12} {} {}  {}",
            kind.styled(theme),
            theme.dim.apply_to(format_relative_time(run.timestamp)),
            run.workflow,
            run.steps_run,
            theme
                .dim
                .apply_to(if run.steps_run == 1 { "step" } else { "steps" }),
            theme
                .duration
                .apply_to(format_duration(Duration::from_millis(run.duration_ms))),
        )
    }

    /// Show detailed info for a run.
    fn show_run_detail(ui: &mut dyn OutputWriter, run: &LogRunRecord, theme: &BivvyTheme) {
        ui.message(&format!(
            "        {} steps run, {} skipped{}",
            run.steps_run,
            run.steps_skipped,
            if run.aborted { " (aborted)" } else { "" },
        ));
        if let Some(ref log_file) = run.log_file {
            ui.message(&format!(
                "        {} {}",
                theme.dim.apply_to("Log:"),
                theme.dim.apply_to(log_file),
            ));
        }
    }
}

/// A run record extracted from a JSONL event log.
#[derive(Debug, Clone, serde::Serialize)]
struct LogRunRecord {
    /// When the run occurred (from log file timestamp).
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Workflow name.
    workflow: String,
    /// Whether the workflow succeeded.
    success: bool,
    /// Whether the user aborted.
    aborted: bool,
    /// Number of steps that ran.
    steps_run: usize,
    /// Number of steps skipped.
    steps_skipped: usize,
    /// Total duration in milliseconds.
    duration_ms: u64,
    /// Log file name (for detail view).
    #[serde(skip)]
    log_file: Option<String>,
}

/// Scan a log directory for `WorkflowCompleted` events belonging to
/// `canonical_project`.
///
/// Returns records sorted by timestamp (most recent first). Logs from other
/// projects (or logs without a recognizable working directory) are skipped.
fn scan_log_dir(log_dir: &Path, limit: usize, canonical_project: &Path) -> Vec<LogRunRecord> {
    if !log_dir.exists() {
        return Vec::new();
    }

    // Collect and sort JSONL files by name (descending = most recent first)
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

    let mut records = Vec::new();
    for path in &files {
        if records.len() >= limit {
            break;
        }
        if !crate::logging::log_belongs_to_project(path, canonical_project) {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    if value.get("type").and_then(|t| t.as_str()) == Some("workflow_completed") {
                        let timestamp = value
                            .get("ts")
                            .and_then(|t| t.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|| {
                                // Fall back to parsing filename timestamp
                                parse_log_filename_timestamp(path).unwrap_or_else(chrono::Utc::now)
                            });

                        records.push(LogRunRecord {
                            timestamp,
                            workflow: value
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            success: value
                                .get("success")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            aborted: value
                                .get("aborted")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            steps_run: value.get("steps_run").and_then(|v| v.as_u64()).unwrap_or(0)
                                as usize,
                            steps_skipped: value
                                .get("steps_skipped")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as usize,
                            duration_ms: value
                                .get("duration_ms")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0),
                            log_file: path.file_name().and_then(|f| f.to_str()).map(String::from),
                        });
                    }
                }
            }
        }
    }

    records
}

/// Scan the default log directory, scoped to `canonical_project`.
fn scan_log_files(limit: usize, canonical_project: &Path) -> Vec<LogRunRecord> {
    scan_log_dir(&crate::logging::default_log_dir(), limit, canonical_project)
}

/// Parse a timestamp from a log filename like `2026-04-25T10-00-00_hash.jsonl`.
fn parse_log_filename_timestamp(path: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let stem = path.file_stem()?.to_str()?;
    // Format: YYYY-MM-DDTHH-MM-SS_suffix
    let ts_part = stem.split('_').next()?;
    // Convert back to RFC 3339: replace dashes in time part with colons
    let rfc3339 = format!("{}Z", ts_part.replacen('-', ":", 2));
    chrono::DateTime::parse_from_rfc3339(&rfc3339)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

impl Command for HistoryCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let limit = self.args.limit.unwrap_or(10);
        let project_id = crate::state::ProjectId::from_path(&self.project_root)?;
        let mut records = scan_log_files(limit.max(100), project_id.path()); // scan more, filter later

        // Apply --since filter
        if let Some(ref since_str) = self.args.since {
            if let Some(duration) = Self::parse_since(since_str) {
                let cutoff = chrono::Utc::now() - duration;
                records.retain(|r| r.timestamp >= cutoff);
            }
        }

        // Apply --step filter: not available from JSONL (WorkflowCompleted
        // only has counts, not step names). Show a note.
        if self.args.step.is_some() {
            ui.message("Note: --step filter is not yet supported with event log history.");
        }

        // Apply limit
        records.truncate(limit);

        if records.is_empty() {
            ui.message("No run history for this project.");
            return Ok(CommandResult::success());
        }

        // JSON output mode
        if self.args.json {
            let json = serde_json::to_string_pretty(&records)
                .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?;
            ui.message(&json);
            return Ok(CommandResult::success());
        }

        let theme = BivvyTheme::new();

        ui.message(&format!(
            "\n  {} {}\n",
            theme.header.apply_to("⛺"),
            theme.highlight.apply_to("Run History"),
        ));

        for run in &records {
            let line = Self::format_run_line(run, &theme);
            ui.message(&line);

            if self.args.detail {
                Self::show_run_detail(ui, run, &theme);
            }
        }

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use tempfile::TempDir;

    #[test]
    fn history_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = HistoryArgs::default();
        let cmd = HistoryCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn history_no_runs() {
        let temp = TempDir::new().unwrap();
        let args = HistoryArgs::default();
        let cmd = HistoryCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn history_with_limit() {
        let temp = TempDir::new().unwrap();
        let args = HistoryArgs {
            limit: Some(5),
            ..Default::default()
        };
        let cmd = HistoryCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn history_args_accessor() {
        let temp = TempDir::new().unwrap();
        let args = HistoryArgs {
            limit: Some(3),
            ..Default::default()
        };
        let cmd = HistoryCommand::new(temp.path(), args);

        assert_eq!(cmd.args().limit, Some(3));
    }

    #[test]
    fn parse_since_hours() {
        let d = HistoryCommand::parse_since("1h").unwrap();
        assert_eq!(d, chrono::Duration::hours(1));
    }

    #[test]
    fn parse_since_days() {
        let d = HistoryCommand::parse_since("7d").unwrap();
        assert_eq!(d, chrono::Duration::days(7));
    }

    #[test]
    fn parse_since_minutes() {
        let d = HistoryCommand::parse_since("30m").unwrap();
        assert_eq!(d, chrono::Duration::minutes(30));
    }

    #[test]
    fn parse_since_weeks() {
        let d = HistoryCommand::parse_since("2w").unwrap();
        assert_eq!(d, chrono::Duration::weeks(2));
    }

    #[test]
    fn parse_since_invalid() {
        assert!(HistoryCommand::parse_since("abc").is_none());
        assert!(HistoryCommand::parse_since("").is_none());
        assert!(HistoryCommand::parse_since("5x").is_none());
    }

    #[test]
    fn scan_log_files_with_jsonl_data() {
        // Create a temp log dir with a JSONL file containing a WorkflowCompleted event
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        let log_file = log_dir.join("2026-04-25T10-00-00_test.jsonl");
        let event_line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_test",
            "type": "workflow_completed",
            "name": "default",
            "success": true,
            "aborted": false,
            "steps_run": 3,
            "steps_skipped": 1,
            "duration_ms": 5000
        });
        std::fs::write(&log_file, format!("{}\n", event_line)).unwrap();

        // scan_log_files uses default_log_dir() which won't find our temp dir,
        // so we test the parsing logic directly
        let content = std::fs::read_to_string(&log_file).unwrap();
        let value: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(value["type"], "workflow_completed");
        assert_eq!(value["name"], "default");
        assert_eq!(value["success"], true);
        assert_eq!(value["steps_run"], 3);
    }

    #[test]
    fn log_run_record_serializes_to_json() {
        let record = LogRunRecord {
            timestamp: chrono::Utc::now(),
            workflow: "default".to_string(),
            success: true,
            aborted: false,
            steps_run: 3,
            steps_skipped: 1,
            duration_ms: 5000,
            log_file: Some("test.jsonl".to_string()),
        };

        let json = serde_json::to_string(&record).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["workflow"], "default");
        assert_eq!(value["success"], true);
        // log_file should be skipped in serialization
        assert!(value.get("log_file").is_none());
    }

    /// Helper: write a log file with `session_started` + `workflow_completed`.
    fn write_run_log(log_dir: &Path, name: &str, project: &Path, workflow: &str) {
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
            "success": true,
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
        );
        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-04_bbbb.jsonl",
            &canonical_b,
            "default",
        );
        write_run_log(
            log_dir.path(),
            "2026-04-28T22-26-05_cccc.jsonl",
            &canonical_a,
            "quick",
        );

        let records_a = scan_log_dir(log_dir.path(), 100, &canonical_a);
        assert_eq!(records_a.len(), 2);
        let workflows: Vec<&str> = records_a.iter().map(|r| r.workflow.as_str()).collect();
        assert!(workflows.contains(&"default"));
        assert!(workflows.contains(&"quick"));

        let records_b = scan_log_dir(log_dir.path(), 100, &canonical_b);
        assert_eq!(records_b.len(), 1);
    }

    #[test]
    fn scan_log_dir_respects_limit() {
        let project = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        for i in 0..5 {
            write_run_log(
                log_dir.path(),
                &format!("2026-04-28T22-26-0{}_aaaa.jsonl", i),
                &canonical,
                "default",
            );
        }

        let records = scan_log_dir(log_dir.path(), 3, &canonical);
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn scan_log_dir_skips_logs_without_working_directory() {
        let project = TempDir::new().unwrap();
        let canonical = project.path().canonicalize().unwrap();
        let log_dir = TempDir::new().unwrap();

        // Log without working_directory — must be excluded under project scoping.
        let event_line = serde_json::json!({
            "ts": "2026-04-25T10:00:00.000Z",
            "session": "sess_old",
            "type": "workflow_completed",
            "name": "default",
            "success": true,
            "aborted": false,
            "steps_run": 1,
            "steps_skipped": 0,
            "duration_ms": 1000
        });
        std::fs::write(
            log_dir.path().join("2026-04-28T22-26-00_old.jsonl"),
            format!("{}\n", event_line),
        )
        .unwrap();

        let records = scan_log_dir(log_dir.path(), 100, &canonical);
        assert!(records.is_empty());
    }
}
