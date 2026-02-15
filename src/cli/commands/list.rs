//! List command implementation.
//!
//! The `bivvy list` command lists steps and workflows.

use std::path::{Path, PathBuf};

use crate::cli::args::ListArgs;
use crate::config::load_merged_config;
use crate::error::{BivvyError, Result};
use crate::ui::theme::BivvyTheme;
use crate::ui::{OutputMode, UserInterface};

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

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.default_output.into());
        }

        let theme = BivvyTheme::new();

        // Show steps
        if !self.args.workflows_only {
            ui.message(&format!("  {}", theme.key.apply_to("Steps:")));
            for (name, step) in &config.steps {
                // First line: step name with template/command detail
                let detail = if let Some(ref template) = step.template {
                    format!(
                        " {}",
                        theme.dim.apply_to(format!("(template: {})", template))
                    )
                } else if let Some(ref cmd) = step.command {
                    format!(
                        " {} {}",
                        theme.dim.apply_to("—"),
                        theme.command.apply_to(cmd)
                    )
                } else {
                    String::new()
                };
                ui.message(&format!("    {}{}", theme.highlight.apply_to(name), detail));

                // Description or title
                if let Some(ref desc) = step.description {
                    ui.message(&format!("      {}", theme.dim.apply_to(desc)));
                } else if let Some(ref title) = step.title {
                    ui.message(&format!("      {}", theme.dim.apply_to(title)));
                }

                // Dependency tree
                if !step.depends_on.is_empty() {
                    ui.message(&format!(
                        "      {} {}",
                        theme.dim.apply_to("└── depends on:"),
                        theme.dim.apply_to(step.depends_on.join(", "))
                    ));
                }
            }

            if !self.args.steps_only {
                ui.message("");
            }
        }

        // Show workflows
        if !self.args.steps_only {
            ui.message(&format!("  {}", theme.key.apply_to("Workflows:")));
            for (name, workflow) in &config.workflows {
                let arrow_steps = workflow.steps.join(" → ");
                ui.message(&format!(
                    "    {}{} {}",
                    theme.highlight.apply_to(name),
                    theme.dim.apply_to(":"),
                    theme.dim.apply_to(&arrow_steps),
                ));

                if let Some(ref desc) = workflow.description {
                    ui.message(&format!("      {}", theme.dim.apply_to(desc)));
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
    fn list_applies_config_default_output() {
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
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
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

    #[test]
    fn list_shows_dependency_tree() {
        let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm run build
    depends_on: [install]
workflows:
  default:
    steps: [install, build]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show dependency arrow for build
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("depends on:") && m.contains("install")));
    }

    #[test]
    fn list_shows_workflow_with_arrows() {
        let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm run build
  deploy:
    command: bin/deploy
workflows:
  default:
    steps: [install, build, deploy]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Should show workflow steps with arrow separators
        assert!(ui.messages().iter().any(|m| m.contains("→")));
    }
}
