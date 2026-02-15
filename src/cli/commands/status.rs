//! Status command implementation.
//!
//! The `bivvy status` command shows current setup status.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::StatusArgs;
use crate::config::load_merged_config;
use crate::error::{BivvyError, Result};
use crate::state::{ProjectId, StateStore, StepStatus};
use crate::ui::theme::BivvyTheme;
use crate::ui::{format_relative_time, hints, OutputMode, StatusKind, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The status command implementation.
pub struct StatusCommand {
    project_root: PathBuf,
    args: StatusArgs,
}

impl StatusCommand {
    /// Create a new status command.
    pub fn new(project_root: &Path, args: StatusArgs) -> Self {
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
    pub fn args(&self) -> &StatusArgs {
        &self.args
    }
}

impl Command for StatusCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Load configuration
        let config = match load_merged_config(&self.project_root) {
            Ok(c) => c,
            Err(BivvyError::ConfigNotFound { .. }) => {
                ui.error("No configuration found. Run 'bivvy init' first.");
                return Ok(CommandResult::failure(2));
            }
            Err(e) => return Err(e),
        };

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.default_output.into());
        }

        // Get project identity
        let project_id = ProjectId::from_path(&self.project_root)?;

        // Load state
        let state = StateStore::load(&project_id)?;

        let theme = BivvyTheme::new();

        // Show header: ⛺ AppName — Status
        let app_name = config.app_name.as_deref().unwrap_or("Bivvy Setup");
        ui.message(&format!(
            "\n  {} {} {} {}\n",
            theme.header.apply_to("⛺"),
            theme.highlight.apply_to(app_name),
            theme.dim.apply_to("—"),
            theme.dim.apply_to("Status"),
        ));

        // Show last run info with relative time
        if let Some(last_run) = state.last_run_record() {
            ui.message(&format!(
                "  {} {} {} {}",
                theme.key.apply_to("Last run:"),
                theme.dim.apply_to(format_relative_time(last_run.timestamp)),
                theme.dim.apply_to("·"),
                theme
                    .dim
                    .apply_to(format!("{} workflow", last_run.workflow)),
            ));
            ui.message("");
        }

        // Show step status
        ui.message(&format!("  {}", theme.key.apply_to("Steps:")));

        let step_names: Vec<&String> = if let Some(ref step_name) = self.args.step {
            if config.steps.contains_key(step_name) {
                vec![step_name]
            } else {
                ui.error(&format!("Unknown step: {}", step_name));
                return Ok(CommandResult::failure(1));
            }
        } else {
            config.steps.keys().collect()
        };

        for step_name in &step_names {
            let step_state = state.get_step(step_name);
            let status = step_state.map(|s| s.status).unwrap_or(StepStatus::NeverRun);
            let kind = StatusKind::from(status);

            // Build the right-side info (duration or relative time)
            let right_side = step_state
                .and_then(|s| {
                    if status == StepStatus::NeverRun {
                        return None;
                    }
                    // Show duration if available, otherwise relative timestamp
                    if let Some(ms) = s.duration_ms {
                        let d = Duration::from_millis(ms);
                        Some(
                            theme
                                .duration
                                .apply_to(crate::ui::format_duration(d))
                                .to_string(),
                        )
                    } else {
                        s.last_run
                            .map(|ts| theme.dim.apply_to(format_relative_time(ts)).to_string())
                    }
                })
                .unwrap_or_default();

            ui.message(&format!(
                "    {} {:<20} {}",
                kind.styled(&theme),
                step_name,
                right_side,
            ));
        }

        // Show recommendations
        let never_run: Vec<_> = config
            .steps
            .keys()
            .filter(|s| {
                state
                    .get_step(s)
                    .map(|st| st.status == StepStatus::NeverRun)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        let failed: Vec<_> = config
            .steps
            .keys()
            .filter(|s| {
                state
                    .get_step(s)
                    .map(|st| st.status == StepStatus::Failed)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        if !failed.is_empty() {
            ui.message("");
            ui.show_hint(&hints::after_failed_run(&failed));
        } else if !never_run.is_empty() {
            ui.message("");
            if never_run.len() == config.steps.len() {
                ui.show_hint(hints::all_steps_pending());
            } else {
                ui.show_hint(&hints::some_steps_pending(&never_run));
            }
        }

        Ok(CommandResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(config: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), config).unwrap();
        temp
    }

    #[test]
    fn status_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn status_no_config() {
        let temp = TempDir::new().unwrap();
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn status_with_config() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn status_applies_config_default_output() {
        let config = r#"
app_name: Test
settings:
  default_output: quiet
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
    }

    #[test]
    fn status_shows_pending_for_never_run_steps() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Never-run steps should show ◌ icon (pending)
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("◌") && m.contains("hello")));
        // Should NOT use warning for pending steps
        assert!(!ui.warnings().iter().any(|m| m.contains("hello")));
    }

    #[test]
    fn status_shows_header_with_app_name() {
        let config = r#"
app_name: MyApp
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show app name in header
        assert!(ui.messages().iter().any(|m| m.contains("MyApp")));
        // Should show ⛺ tent icon
        assert!(ui.messages().iter().any(|m| m.contains("⛺")));
        // Should show "Status" label
        assert!(ui.messages().iter().any(|m| m.contains("Status")));
    }

    #[test]
    fn status_shows_hint_for_all_pending() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
  world:
    command: echo world
workflows:
  default:
    steps: [hello, world]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // All steps pending → hint to run setup
        assert!(ui.hints().iter().any(|m| m.contains("bivvy run")));
    }

    #[test]
    fn status_shows_steps_label() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = StatusArgs::default();
        let cmd = StatusCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("Steps:")));
    }
}
