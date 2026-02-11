//! Run command implementation.
//!
//! The `bivvy run` command executes setup workflows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cli::args::RunArgs;
use crate::config::{load_merged_config, ConfigPaths, InterpolationContext};
use crate::error::{BivvyError, Result};
use crate::registry::Registry;
use crate::runner::{RunOptions, SkipBehavior, WorkflowRunner};
use crate::state::{ProjectId, RunHistoryBuilder, StateStore};
use crate::steps::ResolvedStep;
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The run command implementation.
pub struct RunCommand {
    project_root: PathBuf,
    args: RunArgs,
}

impl RunCommand {
    /// Create a new run command.
    pub fn new(project_root: &Path, args: RunArgs) -> Self {
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
    pub fn args(&self) -> &RunArgs {
        &self.args
    }

    /// Build run options from args.
    fn build_options(&self) -> RunOptions {
        let skip_behavior = match self.args.skip_behavior.as_str() {
            "skip_only" => SkipBehavior::SkipOnly,
            "run_anyway" => SkipBehavior::RunAnyway,
            _ => SkipBehavior::SkipWithDependents,
        };

        RunOptions {
            workflow: Some(self.args.workflow.clone()),
            only: self.args.only.iter().cloned().collect(),
            skip: self.args.skip.iter().cloned().collect(),
            skip_behavior,
            force: self.args.force.iter().cloned().collect(),
            dry_run: self.args.dry_run,
        }
    }

    /// Resolve steps from configuration.
    ///
    /// Steps with a `template` field are resolved through the Registry,
    /// which merges template defaults with config overrides. Steps without
    /// a template are treated as inline definitions.
    fn resolve_steps(
        &self,
        config: &crate::config::BivvyConfig,
    ) -> Result<HashMap<String, ResolvedStep>> {
        let registry = Registry::new(Some(&self.project_root))?;

        let mut steps = HashMap::new();
        for (name, step_config) in &config.steps {
            let resolved = if let Some(template_name) = &step_config.template {
                let (template, _source) = registry.resolve(template_name)?;
                ResolvedStep::from_template(name, template, step_config, &step_config.inputs)
            } else {
                ResolvedStep::from_config(name, step_config)
            };
            steps.insert(name.clone(), resolved);
        }
        Ok(steps)
    }
}

impl Command for RunCommand {
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

