//! List command implementation.
//!
//! The `bivvy list` command lists steps and workflows.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::args::ListArgs;
use crate::config::load_config;
use crate::environment::resolver::ResolvedEnvironment;
use crate::error::{BivvyError, Result};
use crate::ui::theme::BivvyTheme;
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// JSON output for the list command.
#[derive(Debug, Serialize)]
struct ListJsonOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<Vec<StepJsonEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflows: Option<Vec<WorkflowJsonEntry>>,
}

/// A single step in JSON output.
#[derive(Debug, Serialize)]
struct StepJsonEntry {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    skipped: bool,
}

/// A single workflow in JSON output.
#[derive(Debug, Serialize)]
struct WorkflowJsonEntry {
    name: String,
    steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

/// The list command implementation.
pub struct ListCommand {
    project_root: PathBuf,
    args: ListArgs,
    config_override: Option<PathBuf>,
}

impl ListCommand {
    /// Create a new list command.
    pub fn new(project_root: &Path, args: ListArgs) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            args,
            config_override: None,
        }
    }

    /// Set an override config path.
    pub fn with_config_override(mut self, config_override: Option<PathBuf>) -> Self {
        self.config_override = config_override;
        self
    }

    /// Get the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the command arguments.
    pub fn args(&self) -> &ListArgs {
        &self.args
    }

    /// Resolve the target environment using the priority chain.
    fn resolve_environment(&self, config: &crate::config::BivvyConfig) -> ResolvedEnvironment {
        ResolvedEnvironment::resolve_from_config(self.args.env.as_deref(), &config.settings)
    }
}

impl Command for ListCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Load configuration
        let config = match load_config(&self.project_root, self.config_override.as_deref()) {
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

        // Resolve environment
        let resolved_env = self.resolve_environment(&config);
        let env_name = &resolved_env.name;

        // JSON output mode
        if self.args.json {
            let steps = if !self.args.workflows_only {
                let mut entries: Vec<StepJsonEntry> = config
                    .steps
                    .iter()
                    .map(|(name, step)| {
                        let skipped = !step.only_environments.is_empty()
                            && !step.only_environments.iter().any(|e| e == env_name);
                        StepJsonEntry {
                            name: name.clone(),
                            template: step.template.clone(),
                            command: step.command.clone(),
                            description: step.description.clone(),
                            title: step.title.clone(),
                            depends_on: step.depends_on.clone(),
                            skipped,
                        }
                    })
                    .collect();
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                Some(entries)
            } else {
                None
            };

            let workflows = if !self.args.steps_only {
                let mut entries: Vec<WorkflowJsonEntry> = config
                    .workflows
                    .iter()
                    .map(|(name, workflow)| WorkflowJsonEntry {
                        name: name.clone(),
                        steps: workflow.steps.clone(),
                        description: workflow.description.clone(),
                    })
                    .collect();
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                Some(entries)
            } else {
                None
            };

            let output = ListJsonOutput {
                environment: Some(env_name.clone()),
                steps,
                workflows,
            };

            let json = serde_json::to_string_pretty(&output).map_err(|e| {
                BivvyError::ConfigValidationError {
                    message: format!("JSON serialization failed: {}", e),
                }
            })?;
            ui.message(&json);
            return Ok(CommandResult::success());
        }

        // Show environment info
        ui.message(&format!(
            "  {} {} ({})\n",
            theme.key.apply_to("Environment:"),
            theme.highlight.apply_to(env_name),
            theme.dim.apply_to(resolved_env.source.to_string()),
        ));

        // Show steps
        if !self.args.workflows_only {
            ui.message(&format!("  {}", theme.key.apply_to("Steps:")));
            for (name, step) in &config.steps {
                // Check if step is skipped by only_environments
                let skipped = !step.only_environments.is_empty()
                    && !step.only_environments.iter().any(|e| e == env_name);

                if skipped {
                    ui.message(&format!(
                        "    {} {}",
                        theme.dim.apply_to(name),
                        theme.dim.apply_to(format!("(skipped in {})", env_name)),
                    ));
                    continue;
                }

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
    use crate::ui::{MockUI, UiState};
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
    template: yarn-install
workflows:
  default:
    steps: [deps]
"#;
        let temp = setup_project(config);
        let args = ListArgs::default();
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("template: yarn-install")));
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
    fn list_shows_environment_info() {
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

        assert!(ui.messages().iter().any(|m| m.contains("Environment:")));
        // The resolved name depends on where the test runs:
        // "ci" in CI (auto-detected), "development" locally (fallback)
        let has_env_name = ui
            .messages()
            .iter()
            .any(|m| m.contains("development") || m.contains("ci"));
        assert!(has_env_name, "Should show resolved environment name");
    }

    #[test]
    fn list_shows_environment_from_env_flag() {
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
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.messages().iter().any(|m| m.contains("ci")));
    }

    #[test]
    fn list_shows_skipped_steps_for_environment() {
        let config = r#"
app_name: Test
steps:
  dev_only:
    command: echo dev
    only_environments:
      - development
  always_run:
    command: echo always
workflows:
  default:
    steps: [dev_only, always_run]
"#;
        let temp = setup_project(config);
        let args = ListArgs {
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // dev_only should show as skipped in ci
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("dev_only") && m.contains("skipped")));
        // always_run should show normally (no only_environments = runs in all)
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("always_run") && m.contains("echo always")));
    }

    #[test]
    fn list_no_skipped_when_environment_matches() {
        let config = r#"
app_name: Test
steps:
  ci_step:
    command: echo ci
    only_environments:
      - ci
workflows:
  default:
    steps: [ci_step]
"#;
        let temp = setup_project(config);
        let args = ListArgs {
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // ci_step should show normally, not skipped
        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("ci_step") && m.contains("echo ci")));
        assert!(!ui
            .messages()
            .iter()
            .any(|m| m.contains("ci_step") && m.contains("skipped")));
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

    #[test]
    fn list_json_output() {
        let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
    description: "Says hello"
  world:
    command: echo world
    depends_on: [hello]
workflows:
  default:
    description: "Default workflow"
    steps: [hello, world]
"#;
        let temp = setup_project(config);
        let args = ListArgs {
            json: true,
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let output = ui.messages().join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["environment"].is_string());
        assert!(parsed["steps"].is_array());
        assert!(parsed["workflows"].is_array());
        assert_eq!(parsed["steps"][0]["name"], "hello");
        assert_eq!(parsed["steps"][0]["command"], "echo hello");
        assert_eq!(parsed["steps"][0]["description"], "Says hello");
        assert_eq!(parsed["steps"][1]["name"], "world");
        assert_eq!(parsed["steps"][1]["depends_on"][0], "hello");
        assert_eq!(parsed["workflows"][0]["name"], "default");
        assert_eq!(parsed["workflows"][0]["description"], "Default workflow");
    }

    #[test]
    fn list_json_steps_only() {
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
            json: true,
            steps_only: true,
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let output = ui.messages().join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["steps"].is_array());
        assert!(parsed.get("workflows").is_none());
    }

    #[test]
    fn list_json_workflows_only() {
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
            json: true,
            workflows_only: true,
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let output = ui.messages().join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("steps").is_none());
        assert!(parsed["workflows"].is_array());
    }

    #[test]
    fn list_json_skipped_step() {
        let config = r#"
app_name: Test
steps:
  dev_only:
    command: echo dev
    only_environments:
      - development
workflows:
  default:
    steps: [dev_only]
"#;
        let temp = setup_project(config);
        let args = ListArgs {
            json: true,
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = ListCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
        let output = ui.messages().join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["steps"][0]["skipped"], true);
    }
}
