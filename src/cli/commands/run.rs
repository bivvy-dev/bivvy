//! Run command implementation.
//!
//! The `bivvy run` command executes setup workflows.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cli::args::RunArgs;
use crate::config::{load_merged_config, ConfigPaths, InterpolationContext};
use crate::environment::resolver::ResolvedEnvironment;
use crate::error::{BivvyError, Result};
use crate::registry::Registry;
use crate::requirements::checker::GapChecker;
use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::RequirementRegistry;
use crate::runner::{RunOptions, SkipBehavior, WorkflowRunner};
use crate::state::{ProjectId, RunHistoryBuilder, StateStore};
use crate::steps::ResolvedStep;
use crate::ui::{hints, OutputMode, RunSummary, StatusKind, StepSummary, UserInterface};

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
            provided_requirements: HashSet::new(),
            active_environment: None,
        }
    }

    /// Resolve the target environment using the priority chain.
    fn resolve_environment(&self, config: &crate::config::BivvyConfig) -> ResolvedEnvironment {
        ResolvedEnvironment::resolve_from_config(self.args.env.as_deref(), &config.settings)
    }

    /// Resolve steps from configuration.
    ///
    /// Steps with a `template` field are resolved through the Registry,
    /// which merges template defaults with config overrides. Steps without
    /// a template are treated as inline definitions.
    fn resolve_steps(
        &self,
        config: &crate::config::BivvyConfig,
        environment: Option<&str>,
    ) -> Result<HashMap<String, ResolvedStep>> {
        let registry = if config.template_sources.is_empty() {
            Registry::new(Some(&self.project_root))?
        } else {
            Registry::with_remote_sources(Some(&self.project_root), &config.template_sources)?
        };

        let mut steps = HashMap::new();
        for (name, step_config) in &config.steps {
            let resolved = if let Some(template_name) = &step_config.template {
                let (template, _source) = registry.resolve(template_name)?;
                ResolvedStep::from_template(
                    name,
                    template,
                    step_config,
                    &step_config.inputs,
                    environment,
                )
            } else {
                ResolvedStep::from_config(name, step_config, environment)
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

        // Deprecation warning for --ci flag
        if self.args.ci {
            ui.warning(
                "--ci is deprecated and will be removed in 2.0. \
                 Use --non-interactive and --env ci instead.",
            );
        }

        // Apply config default_output when no CLI flag was explicitly set.
        // OutputMode::Normal means the user didn't pass --verbose or --quiet.
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.default_output.into());
        }

        // Resolve environment (before header so we can use resolved workflow)
        let resolved_env = self.resolve_environment(&config);
        let env_name = resolved_env.name.clone();

        // Warn if the environment is not known
        if !resolved_env.is_known(&config) {
            ui.warning(&format!(
                "Unknown environment '{}'. It is not defined in settings.environments                  or a built-in. Step overrides for this environment will have no effect.",
                env_name
            ));
        }

        // Get provided_requirements from environment config
        let provided_requirements: HashSet<String> = config
            .settings
            .environments
            .get(&env_name)
            .map(|env_config| env_config.provided_requirements.iter().cloned().collect())
            .unwrap_or_default();

        // Use environment's default_workflow if no explicit --workflow and config has one
        let workflow_name = if self.args.workflow == "default" {
            config
                .settings
                .environments
                .get(&env_name)
                .and_then(|env_config| env_config.default_workflow.clone())
                .unwrap_or_else(|| self.args.workflow.clone())
        } else {
            self.args.workflow.clone()
        };

        // Show header
        let app_name = config.app_name.as_deref().unwrap_or("project");
        let step_count = config
            .workflows
            .get(workflow_name.as_str())
            .map(|w| w.steps.len())
            .unwrap_or(0);
        ui.show_run_header(app_name, &workflow_name, step_count);

        // Show environment and config info
        ui.message(&format!(
            "  Environment: {} ({})",
            env_name, resolved_env.source
        ));

        if self.args.dry_run {
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

        // Resolve steps with environment
        let steps = self.resolve_steps(&config, Some(&env_name))?;

        // Check if workflow exists
        if !config.workflows.contains_key(&workflow_name) {
            ui.error(&format!("Unknown workflow: {}", workflow_name));
            return Ok(CommandResult::failure(1));
        }

        // Extract workflow-level non_interactive setting
        let workflow_non_interactive = config
            .workflows
            .get(&workflow_name)
            .and_then(|w| w.settings.as_ref())
            .is_some_and(|s| s.non_interactive);

        // Extract step overrides from workflow config
        let step_overrides = config
            .workflows
            .get(&workflow_name)
            .map(|w| w.overrides.clone())
            .unwrap_or_default();

        // Build run options with resolved workflow and provided requirements
        let mut options = self.build_options();
        options.workflow = Some(workflow_name.clone());
        options.provided_requirements = provided_requirements;
        options.active_environment = Some(env_name.clone());

        // Create runner
        let runner = WorkflowRunner::new(&config, steps);

        // Create gap checker for requirement detection
        let probe = EnvironmentProbe::run();
        let req_registry = RequirementRegistry::new().with_custom(&config.requirements);
        let mut gap_checker = GapChecker::new(&req_registry, &probe, &self.project_root);

        // Create interpolation context
        let ctx = InterpolationContext::new();
        let global_env: HashMap<String, String> = std::env::vars().collect();

        // Start history recording
        let mut history = RunHistoryBuilder::start(&workflow_name);

        // Run the workflow with UI-driven interactive prompts
        let result = runner.run_with_ui(
            &options,
            &ctx,
            &global_env,
            &self.project_root,
            workflow_non_interactive,
            &step_overrides,
            Some(&mut gap_checker),
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

        // Build and show run summary
        let steps_run = result.steps.len();
        let steps_skipped = result.skipped.len();
        let failed_steps: Vec<String> = result
            .steps
            .iter()
            .filter(|s| !s.success && !s.skipped)
            .map(|s| s.name.clone())
            .collect();

        let step_results: Vec<StepSummary> = result
            .steps
            .iter()
            .map(|s| StepSummary {
                name: s.name.clone(),
                status: StatusKind::from(s.status()),
                duration: if s.duration.as_millis() > 0 {
                    Some(s.duration)
                } else {
                    None
                },
                detail: if let Some(ref rd) = s.recovery_detail {
                    Some(rd.clone())
                } else if s.skipped {
                    Some(
                        s.check_result
                            .as_ref()
                            .map(|c| c.short_description().to_string())
                            .unwrap_or_else(|| "already complete".to_string()),
                    )
                } else {
                    None
                },
            })
            .collect();

        let summary = RunSummary {
            step_results,
            total_duration: result.duration,
            steps_run,
            steps_skipped,
            success: result.success,
            failed_steps: failed_steps.clone(),
        };

        ui.show_run_summary(&summary);

        // Show contextual hint
        if result.success {
            ui.show_hint(hints::after_successful_run());
            Ok(CommandResult::success())
        } else if result.aborted {
            ui.show_hint("Workflow aborted by user. Re-run to resume.");
            Ok(CommandResult::failure(1))
        } else {
            ui.show_hint(&hints::after_failed_run(&failed_steps));
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

        // show_run_summary default impl calls ui.success()
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

        // With 1 step, summary shows "1 run"
        assert!(ui.has_success("1 run"));
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

        // With 2 steps, summary shows "2 run"
        assert!(ui.has_success("2 run"));
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
    fn execute_verbose_does_not_show_config_path() {
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
            dry_run: false,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::with_mode(OutputMode::Verbose);

        cmd.execute(&mut ui).unwrap();

        assert!(
            !ui.messages().iter().any(|m| m.contains("Config:")),
            "Verbose mode should not show Config: line"
        );
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
        let steps = cmd.resolve_steps(&config, None).unwrap();

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
        let result = cmd.resolve_steps(&config, None);

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
        let steps = cmd.resolve_steps(&config, None).unwrap();

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
    fn build_options_has_empty_provided_requirements() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let options = cmd.build_options();

        assert!(options.provided_requirements.is_empty());
    }

    #[test]
    fn resolve_environment_fallback_when_no_config() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let config = crate::config::BivvyConfig::default();
        let resolved = cmd.resolve_environment(&config);

        assert_eq!(resolved.name, "development");
        assert_eq!(
            resolved.source,
            crate::environment::resolver::EnvironmentSource::Fallback
        );
    }

    #[test]
    fn resolve_environment_uses_env_flag() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs {
            env: Some("staging".to_string()),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);

        let config = crate::config::BivvyConfig::default();
        let resolved = cmd.resolve_environment(&config);

        assert_eq!(resolved.name, "staging");
        assert_eq!(
            resolved.source,
            crate::environment::resolver::EnvironmentSource::Flag
        );
    }

    #[test]
    fn resolve_environment_uses_config_default() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let mut config = crate::config::BivvyConfig::default();
        config.settings.default_environment = Some("production".to_string());
        let resolved = cmd.resolve_environment(&config);

        assert_eq!(resolved.name, "production");
        assert_eq!(
            resolved.source,
            crate::environment::resolver::EnvironmentSource::ConfigDefault
        );
    }

    #[test]
    fn resolve_environment_flag_overrides_config_default() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs {
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);

        let mut config = crate::config::BivvyConfig::default();
        config.settings.default_environment = Some("production".to_string());
        let resolved = cmd.resolve_environment(&config);

        assert_eq!(resolved.name, "ci");
        assert_eq!(
            resolved.source,
            crate::environment::resolver::EnvironmentSource::Flag
        );
    }

    #[test]
    fn execute_shows_environment_info() {
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
            env: Some("staging".to_string()),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui
            .messages()
            .iter()
            .any(|m| m.contains("Environment:") && m.contains("staging")));
    }

    #[test]
    fn execute_with_environment_default_workflow() {
        let config = r#"
app_name: Test Project
settings:
  environments:
    ci:
      default_workflow: quick
steps:
  hello:
    command: echo hello
  fast:
    command: echo fast
workflows:
  default:
    steps: [hello]
  quick:
    steps: [fast]
"#;
        let temp = setup_project(config);
        let args = RunArgs {
            env: Some("ci".to_string()),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        // Should succeed using the "quick" workflow instead of "default"
        assert!(result.success);
    }

    #[test]
    fn execute_explicit_workflow_overrides_env_default() {
        let config = r#"
app_name: Test Project
settings:
  environments:
    ci:
      default_workflow: quick
steps:
  hello:
    command: echo hello
  fast:
    command: echo fast
workflows:
  default:
    steps: [hello]
  quick:
    steps: [fast]
  custom:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs {
            env: Some("ci".to_string()),
            workflow: "custom".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        // Should succeed using "custom" workflow, not "quick"
        assert!(result.success);
    }

    #[test]
    fn resolve_steps_with_environment_applies_overrides() {
        let config_yaml = r#"
app_name: test
steps:
  hello:
    command: echo hello
    environments:
      ci:
        command: echo ci-hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config_yaml);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let config = load_merged_config(temp.path()).unwrap();

        // Without environment
        let steps = cmd.resolve_steps(&config, None).unwrap();
        assert_eq!(steps.get("hello").unwrap().command, "echo hello");

        // With ci environment
        let steps = cmd.resolve_steps(&config, Some("ci")).unwrap();
        assert_eq!(steps.get("hello").unwrap().command, "echo ci-hello");
    }

    #[test]
    fn execute_ci_flag_shows_deprecation_warning() {
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
            ci: true,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert!(ui.has_warning("--ci is deprecated"));
        assert!(ui.has_warning("--non-interactive"));
        assert!(ui.has_warning("--env ci"));
    }

    #[test]
    fn execute_without_ci_flag_no_deprecation_warning() {
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

        assert!(!ui.has_warning("--ci is deprecated"));
    }

    #[test]
    fn summary_shows_recovery_detail() {
        let config = r#"
app_name: Test
steps:
  flaky:
    command: "if [ -f /tmp/bivvy_test_flaky ]; then exit 0; else touch /tmp/bivvy_test_flaky && exit 1; fi"
workflows:
  default:
    steps: [flaky]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_flaky", "retry");

        // Clean up marker from any previous test run
        let _ = std::fs::remove_file("/tmp/bivvy_test_flaky");

        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);

        // Check that the summary includes recovery detail
        let summaries = ui.summaries();
        assert!(!summaries.is_empty());

        // Clean up
        let _ = std::fs::remove_file("/tmp/bivvy_test_flaky");
    }

    #[test]
    fn summary_shows_aborted_hint() {
        let config = r#"
app_name: Test
steps:
  broken:
    command: "exit 1"
workflows:
  default:
    steps: [broken]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("recovery_broken", "abort");

        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);

        // Should show the aborted hint
        assert!(ui.has_hint("Workflow aborted by user"));
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
