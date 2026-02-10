//! Last command implementation.
//!
//! The `bivvy last` command shows last run information.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::LastArgs;
use crate::error::Result;
use crate::state::{ProjectId, RunStatus, StateStore};
use crate::ui::{format_duration, format_relative_time, UserInterface};

use super::dispatcher::{Command, CommandResult};
use super::display;

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

        ui.show_header("Last Run");

        ui.message(&format!(
            "Date: {} ({})",
            last_run.timestamp.format("%Y-%m-%d %H:%M:%S"),
            format_relative_time(last_run.timestamp)
        ));
        ui.message(&format!("Workflow: {}", last_run.workflow));
        ui.message(&format!(
            "Duration: {}",
            format_duration(Duration::from_millis(last_run.duration_ms))
        ));

        match last_run.status {
            RunStatus::Success => ui.success("Status: Success"),
            RunStatus::Failed => ui.error("Status: Failed"),
            RunStatus::Interrupted => ui.warning("Status: Interrupted"),
        };

        if !last_run.steps_run.is_empty() {
            ui.message("");
            ui.message("Steps executed:");
            let mut seen_statuses = std::collections::HashSet::new();
            for step_name in &last_run.steps_run {
                let status = state
                    .get_step(step_name)
                    .map(|s| &s.status)
                    .copied()
                    .unwrap_or(crate::state::StepStatus::NeverRun);
                seen_statuses.insert(display::status_key(&status));
                display::show_step_status(ui, step_name, &status);
            }

            if let Some(legend) = display::format_legend(&seen_statuses) {
                ui.message("");
                ui.message(&legend);
            }
        }

        if !last_run.steps_skipped.is_empty() {
            ui.message("");
            ui.message(&format!("Skipped: {}", last_run.steps_skipped.join(", ")));
        }

        if let Some(ref error) = last_run.error {
            ui.message("");
            ui.error(&format!("Error: {}", error));
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
    fn last_shows_legend_for_steps() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let mut state = StateStore::load(&project_id).unwrap();

        // Record a run with a step
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
        assert!(ui.has_message("Legend:"));
    }

    #[test]
    fn last_untracked_step_shows_dash_status() {
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

        // Should show [pending] for steps with no recorded status
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("[pending] unknown_step")));
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
}
