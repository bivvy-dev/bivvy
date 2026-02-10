//! List command implementation.
//!
//! The `bivvy list` command lists steps and workflows.

use std::path::{Path, PathBuf};

use crate::cli::args::ListArgs;
use crate::config::load_merged_config;
use crate::error::{BivvyError, Result};
use crate::ui::UserInterface;

use super::dispatcher::{Command, CommandResult};

/// The list command implementation.
pub struct ListCommand {
    project_root: PathBuf,
    args: ListArgs,
}

impl ListCommand {
    /// Create a new list command.
    pub fn new(project_root: &Path, args: ListArgs) -> Self {
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
    pub fn args(&self) -> &ListArgs {
        &self.args
    }
}

impl Command for ListCommand {
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

        // Show steps
        if !self.args.workflows_only {
            ui.message("Steps:");
            for (name, step) in &config.steps {
                // First line: name with template/command info
                let detail = if let Some(ref template) = step.template {
                    format!(" (template: {})", template)
                } else if let Some(ref cmd) = step.command {
                    format!(" — {}", cmd)
                } else {
                    String::new()
                };
                let depends = if step.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" -> {}", step.depends_on.join(", "))
                };
                ui.message(&format!("  {}{}{}", name, detail, depends));

                // Second line: description or title if present
                if let Some(ref desc) = step.description {
                    ui.message(&format!("    {}", desc));
                } else if let Some(ref title) = step.title {
                    ui.message(&format!("    {}", title));
                }
            }

            if !self.args.steps_only {
                ui.message("");
            }
        }

        // Show workflows
        if !self.args.steps_only {
            ui.message("Workflows:");
            for (name, workflow) in &config.workflows {
                let steps = if workflow.steps.len() > 5 {
                    format!("{}, ...", workflow.steps[..5].join(", "))
                } else {
                    workflow.steps.join(", ")
                };
                ui.message(&format!("  {}: [{}]", name, steps));

                if let Some(ref desc) = workflow.description {
                    ui.message(&format!("    {}", desc));
                }
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
    fn list_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn list_no_config() {
        let temp = TempDir::new().unwrap();
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn list_with_config() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
  world:
    command: echo world
    depends_on: [hello]
workflows:
  default:
    steps: [hello, world]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn list_shows_step_command() {
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
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("echo hello")));
    }

    #[test]
    fn list_shows_step_description() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
    description: "Prints a greeting"
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("Prints a greeting")));
    }

    #[test]
    fn list_shows_step_title_when_no_description() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
    title: "Hello Step"
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("Hello Step")));
    }

    #[test]
    fn list_shows_template_instead_of_command() {
        let config = r#"
app_name: Test
steps:
  deps:
    template: yarn
workflows:
  default:
    steps: [deps]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("template: yarn")));
    }

    #[test]
    fn list_shows_workflow_description() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    description: "Full development setup"
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("Full development setup")));
    }

    #[test]
    fn list_steps_only() {
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
        let args = ListArgs {
            steps_only: true,
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }
}
