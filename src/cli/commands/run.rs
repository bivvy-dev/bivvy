//! Run command implementation.
//!
//! The `bivvy run` command executes setup workflows.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cli::args::RunArgs;
#[cfg(test)]
use crate::config::load_merged_config;
use crate::config::{
    evaluate_vars, load_merged_config_with_trust, ConfigPaths, ExtendsResolver,
    InterpolationContext, TrustPolicy, TrustStore,
};
use crate::environment::resolver::ResolvedEnvironment;
use crate::error::{BivvyError, Result};
use crate::registry::Registry;
use crate::requirements::checker::GapChecker;
use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::RequirementRegistry;
use crate::runner::{RunOptions, SkipBehavior, WorkflowRunner};
use crate::state::{ProjectId, StateStore};
use crate::steps::ResolvedStep;
use crate::ui::{hints, OutputMode, RunSummary, StatusKind, StepSummary, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The run command implementation.
pub struct RunCommand {
    project_root: PathBuf,
    args: RunArgs,
    trust_policy: TrustPolicy,
    config_override: Option<PathBuf>,
}

impl RunCommand {
    /// Create a new run command.
    pub fn new(project_root: &Path, args: RunArgs) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            args,
            trust_policy: TrustPolicy::Prompt,
            config_override: None,
        }
    }

    /// Create a new run command with a specific trust policy.
    pub fn with_trust_policy(mut self, policy: TrustPolicy) -> Self {
        self.trust_policy = policy;
        self
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
    pub fn args(&self) -> &RunArgs {
        &self.args
    }

    /// Build run options from args.
    fn build_options(&self, config: &crate::config::BivvyConfig) -> RunOptions {
        let skip_behavior = match self.args.skip_behavior.as_str() {
            "skip_only" => SkipBehavior::SkipOnly,
            "run_anyway" => SkipBehavior::RunAnyway,
            _ => SkipBehavior::SkipWithDependents,
        };

        // CLI flags override config: --diagnostic-funnel forces on,
        // --no-diagnostic-funnel forces off, otherwise use config value.
        let diagnostic_funnel = if self.args.diagnostic_funnel {
            true
        } else if self.args.no_diagnostic_funnel {
            false
        } else {
            config.settings.execution.diagnostic_funnel
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
            diagnostic_funnel,
            fresh: self.args.fresh,
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

        let defaults = &config.settings.defaults;
        // Resolve the effective global rerun window: prefer defaults.rerun_window,
        // fall back to execution.default_rerun_window for backward compatibility.
        let global_rerun_window = defaults.rerun_window.as_deref().or(config
            .settings
            .execution
            .default_rerun_window
            .as_deref());

        let mut steps = HashMap::new();
        for (name, step_config) in &config.steps {
            let mut resolved = if let Some(template_name) = &step_config.template {
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

            // Apply global defaults for fields that weren't explicitly set at step level
            if step_config.behavior.auto_run.is_none() {
                resolved.behavior.auto_run = defaults.auto_run;
            }
            if step_config.behavior.prompt_on_rerun.is_none() {
                resolved.behavior.prompt_on_rerun = defaults.prompt_on_rerun;
            }
            if step_config.behavior.rerun_window.is_none() {
                if let Some(window_str) = global_rerun_window {
                    if let Ok(w) = window_str.parse() {
                        resolved.behavior.rerun_window = w;
                    }
                }
            }

            steps.insert(name.clone(), resolved);
        }
        Ok(steps)
    }
}

impl Command for RunCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Load configuration — use override path if provided, otherwise
        // discover and merge with trust verification for remote extends.
        let config = if let Some(ref override_path) = self.config_override {
            match crate::config::load_config_file(override_path) {
                Ok(c) => c,
                Err(e) => {
                    ui.error(&format!(
                        "Failed to load config from {}: {}",
                        override_path.display(),
                        e
                    ));
                    return Ok(CommandResult::failure(2));
                }
            }
        } else {
            let trust_store_path = TrustStore::default_path();
            let resolver = ExtendsResolver::default();
            match load_merged_config_with_trust(
                &self.project_root,
                &resolver,
                self.trust_policy,
                &trust_store_path,
                ui,
            ) {
                Ok(c) => c,
                Err(BivvyError::ConfigNotFound { .. }) => {
                    ui.error("No configuration found. Run 'bivvy init' first.");
                    return Ok(CommandResult::failure(2));
                }
                Err(e) => return Err(e),
            }
        };

        // Deprecation warning for --ci flag
        if self.args.ci {
            ui.warning(
                "--ci is deprecated and will be removed in 2.0. \
                 Use --non-interactive and --env ci instead.",
            );
        }

        // Collect and display config deprecation warnings
        let mut deprecation_warnings =
            crate::lint::rules::deprecated_fields::collect_deprecation_warnings(&config);

        // Scan raw YAML for alias-based deprecations (e.g., old field names)
        {
            let config_file_paths: Vec<std::path::PathBuf> =
                if let Some(ref p) = self.config_override {
                    vec![p.clone()]
                } else {
                    let paths = ConfigPaths::discover(&self.project_root);
                    paths
                        .all_existing()
                        .iter()
                        .map(|p| p.to_path_buf())
                        .collect()
                };
            let refs: Vec<&std::path::Path> =
                config_file_paths.iter().map(|p| p.as_path()).collect();
            deprecation_warnings.extend(
                crate::lint::rules::deprecated_fields::collect_raw_yaml_deprecation_warnings(&refs),
            );
        }

        for warning in &deprecation_warnings {
            ui.warning(warning);
        }

        // Apply config default_output when no CLI flag was explicitly set.
        // OutputMode::Normal means the user didn't pass --verbose or --quiet.
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.defaults.output.into());
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
            .environment_profiles
            .environments
            .get(&env_name)
            .map(|env_config| env_config.provided_requirements.iter().cloned().collect())
            .unwrap_or_default();

        // Use environment's default_workflow if no explicit --workflow and config has one
        let workflow_name = if self.args.workflow == "default" {
            config
                .settings
                .environment_profiles
                .environments
                .get(&env_name)
                .and_then(|env_config| env_config.default_workflow.clone())
                .unwrap_or_else(|| self.args.workflow.clone())
        } else {
            self.args.workflow.clone()
        };

        // Show header (suppressed when chaining from init)
        if !self.args.suppress_header {
            let app_name = config.app_name.as_deref().unwrap_or("project");
            let step_count = config
                .workflows
                .get(workflow_name.as_str())
                .map(|w| w.steps.len())
                .unwrap_or(0);
            ui.show_run_header(
                app_name,
                &workflow_name,
                step_count,
                crate::updates::version::VERSION,
            );

            // Show environment and config info
            ui.message(&format!(
                "  Environment: {} ({})",
                env_name, resolved_env.source
            ));
        }

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

        // Load state (with any v1→v2 baseline migrations)
        let (mut state, baseline_migrations) = StateStore::load(&project_id)?;

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
        let workflow_config = config.workflows.get(&workflow_name);
        let mut step_overrides = workflow_config
            .map(|w| w.overrides.clone())
            .unwrap_or_default();

        // Apply workflow-level auto_run_steps to step overrides (individual overrides take precedence)
        if let Some(wf) = workflow_config {
            if let Some(wf_auto_run) = wf.auto_run_steps {
                for step_name in &wf.steps {
                    let entry = step_overrides.entry(step_name.clone()).or_default();
                    if entry.auto_run.is_none() {
                        entry.auto_run = Some(wf_auto_run);
                    }
                }
            }
        }

        // Build run options with resolved workflow and provided requirements
        let mut options = self.build_options(&config);
        options.workflow = Some(workflow_name.clone());
        options.provided_requirements = provided_requirements;
        options.active_environment = Some(env_name.clone());

        // Create runner with project-backed snapshot store for change check baselines
        let mut snapshot_store = crate::snapshots::SnapshotStore::load_for_project(&project_id);

        // Apply v1 baseline migrations into snapshot store
        for migration in &baseline_migrations {
            use crate::snapshots::SnapshotKey;
            let key = SnapshotKey::project(&migration.step_name, "v1_migrated");
            snapshot_store.record_baseline(
                &key,
                "_last_run",
                migration.hash.clone(),
                format!(
                    "migrated from v1 watches_hash for step '{}'",
                    migration.step_name
                ),
            );
        }

        let mut runner = WorkflowRunner::with_snapshot_store(&config, steps, snapshot_store);

        // Create gap checker for requirement detection
        let probe = EnvironmentProbe::run();
        let req_registry = RequirementRegistry::new().with_custom(&config.requirements);
        let mut gap_checker = GapChecker::new(&req_registry, &probe, &self.project_root);

        // Evaluate user-defined vars
        let resolved_vars = evaluate_vars(&config.vars, &self.project_root)?;

        // Create interpolation context
        let ctx = InterpolationContext::new().with_vars(resolved_vars);
        let global_env: HashMap<String, String> = std::env::vars().collect();

        // Create event bus with logger for structured event logging
        let mut event_bus = crate::logging::EventBus::new();
        if config.settings.logging.logging {
            if let Ok(mut logger) = crate::logging::EventLogger::new(
                crate::logging::default_log_dir(),
                &format!(
                    "sess_{}_{}",
                    chrono::Utc::now().format("%Y%m%d%H%M%S"),
                    &project_id.hash()[..8]
                ),
                config.settings.logging.to_retention_policy(),
            ) {
                // Register sensitive steps for redaction
                let sensitive: Vec<String> = config
                    .steps
                    .iter()
                    .filter(|(_, s)| s.behavior.sensitive)
                    .map(|(name, _)| name.clone())
                    .collect();
                logger.set_sensitive_steps(sensitive);
                event_bus.add_consumer(Box::new(logger));
            }
        }

        // Emit session started
        event_bus.emit(&crate::logging::BivvyEvent::SessionStarted {
            command: "run".to_string(),
            args: vec![workflow_name.clone()],
            version: crate::updates::version::VERSION.to_string(),
            os: Some(std::env::consts::OS.to_string()),
            working_directory: Some(self.project_root.display().to_string()),
        });

        // Emit config loaded
        {
            let config_path = if let Some(ref override_path) = self.config_override {
                override_path.display().to_string()
            } else {
                let paths = ConfigPaths::discover(&self.project_root);
                paths
                    .project
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| ".bivvy/config.yml".to_string())
            };
            event_bus.emit(&crate::logging::BivvyEvent::ConfigLoaded {
                config_path,
                parse_duration_ms: None,
                deprecation_warnings: deprecation_warnings.clone(),
            });
        }

        // Initialize satisfaction cache (two-layer: persisted + runtime)
        let mut satisfaction_cache = if options.fresh {
            crate::state::SatisfactionCache::empty(project_id.satisfaction_path())
        } else {
            crate::state::SatisfactionCache::load(project_id.satisfaction_path())
        };

        // Run the workflow with UI-driven interactive prompts
        let result = runner.run_with_ui(
            &options,
            &ctx,
            &global_env,
            &self.project_root,
            workflow_non_interactive,
            &step_overrides,
            Some(&mut gap_checker),
            Some(&mut state),
            &mut satisfaction_cache,
            ui,
            &mut event_bus,
        )?;

        // Update state and snapshots (unless dry-run)
        if !self.args.dry_run {
            // Record step results into state store
            for step_result in &result.steps {
                let status = match step_result.status() {
                    crate::steps::StepStatus::Completed => crate::state::StepStatus::Success,
                    crate::steps::StepStatus::Failed => crate::state::StepStatus::Failed,
                    crate::steps::StepStatus::Skipped => crate::state::StepStatus::Skipped,
                    _ => crate::state::StepStatus::NeverRun,
                };
                state.record_step_result(&step_result.name, status, step_result.duration);
            }

            state.save(&project_id)?;

            // Save snapshot baselines accumulated during check evaluation
            if let Err(e) = runner.snapshot_store_mut().save() {
                tracing::warn!("Failed to save snapshot store: {}", e);
            }
        }

        // Build and show run summary
        let steps_satisfied = result
            .steps
            .iter()
            .filter(|s| {
                s.skipped
                    && s.check_result
                        .as_ref()
                        .map(|c| c.description.starts_with("satisfied:"))
                        .unwrap_or(false)
            })
            .count();
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
                            .map(|c| c.description.clone())
                            .unwrap_or_else(|| "already complete".to_string()),
                    )
                } else if s.success && s.check_result.is_some() && s.duration.as_millis() == 0 {
                    // check_passed result: show what check determined completion
                    Some(
                        s.check_result
                            .as_ref()
                            .map(|c| format!("Check passed ({})", c.description))
                            .unwrap_or_else(|| "Check passed".to_string()),
                    )
                } else if !s.success && !s.skipped {
                    s.error.clone()
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
            steps_satisfied,
            success: result.success,
            failed_steps: failed_steps.clone(),
        };

        ui.show_run_summary(&summary);

        // Show contextual hint and emit session ended
        let (exit_code, cmd_result) = if result.success {
            ui.show_hint(hints::after_successful_run());
            (0, CommandResult::success())
        } else if result.aborted {
            ui.show_hint("Workflow aborted by user. Re-run to resume.");
            (1, CommandResult::failure(1))
        } else {
            ui.show_hint(&hints::after_failed_run(&failed_steps));
            (1, CommandResult::failure(1))
        };

        event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
            exit_code,
            duration_ms: result.duration.as_millis() as u64,
        });

        Ok(cmd_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{MockUI, UiState};
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

        let options = cmd.build_options(&crate::config::BivvyConfig::default());

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
        let options = cmd.build_options(&crate::config::BivvyConfig::default());

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
            cmd.build_options(&crate::config::BivvyConfig::default())
                .skip_behavior,
            SkipBehavior::SkipWithDependents
        );

        // Skip only
        let args = RunArgs {
            skip_behavior: "skip_only".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        assert_eq!(
            cmd.build_options(&crate::config::BivvyConfig::default())
                .skip_behavior,
            SkipBehavior::SkipOnly
        );

        // Run anyway
        let args = RunArgs {
            skip_behavior: "run_anyway".to_string(),
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        assert_eq!(
            cmd.build_options(&crate::config::BivvyConfig::default())
                .skip_behavior,
            SkipBehavior::RunAnyway
        );
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
    fn execute_skipped_step_summary_shows_check_description() {
        let config = r#"
app_name: Test Project
steps:
  hello:
    command: echo hello
    check:
      type: execution
      command: "exit 0"
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        let summaries = ui.summaries();
        assert!(!summaries.is_empty());
        let step = &summaries[0].step_results[0];
        assert_eq!(step.status, StatusKind::Success);
        // Should show the check description with context
        let detail = step.detail.as_deref().unwrap();
        assert!(
            detail.contains("Check passed") && detail.contains("exit 0"),
            "expected 'Check passed' with check description in summary detail, got: {}",
            detail
        );
    }

    #[test]
    fn resolve_steps_uses_template_for_brew_bundle() {
        let config_yaml = r#"
app_name: test
steps:
  brew:
    template: brew-bundle
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
            !brew_step.execution.command.is_empty(),
            "template step should have a command from the brew-bundle template"
        );
        assert!(
            brew_step.execution.command.contains("brew"),
            "brew-bundle template command should mention brew, got: {}",
            brew_step.execution.command
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
        assert_eq!(hello_step.execution.command, "echo hello");
    }

    #[test]
    fn execute_dry_run_with_template_shows_real_command() {
        let config_yaml = r#"
app_name: test
steps:
  brew:
    template: brew-bundle
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
  defaults:
    output: quiet
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
  defaults:
    output: quiet
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

        let options = cmd.build_options(&crate::config::BivvyConfig::default());

        assert!(options.provided_requirements.is_empty());
    }

    #[test]
    fn resolve_environment_fallback_when_no_config() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        let config = crate::config::BivvyConfig::default();
        let resolved = cmd.resolve_environment(&config);

        // Without flag or config default, environment is either auto-detected
        // (e.g. "ci" in CI) or falls back to "development"
        match &resolved.source {
            crate::environment::resolver::EnvironmentSource::Fallback => {
                assert_eq!(resolved.name, "development");
            }
            crate::environment::resolver::EnvironmentSource::AutoDetected(_) => {
                // Auto-detected from env vars (e.g. CI=true in GitHub Actions)
            }
            other => panic!("Expected Fallback or AutoDetected, got {:?}", other),
        }
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
        config.settings.environment_profiles.default_environment = Some("production".to_string());
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
        config.settings.environment_profiles.default_environment = Some("production".to_string());
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
        assert_eq!(steps.get("hello").unwrap().execution.command, "echo hello");

        // With ci environment
        let steps = cmd.resolve_steps(&config, Some("ci")).unwrap();
        assert_eq!(
            steps.get("hello").unwrap().execution.command,
            "echo ci-hello"
        );
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
        ui.set_default_prompt_response("yes");
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
        ui.set_default_prompt_response("yes");
        ui.set_prompt_response("recovery_broken", "abort");

        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, 1);

        // Should show the aborted hint
        assert!(ui.has_hint("Workflow aborted by user"));
    }

    #[test]
    fn vars_available_in_step_commands() {
        let config = r#"
app_name: Test
vars:
  greeting: "hello-from-vars"
steps:
  greet:
    command: "echo ${greeting}"
workflows:
  default:
    steps: [greet]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn computed_vars_available_in_step_commands() {
        let config = r#"
app_name: Test
vars:
  computed_val:
    command: "echo computed-output"
steps:
  use_var:
    command: "echo ${computed_val}"
workflows:
  default:
    steps: [use_var]
"#;
        let temp = setup_project(config);
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
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

    #[test]
    fn build_options_diagnostic_funnel_defaults_to_config_value() {
        let temp = TempDir::new().unwrap();
        let args = RunArgs::default();
        let cmd = RunCommand::new(temp.path(), args);

        // Default config has diagnostic_funnel = true
        let config = crate::config::BivvyConfig::default();
        let options = cmd.build_options(&config);
        assert!(options.diagnostic_funnel);

        // Config with diagnostic_funnel = false
        let mut config_off = crate::config::BivvyConfig::default();
        config_off.settings.execution.diagnostic_funnel = false;
        let options = cmd.build_options(&config_off);
        assert!(!options.diagnostic_funnel);
    }

    #[test]
    fn build_options_diagnostic_funnel_cli_flag_overrides_config() {
        let temp = TempDir::new().unwrap();

        // --diagnostic-funnel forces on even when config says false
        let args = RunArgs {
            diagnostic_funnel: true,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let mut config = crate::config::BivvyConfig::default();
        config.settings.execution.diagnostic_funnel = false;
        let options = cmd.build_options(&config);
        assert!(options.diagnostic_funnel);

        // --no-diagnostic-funnel forces off even when config says true
        let args = RunArgs {
            no_diagnostic_funnel: true,
            ..Default::default()
        };
        let cmd = RunCommand::new(temp.path(), args);
        let config = crate::config::BivvyConfig::default();
        let options = cmd.build_options(&config);
        assert!(!options.diagnostic_funnel);
    }

    #[test]
    fn config_diagnostic_funnel_deserialization() {
        let yaml = r#"
app_name: test
settings:
  diagnostic_funnel: false
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let config: crate::config::BivvyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.settings.execution.diagnostic_funnel);

        // Omitted means true (default)
        let yaml_default = r#"
app_name: test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let config: crate::config::BivvyConfig = serde_yaml::from_str(yaml_default).unwrap();
        assert!(config.settings.execution.diagnostic_funnel);
    }
}
