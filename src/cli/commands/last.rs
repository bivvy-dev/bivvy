//! Last command implementation.
//!
//! The `bivvy last` command shows last run information.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::LastArgs;
use crate::error::Result;
use crate::state::{ProjectId, RunStatus, StateStore, StepStatus};
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

impl Command for LastCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        let project_id = ProjectId::from_path(&self.project_root)?;
        let state = StateStore::load(&project_id)?;

        let last_run = match state.last_run_record() {
            Some(r) => r,
            None => {
                ui.message("No runs recorded for this project.");
                return Ok(CommandResult::success());
            }
        };

        let theme = BivvyTheme::new();

        // Header
        ui.message(&format!(
            "\n  {} {}\n",
            theme.header.apply_to("⛺"),
            theme.highlight.apply_to("Last Run"),
        ));

        // Key-value metadata
        ui.message(&format!(
            "  {}  {}",
            theme.key.apply_to("Workflow:"),
            last_run.workflow,
        ));

        ui.message(&format!(
            "  {}      {} {}",
            theme.key.apply_to("When:"),
            format_relative_time(last_run.timestamp),
            theme.dim.apply_to(format!(
                "({})",
                last_run.timestamp.format("%Y-%m-%d %H:%M:%S")
            )),
        ));

        ui.message(&format!(
            "  {}  {}",
            theme.key.apply_to("Duration:"),
            theme
                .duration
                .apply_to(format_duration(Duration::from_millis(last_run.duration_ms))),
        ));

        let status_kind = StatusKind::from(last_run.status);
        let status_label = match last_run.status {
            RunStatus::Success => "Success",
            RunStatus::Failed => "Failed",
            RunStatus::Interrupted => "Interrupted",
        };
        ui.message(&format!(
            "  {}    {} {}",
            theme.key.apply_to("Status:"),
            status_kind.styled(&theme),
            status_label,
        ));

        // Steps section
        if !last_run.steps_run.is_empty() || !last_run.steps_skipped.is_empty() {
            ui.message("");
            ui.message(&format!("  {}", theme.key.apply_to("Steps:")));

            for step_name in &last_run.steps_run {
                let status = state
                    .get_step(step_name)
                    .map(|s| s.status)
                    .unwrap_or(StepStatus::NeverRun);
                let kind = StatusKind::from(status);

                let duration_info = state
                    .get_step(step_name)
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
                    kind.styled(&theme),
                    step_name,
                    duration_info,
                ));
            }

            for step_name in &last_run.steps_skipped {
                ui.message(&format!(
                    "    {} {:<20} {}",
                    StatusKind::Skipped.styled(&theme),
                    step_name,
                    theme.dim.apply_to("skipped"),
                ));
            }
        }

        // Error detail
        if let Some(ref error) = last_run.error {
            ui.message("");
            ui.error(&format!("  Error: {}", error));
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
    fn last_shows_header() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("⛺")));
        assert!(ui.messages().iter().any(|m| m.contains("Last Run")));
    }

    #[test]
    fn last_shows_workflow_and_status() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("Workflow:")));
        assert!(ui.messages().iter().any(|m| m.contains("default")));
        assert!(ui.messages().iter().any(|m| m.contains("Status:")));
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("✓") && m.contains("Success")));
    }

    #[test]
    fn last_shows_steps_for_recorded_run() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui.has_message("Steps:"));
        assert!(ui.messages().iter().any(|m| m.contains("setup")));
    }

    #[test]
    fn last_shows_skipped_steps() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("build");
        history.step_skipped("deploy");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("○") && m.contains("deploy") && m.contains("skipped")));
    }

    #[test]
    fn last_shows_when_and_duration() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("When:")));
        assert!(ui.messages().iter().any(|m| m.contains("Duration:")));
    }

    #[test]
    fn last_untracked_step_shows_pending_icon() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        // Record a run with a step name that won't have state tracking
        let mut history = RunHistoryBuilder::start("default");
        history.step_run("unknown_step");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show ◌ icon (pending) for steps with no recorded status
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("◌") && m.contains("unknown_step")));
        // Should not use old [?] indicator anywhere
        let all_output: Vec<&String> = ui
            .messages()
            .iter()
            .chain(ui.warnings().iter())
            .chain(ui.errors().iter())
            .chain(ui.successes().iter())
            .collect();
        assert!(!all_output.iter().any(|m| m.contains("[?]")));
    }

    #[test]
    fn last_shows_error_for_failed_run() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("build");
        let record = history.finish_failed("Build failed: missing dependency");
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs::default();
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("✗") && m.contains("Failed")));
        assert!(ui
            .errors()
            .iter()
            .any(|m| m.contains("Build failed: missing dependency")));
    }
}
