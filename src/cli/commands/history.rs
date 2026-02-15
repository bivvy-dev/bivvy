//! History command implementation.
//!
//! The `bivvy history` command shows execution history.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::HistoryArgs;
use crate::error::Result;
use crate::state::{ProjectId, RunRecord, StateStore};
use crate::ui::theme::BivvyTheme;
use crate::ui::{format_duration, format_relative_time, StatusKind, UserInterface};

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
    fn format_run_line(run: &RunRecord, theme: &BivvyTheme) -> String {
        let kind = StatusKind::from(run.status);
        let step_count = run.steps_run.len();
        let step_label = if step_count == 1 { "step" } else { "steps" };

        format!(
            "    {}  {:<18} {:<12} {} {}  {}",
            kind.styled(theme),
            theme.dim.apply_to(format_relative_time(run.timestamp)),
            run.workflow,
            step_count,
            theme.dim.apply_to(step_label),
            theme
                .duration
                .apply_to(format_duration(Duration::from_millis(run.duration_ms))),
        )
    }

    /// Show detailed info for a run.
    fn show_run_detail(ui: &mut dyn UserInterface, run: &RunRecord, theme: &BivvyTheme) {
        if !run.steps_run.is_empty() {
            ui.message(&format!(
                "        {} {}",
                theme.dim.apply_to("Steps:"),
                theme.dim.apply_to(run.steps_run.join(", "))
            ));
        }
        if !run.steps_skipped.is_empty() {
            ui.message(&format!(
                "        {} {}",
                theme.dim.apply_to("Skipped:"),
                theme.dim.apply_to(run.steps_skipped.join(", "))
            ));
        }
        if let Some(ref error) = run.error {
            ui.error(&format!("        Error: {}", error));
        }
    }
}

impl Command for HistoryCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let project_id = ProjectId::from_path(&self.project_root)?;
        let state = StateStore::load(&project_id)?;

        let limit = self.args.limit.unwrap_or(10);
        let runs = state.run_history(limit);

        // Apply --since filter
        let since_cutoff = self.args.since.as_deref().and_then(Self::parse_since);
        let filtered_runs: Vec<&RunRecord> = if let Some(duration) = since_cutoff {
            let cutoff = chrono::Utc::now() - duration;
            runs.iter().filter(|r| r.timestamp >= cutoff).collect()
        } else {
            runs.iter().collect()
        };

        if filtered_runs.is_empty() {
            ui.message("No run history for this project.");
            return Ok(CommandResult::success());
        }

        let theme = BivvyTheme::new();

        ui.message(&format!(
            "\n  {} {}\n",
            theme.header.apply_to("⛺"),
            theme.highlight.apply_to("Run History"),
        ));

        for run in &filtered_runs {
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
    use crate::state::RunHistoryBuilder;
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
    fn history_detail_shows_steps() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        history.step_run("build");
        history.step_skipped("deploy");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = HistoryArgs {
            detail: true,
            ..Default::default()
        };
        let cmd = HistoryCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui.messages().iter().any(|m| m.contains("Steps:")));
        assert!(ui.messages().iter().any(|m| m.contains("setup")));
        assert!(ui.messages().iter().any(|m| m.contains("Skipped:")));
        assert!(ui.messages().iter().any(|m| m.contains("deploy")));
    }

    #[test]
    fn history_since_filters_runs() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        // Add a recent run
        let mut history = RunHistoryBuilder::start("default");
        history.step_run("step1");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        // Filter to last hour (should include the run)
        let args = HistoryArgs {
            since: Some("1h".to_string()),
            ..Default::default()
        };
        let cmd = HistoryCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should show the run (not "No run history")
        assert!(!ui.messages().iter().any(|m| m.contains("No run history")));
    }
}
