//! Last command implementation.
//!
//! The `bivvy last` command shows last run information.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::LastArgs;
use crate::error::{BivvyError, Result};
use crate::state::{ProjectId, RunRecord, RunStatus, StateStore, StepStatus};
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

    /// Display a single run record with styled output.
    fn display_run(
        &self,
        ui: &mut dyn UserInterface,
        run: &RunRecord,
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

        let status_kind = StatusKind::from(run.status);
        let status_label = match run.status {
            RunStatus::Success => "Success",
            RunStatus::Failed => "Failed",
            RunStatus::Interrupted => "Interrupted",
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

        // --all: show all runs instead of just the last one
        if self.args.all {
            let runs = &state.runs;

            if runs.is_empty() {
                ui.message("No runs recorded for this project.");
                return Ok(CommandResult::success());
            }

            // --json with --all: serialize all runs
            if self.args.json {
                let json =
                    serde_json::to_string_pretty(runs).map_err(|e| BivvyError::Other(e.into()))?;
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

        let last_run = match state.last_run_record() {
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
            let json =
                serde_json::to_string_pretty(last_run).map_err(|e| BivvyError::Other(e.into()))?;
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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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
        let (mut state, _) = StateStore::load(&project_id).unwrap();

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

    #[test]
    fn last_json_output() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs {
            json: true,
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should output valid JSON, not styled text
        let json_output = ui.messages().join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();
        assert_eq!(parsed["workflow"], "default");
        assert_eq!(parsed["status"], "Success");
        // Should NOT contain styled output
        assert!(!json_output.contains("⛺"));
    }

    #[test]
    fn last_step_filter_shows_matching_step() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        history.step_run("build");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs {
            step: Some("setup".to_string()),
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui.messages().iter().any(|m| m.contains("setup")));
        // "build" should not appear in the steps listing
        // (it may appear in the header area but not in the steps section)
        let steps_section: Vec<&String> = ui
            .messages()
            .iter()
            .filter(|m| m.contains("build") && !m.contains("Workflow"))
            .collect();
        assert!(steps_section.is_empty());
    }

    #[test]
    fn last_step_filter_error_for_unknown_step() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs {
            step: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert!(ui
            .errors()
            .iter()
            .any(|m| m.contains("nonexistent") && m.contains("not part of")));
    }

    #[test]
    fn last_all_shows_multiple_runs() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        // Record two runs
        let mut history1 = RunHistoryBuilder::start("default");
        history1.step_run("setup");
        let record1 = history1.finish_success();
        state.record_run(record1);

        let mut history2 = RunHistoryBuilder::start("deploy");
        history2.step_run("build");
        let record2 = history2.finish_success();
        state.record_run(record2);

        state.save(&project_id).unwrap();

        let args = LastArgs {
            all: true,
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        // Should show headers for both runs
        assert!(ui.messages().iter().any(|m| m.contains("Run 1 of 2")));
        assert!(ui.messages().iter().any(|m| m.contains("Run 2 of 2")));
    }

    #[test]
    fn last_all_no_runs() {
        let temp = TempDir::new().unwrap();
        let args = LastArgs {
            all: true,
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui.messages().iter().any(|m| m.contains("No runs recorded")));
    }

    #[test]
    fn last_all_json() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs {
            all: true,
            json: true,
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let json_output = ui.messages().join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();
        // Should be an array
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[test]
    fn last_output_flag_shows_note() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("setup");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs {
            output: true,
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("no captured output")));
    }

    #[test]
    fn last_step_filter_with_skipped_step() {
        let temp = TempDir::new().unwrap();
        let project_id = ProjectId::from_path(temp.path()).unwrap();
        let (mut state, _) = StateStore::load(&project_id).unwrap();

        let mut history = RunHistoryBuilder::start("default");
        history.step_run("build");
        history.step_skipped("deploy");
        let record = history.finish_success();
        state.record_run(record);
        state.save(&project_id).unwrap();

        let args = LastArgs {
            step: Some("deploy".to_string()),
            ..Default::default()
        };
        let cmd = LastCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("deploy") && m.contains("skipped")));
        // "build" should not appear in the steps section
        let build_in_steps: Vec<&String> = ui
            .messages()
            .iter()
            .filter(|m| m.contains("build") && !m.contains("Workflow"))
            .collect();
        assert!(build_in_steps.is_empty());
    }
}
