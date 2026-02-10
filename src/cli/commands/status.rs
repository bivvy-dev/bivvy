//! Status command implementation.
//!
//! The `bivvy status` command shows current setup status.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cli::args::StatusArgs;
use crate::config::load_merged_config;
use crate::error::{BivvyError, Result};
use crate::state::{ProjectId, StateStore, StepStatus};
use crate::ui::UserInterface;

use super::dispatcher::{Command, CommandResult};
use super::display;

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

        // Get project identity
        let project_id = ProjectId::from_path(&self.project_root)?;

        // Load state
        let state = StateStore::load(&project_id)?;

        // Show header
        let app_name = config.app_name.as_deref().unwrap_or("Bivvy Setup");
        ui.show_header(&format!("{} - Status", app_name));

        // Show last run info
        if let Some(last_run) = state.last_run_record() {
            ui.message(&format!(
                "Last run: {} ({})",
                last_run.timestamp.format("%Y-%m-%d %H:%M"),
                last_run.workflow
            ));
            ui.message("");
        }

        // Show step status
        ui.message("Steps:");
        let mut seen_statuses = HashSet::new();

        if let Some(step_name) = &self.args.step {
            // Show single step
            let status = state
                .get_step(step_name)
                .map(|s| &s.status)
                .unwrap_or(&StepStatus::NeverRun);
            seen_statuses.insert(display::status_key(status));
            display::show_step_status(ui, step_name, status);
        } else {
            // Show all steps
            for step_name in config.steps.keys() {
                let status = state
                    .get_step(step_name)
                    .map(|s| &s.status)
                    .unwrap_or(&StepStatus::NeverRun);
                seen_statuses.insert(display::status_key(status));
                display::show_step_status(ui, step_name, status);
            }
        }

        // Show legend for status indicators
        if let Some(legend) = display::format_legend(&seen_statuses) {
            ui.message("");
            ui.message(&legend);
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

        if !never_run.is_empty() && never_run.len() < config.steps.len() {
            ui.message("");
            ui.message(&format!(
                "Run `bivvy run --only={}` to run remaining steps",
                never_run.join(",")
            ));
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

        // Never-run steps should show [pending], not [--]
        assert!(ui.messages().iter().any(|m| m.contains("[pending] hello")));
        // Should NOT use warning for pending steps
        assert!(!ui.warnings().iter().any(|m| m.contains("hello")));
    }

    #[test]
    fn status_shows_legend() {
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

        assert!(ui.messages().iter().any(|m| m.contains("Legend:")));
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("[pending] not yet run")));
    }
}