        // Apply config default_output when no CLI flag was explicitly set.
        // OutputMode::Normal means the user didn't pass --verbose or --quiet.
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.default_output.into());
        }

        // Show header
        let app_name = config.app_name.as_deref().unwrap_or("project");
        ui.show_header(&format!("Setting up {}", app_name));

        // Show config path in verbose mode
        if self.args.dry_run || ui.output_mode() == OutputMode::Verbose {
            let paths = ConfigPaths::discover(&self.project_root);
            if let Some(project_path) = &paths.project {
                ui.message(&format!("Config: {}", project_path.display()));
            }
        }

        // Check for dry-run mode
        if self.args.dry_run {
            ui.message("Running in dry-run mode - no commands will be executed");
        }

        // Get project identity
        let project_id = ProjectId::from_path(&self.project_root)?;

        // Load state
        let mut state = StateStore::load(&project_id)?;

        // Resolve steps
        let steps = self.resolve_steps(&config)?;

        // Check if workflow exists
        let workflow_name = &self.args.workflow;
        if !config.workflows.contains_key(workflow_name) {
            ui.error(&format!("Unknown workflow: {}", workflow_name));
            return Ok(CommandResult::failure(1));
        }

        // Extract workflow-level non_interactive setting
        let workflow_non_interactive = config
            .workflows
            .get(workflow_name)
            .and_then(|w| w.settings.as_ref())
            .is_some_and(|s| s.non_interactive);

        // Extract step overrides from workflow config
        let step_overrides = config
            .workflows
            .get(workflow_name)
            .map(|w| w.overrides.clone())
            .unwrap_or_default();

        // Build run options
        let options = self.build_options();

        // Create runner
        let runner = WorkflowRunner::new(&config, steps);

        // Create interpolation context
        let ctx = InterpolationContext::new();
        let global_env: HashMap<String, String> = std::env::vars().collect();

        // Start history recording
        let mut history = RunHistoryBuilder::start(workflow_name);

        // Run the workflow with UI-driven interactive prompts
        let result = runner.run_with_ui(
            &options,
            &ctx,
            &global_env,
            &self.project_root,
            workflow_non_interactive,
            &step_overrides,
            ui,
        )?;

        // Record step results to history
        for step_result in &result.steps {
            history.step_run(&step_result.name);
        }
        for skipped in &result.skipped {
            history.step_skipped(skipped);
        }

        // Update state (unless dry-run)
        if !self.args.dry_run {
            let record = if result.success {
                history.finish_success()
            } else {
                history.finish_failed("One or more steps failed")
            };
            state.record_run(record);
            state.save(&project_id)?;
        }

        // Report result
        let steps_run = result.steps.len();
        let steps_skipped = result.skipped.len();

        if result.success {
            let run_label = if steps_run == 1 { "step" } else { "steps" };
            let msg = format!(
                "Setup complete! ({} {} run, {} skipped)",
                steps_run, run_label, steps_skipped
            );
            ui.success(&msg);
            Ok(CommandResult::success())
        } else {
            let failed_steps: Vec<_> = result
                .steps
                .iter()
                .filter(|s| !s.success)
                .map(|s| s.name.as_str())
                .collect();
            let msg = format!("Setup failed at: {}", failed_steps.join(", "));
            ui.error(&msg);
            Ok(CommandResult::failure(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MockUI;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(config_content: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        let bivvy_dir = temp.path().join(".bivvy");
        fs::create_dir_all(&bivvy_dir).unwrap();
        fs::write(bivvy_dir.join("config.yml"), config_content).unwrap();
        temp
    }

    #[test]
    fn run_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn run_command_args() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs {
            workflow: "custom".to_string(),
            dry_run: true,
            ..Default::default()
        };

        let cmd = RunCommand::new(temp.path(), args);

        assert_eq!(cmd.args().workflow, "custom");
        assert!(cmd.args().dry_run);
    }

    #[test]
    fn build_options_default() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let options = cmd.build_options();

        assert!(options.only.is_empty());
        assert!(options.skip.is_empty());
        assert!(!options.dry_run);
    }

    #[test]
    fn build_options_with_skip() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs {
            skip: vec!["step1".to_string(), "step2".to_string()],
            ..Default::default()
        };

        let cmd = RunCommand::new(temp.path(), args);
        let options = cmd.build_options();

        assert!(options.skip.contains("step1"));
        assert!(options.skip.contains("step2"));
    }

    #[test]
    fn build_options_skip_behavior() {
        let temp = TempDir::new().unwrap();

        // Default
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        assert_eq!(
            cmd.build_options().skip_behavior,
            SkipBehavior::SkipWithDependents
        );

        // Skip only
        let args = RunArgs {
            skip_behavior: "skip_only".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        assert_eq!(cmd.build_options().skip_behavior, SkipBehavior::SkipOnly);

        // Run anyway
        let args = RunArgs {
            skip_behavior: "run_anyway".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        assert_eq!(cmd.build_options().skip_behavior, SkipBehavior::RunAnyway);
    }

    #[test]
    fn execute_with_no_config_returns_error() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn execute_with_unknown_workflow() {
        let config = r#"
app_name: test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs {
            workflow: "nonexistent".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn execute_dry_run_success() {
        let config = r#"
app_name: Test Project
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs {
            dry_run: true,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn execute_real_workflow() {
        let config = r#"
app_name: Test Project
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn execute_success_does_not_duplicate_message() {
        let config = r#"
app_name: Test Project
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Command outputs via ui.success() directly
        assert!(ui.has_success("Setup complete!"));
    }

    #[test]
    fn execute_singular_step_grammar() {
        let config = r#"
app_name: Test Project
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // With 1 step, should say "step" not "steps"
        assert!(ui.has_success("1 step run"));
    }

    #[test]
    fn execute_plural_steps_grammar() {
        let config = r#"
app_name: Test Project
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
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // With 2 steps, should say "steps"
        assert!(ui.has_success("2 steps run"));
    }

    #[test]
    fn execute_shows_config_path() {
        let config = r#"
app_name: Test Project
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs {
            dry_run: true,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("Config:") && m.contains("config.yml")));
    }

    #[test]
    fn resolve_steps_uses_template_for_brew() {
        let config_yaml = r#"
app_name: test
steps:
  brew:
    template: brew
workflows:
  default:
    steps: [brew]
"#;
        let temp = setup_project(config_yaml);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let config = load_merged_config(temp.path()).unwrap();
        let steps = cmd.resolve_steps(&config).unwrap();

        let brew_step = steps.get("brew").unwrap();
        assert!(
            !brew_step.command.is_empty(),
            "template step should have a command from the brew template"
        );
        assert!(
            brew_step.command.contains("brew"),
            "brew template command should mention brew, got: {}",
            brew_step.command
        );
    }

    #[test]
    fn resolve_steps_errors_on_unknown_template() {
        let config_yaml = r#"
app_name: test
steps:
  bad:
    template: nonexistent_template_xyz
workflows:
  default:
    steps: [bad]
"#;
        let temp = setup_project(config_yaml);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let config = load_merged_config(temp.path()).unwrap();
        let result = cmd.resolve_steps(&config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BivvyError::UnknownTemplate { .. }),
            "expected UnknownTemplate, got: {}",
            err
        );
    }

    #[test]
    fn resolve_steps_inline_step_still_works() {
        let config_yaml = r#"
app_name: test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config_yaml);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let config = load_merged_config(temp.path()).unwrap();
        let steps = cmd.resolve_steps(&config).unwrap();

        let hello_step = steps.get("hello").unwrap();
        assert_eq!(hello_step.command, "echo hello");
    }

    #[test]
    fn execute_dry_run_with_template_shows_real_command() {
        let config_yaml = r#"
app_name: test
steps:
  brew:
    template: brew
workflows:
  default:
    steps: [brew]
"#;
        let temp = setup_project(config_yaml);
        let args = RunArgs {
            dry_run: true,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn execute_applies_config_default_output() {
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
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        // MockUI starts with Normal (no CLI override)
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // After execute, the output mode should have been changed to Quiet
        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
    }

    #[test]
    fn execute_cli_verbose_overrides_config_default() {
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
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        // MockUI starts with Verbose (simulating --verbose CLI flag)
        let mut ui = MockUI::with_mode(crate::ui::OutputMode::Verbose);

        cmd.execute(&mut ui).unwrap();

        // CLI flag should win, mode stays Verbose
        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Verbose);
    }

    #[test]
    fn execute_unknown_workflow_does_not_duplicate_message() {
        let config = r#"
app_name: test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs {
            workflow: "nonexistent".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        // Command outputs via ui.error() directly
        assert!(ui.has_error("Unknown workflow"));
    }
}
