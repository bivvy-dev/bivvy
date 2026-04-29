//! Lint command implementation.
//!
//! The `bivvy lint` command validates configuration files using the lint rule system.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cli::args::LintArgs;
use crate::config::{
    load_config, load_merged_config, load_project_config, load_single_step_file,
    load_single_workflow_file, BivvyConfig, ConfigPaths, Discovery,
};
use crate::error::{BivvyError, Result};
use crate::lint::{
    CircularRequirementDepRule, Fix, FixEngine, HumanFormatter, InstallTemplateMissingRule,
    JsonFormatter, LintDiagnostic, LintFormatter, RuleRegistry, SarifFormatter,
    ServiceRequirementWithoutHintRule, Severity, TemplateInputsRule, UndefinedTemplateRule,
    UnknownRequirementRule,
};
use crate::registry::Registry;
use crate::requirements::registry::RequirementRegistry;
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// What the user asked to lint.
enum LintTarget {
    /// Bare invocation or `--config`: lint `.bivvy/config.yml` only.
    ProjectConfig,
    /// `--workflow <name>` or positional that resolved to a workflow file.
    WorkflowFile(String),
    /// `--step <name>` or positional that resolved to a step file.
    StepFile(String),
    /// `--all`: full merged config (legacy behavior).
    All,
}

/// The lint command implementation.
pub struct LintCommand {
    project_root: PathBuf,
    args: LintArgs,
    config_override: Option<PathBuf>,
}

impl LintCommand {
    /// Create a new lint command.
    pub fn new(project_root: &Path, args: LintArgs) -> Self {
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
    pub fn args(&self) -> &LintArgs {
        &self.args
    }

    /// Run all lint rules and collect diagnostics.
    fn run_rules(
        &self,
        registry: &RuleRegistry,
        config: &crate::config::BivvyConfig,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        for rule in registry.iter() {
            diagnostics.extend(rule.check(config));
        }
        diagnostics
    }

    /// Resolve which target the user asked to lint.
    fn resolve_target(&self) -> Result<LintTarget> {
        if self.config_override.is_some() {
            // Explicit config file override: lint just that file via load_config.
            return Ok(LintTarget::ProjectConfig);
        }
        if let Some(ref name) = self.args.workflow {
            return Ok(LintTarget::WorkflowFile(name.clone()));
        }
        if let Some(ref name) = self.args.step {
            return Ok(LintTarget::StepFile(name.clone()));
        }
        if self.args.all {
            return Ok(LintTarget::All);
        }
        if self.args.config {
            return Ok(LintTarget::ProjectConfig);
        }
        if let Some(ref name) = self.args.target {
            let discovery = Discovery::new(&self.project_root);
            if discovery.workflow_path(name).is_some() {
                if discovery.step_path(name).is_some() {
                    // Both exist (rare) — pick workflow but surface a hint.
                    return Ok(LintTarget::WorkflowFile(name.clone()));
                }
                return Ok(LintTarget::WorkflowFile(name.clone()));
            }
            if discovery.step_path(name).is_some() {
                return Ok(LintTarget::StepFile(name.clone()));
            }
            return Err(BivvyError::ConfigValidationError {
                message: format!(
                    "Unknown lint target: {name}. No file found at \
                     .bivvy/workflows/{name}.yml or .bivvy/steps/{name}.yml"
                ),
            });
        }
        Ok(LintTarget::ProjectConfig)
    }

    /// Build the [`BivvyConfig`] view to lint plus the source paths it draws from.
    fn build_target_config(&self, target: &LintTarget) -> Result<(BivvyConfig, Vec<PathBuf>)> {
        if let Some(ref override_path) = self.config_override {
            // Explicit override: just load that file in isolation.
            let cfg = load_config(&self.project_root, Some(override_path))?;
            return Ok((cfg, vec![override_path.clone()]));
        }

        match target {
            LintTarget::ProjectConfig => {
                let cfg = load_project_config(&self.project_root)?;
                let path = self.project_root.join(".bivvy").join("config.yml");
                Ok((cfg, vec![path]))
            }
            LintTarget::WorkflowFile(name) => {
                let discovery = Discovery::new(&self.project_root);
                let workflow_path =
                    discovery
                        .workflow_path(name)
                        .ok_or_else(|| BivvyError::ConfigNotFound {
                            path: self
                                .project_root
                                .join(".bivvy")
                                .join("workflows")
                                .join(format!("{name}.yml")),
                        })?;

                // Project file gives us context (settings, templates, custom
                // requirements). Missing project file is fine — fall back to
                // a default config so we can still lint the workflow file.
                let mut cfg = match load_project_config(&self.project_root) {
                    Ok(c) => c,
                    Err(BivvyError::ConfigNotFound { .. }) => BivvyConfig::default(),
                    Err(e) => return Err(e),
                };

                let workflow_file = load_single_workflow_file(&workflow_path)?;

                // Replace workflows with just the named one so cross-workflow
                // rules don't fire on workflows we aren't targeting.
                let mut workflow = workflow_file.workflow.clone();
                if workflow.description.is_none() {
                    workflow.description = workflow_file.description.clone();
                }
                cfg.workflows = HashMap::new();
                cfg.workflows.insert(name.clone(), workflow);

                // Splice in steps and vars from the workflow file.
                for (step_name, step_config) in workflow_file.steps {
                    cfg.steps.insert(step_name, step_config);
                }
                for (var_name, var_def) in workflow_file.vars {
                    cfg.vars.insert(var_name, var_def);
                }
                cfg.migrate_deprecated_fields();

                let mut paths = vec![workflow_path];
                let project_path = self.project_root.join(".bivvy").join("config.yml");
                if project_path.exists() {
                    paths.push(project_path);
                }
                Ok((cfg, paths))
            }
            LintTarget::StepFile(name) => {
                let discovery = Discovery::new(&self.project_root);
                let step_path =
                    discovery
                        .step_path(name)
                        .ok_or_else(|| BivvyError::ConfigNotFound {
                            path: self
                                .project_root
                                .join(".bivvy")
                                .join("steps")
                                .join(format!("{name}.yml")),
                        })?;

                let mut cfg = match load_project_config(&self.project_root) {
                    Ok(c) => c,
                    Err(BivvyError::ConfigNotFound { .. }) => BivvyConfig::default(),
                    Err(e) => return Err(e),
                };

                let step_config = load_single_step_file(&step_path)?;
                cfg.steps.insert(name.clone(), step_config);
                cfg.migrate_deprecated_fields();

                let mut paths = vec![step_path];
                let project_path = self.project_root.join(".bivvy").join("config.yml");
                if project_path.exists() {
                    paths.push(project_path);
                }
                Ok((cfg, paths))
            }
            LintTarget::All => {
                let cfg = load_merged_config(&self.project_root)?;
                let discovered = ConfigPaths::discover(&self.project_root);
                let mut paths: Vec<PathBuf> = discovered
                    .all_existing()
                    .iter()
                    .map(|p| (*p).clone())
                    .collect();
                paths.extend(discovered.split_steps.iter().cloned());
                paths.extend(discovered.split_workflows.iter().cloned());
                Ok((cfg, paths))
            }
        }
    }

    /// Format diagnostics using the appropriate formatter.
    fn format_output(&self, diagnostics: &[LintDiagnostic]) -> String {
        let mut output = Vec::new();

        match self.args.format.as_str() {
            "json" => {
                let formatter = JsonFormatter::new();
                formatter.format(diagnostics, &mut output).ok();
            }
            "sarif" => {
                let formatter = SarifFormatter::new("bivvy", env!("CARGO_PKG_VERSION"));
                formatter.format(diagnostics, &mut output).ok();
            }
            _ => {
                let formatter = HumanFormatter::new(true);
                formatter.format(diagnostics, &mut output).ok();
            }
        }

        String::from_utf8(output).unwrap_or_default()
    }
}

impl Command for LintCommand {
    fn execute(&self, ui: &mut dyn UserInterface) -> Result<CommandResult> {
        // Create event bus for structured logging
        let mut event_bus = crate::logging::EventBus::new();
        if let Ok(logger) = crate::logging::EventLogger::new(
            crate::logging::default_log_dir(),
            &format!("sess_{}_lint", chrono::Utc::now().format("%Y%m%d%H%M%S"),),
            crate::logging::RetentionPolicy::default(),
        ) {
            event_bus.add_consumer(Box::new(logger));
        }
        let start = std::time::Instant::now();

        event_bus.emit(&crate::logging::BivvyEvent::SessionStarted {
            command: "lint".to_string(),
            args: vec![
                format!("--format={}", self.args.format),
                if self.args.strict {
                    "--strict".to_string()
                } else {
                    String::new()
                },
                if self.args.fix {
                    "--fix".to_string()
                } else {
                    String::new()
                },
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: Some(std::env::consts::OS.to_string()),
            working_directory: Some(self.project_root.display().to_string()),
        });

        // Check if config exists (skip check when override is provided)
        if self.config_override.is_none() {
            let paths = ConfigPaths::discover(&self.project_root);
            if !paths.has_project_config() {
                ui.error("No configuration found. Run 'bivvy init' first.");
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 2,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(2));
            }
        }

        // Resolve which target the user asked to lint.
        let target = match self.resolve_target() {
            Ok(t) => t,
            Err(BivvyError::ConfigValidationError { message }) => {
                ui.error(&message);
                let discovery = Discovery::new(&self.project_root);
                let workflows = discovery.workflow_names();
                let steps = discovery.step_file_names();
                if !workflows.is_empty() {
                    ui.message(&format!("Available workflows: {}", workflows.join(", ")));
                }
                if !steps.is_empty() {
                    ui.message(&format!("Available steps: {}", steps.join(", ")));
                }
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 1,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(1));
            }
            Err(e) => return Err(e),
        };

        // Build the BivvyConfig view to lint along with the file paths
        // we actually consulted (used for raw-YAML deprecation scanning).
        let (config, lint_file_paths) = match self.build_target_config(&target) {
            Ok(pair) => pair,
            Err(BivvyError::ConfigParseError { path, message }) => {
                ui.error(&format!("Parse error in {}: {}", path.display(), message));
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 1,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(1));
            }
            Err(BivvyError::ConfigNotFound { path }) => {
                ui.error(&format!("File not found: {}", path.display()));
                event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                    exit_code: 1,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                return Ok(CommandResult::failure(1));
            }
            Err(e) => return Err(e),
        };

        let mut deprecation_warnings =
            crate::lint::rules::deprecated_fields::collect_deprecation_warnings(&config);

        // Scan raw YAML for alias-based deprecations (e.g., old field names)
        {
            let refs: Vec<&std::path::Path> = lint_file_paths.iter().map(|p| p.as_path()).collect();
            deprecation_warnings.extend(
                crate::lint::rules::deprecated_fields::collect_raw_yaml_deprecation_warnings(&refs),
            );
        }

        // Display deprecation warnings to the user
        for warning in &deprecation_warnings {
            ui.warning(warning);
        }

        event_bus.emit(&crate::logging::BivvyEvent::ConfigLoaded {
            config_path: self
                .config_override
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".bivvy/config.yml".to_string()),
            parse_duration_ms: None,
            deprecation_warnings,
        });

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.defaults.output.into());
        }

        // Create rule registry with built-in rules
        let mut rule_registry = RuleRegistry::with_builtins();

        // Add template-related rules if we can load the template registry
        let template_registry_result = if config.template_sources.is_empty() {
            Registry::new(Some(&self.project_root))
        } else {
            Registry::with_remote_sources(Some(&self.project_root), &config.template_sources)
        };
        if let Ok(template_registry) = template_registry_result {
            rule_registry.register(Box::new(UndefinedTemplateRule::new(
                template_registry.clone(),
            )));
            rule_registry.register(Box::new(TemplateInputsRule::new(template_registry)));
        }

        // Add requirement-related rules
        // Each rule takes ownership of its own RequirementRegistry instance
        let make_req_registry = || RequirementRegistry::new().with_custom(&config.requirements);
        rule_registry.register(Box::new(UnknownRequirementRule::new(make_req_registry())));
        rule_registry.register(Box::new(CircularRequirementDepRule::new(
            make_req_registry(),
        )));
        rule_registry.register(Box::new(InstallTemplateMissingRule::new(
            make_req_registry(),
        )));
        rule_registry.register(Box::new(ServiceRequirementWithoutHintRule::new(
            make_req_registry(),
        )));

        // Run all lint rules
        let mut diagnostics = self.run_rules(&rule_registry, &config);

        // Apply fixes if requested
        if self.args.fix {
            let fixes: Vec<Fix> = diagnostics
                .iter()
                .filter_map(|d| {
                    // Only create fixes for diagnostics that have suggestions and spans
                    match (&d.suggestion, &d.span) {
                        (Some(suggestion), Some(span)) => Some(Fix {
                            file: span.file.clone(),
                            start: 0, // Would need actual byte offsets from marked_yaml
                            end: 0,
                            replacement: suggestion.clone(),
                        }),
                        _ => None,
                    }
                })
                .collect();

            if !fixes.is_empty() {
                let engine = FixEngine::new();
                let result = engine.apply_fixes(&diagnostics, &fixes);
                if result.applied > 0 {
                    ui.success(&format!("Applied {} fix(es)", result.applied));
                    // Re-run rules after fixes
                    diagnostics = self.run_rules(&rule_registry, &config);
                }
            }
        }

        // Evaluate checks defined in config and emit CheckEvaluated events
        {
            let ctx = crate::config::interpolation::InterpolationContext::default();
            let mut snapshot_store = crate::snapshots::SnapshotStore::empty();
            for (step_name, step_config) in &config.steps {
                if let Some(ref check) = step_config.execution.check {
                    let mut evaluator = crate::checks::evaluator::CheckEvaluator::new(
                        &self.project_root,
                        &ctx,
                        &mut snapshot_store,
                    );
                    let result = evaluator.evaluate(check);
                    event_bus.emit(&crate::logging::BivvyEvent::CheckEvaluated {
                        step: step_name.clone(),
                        check_name: check.name().map(|s| s.to_string()),
                        check_type: check.type_name().to_string(),
                        outcome: result.outcome.as_str().to_string(),
                        description: result.description.clone(),
                        details: result.details.clone(),
                        duration_ms: None,
                    });
                }
            }
        }

        // Check for errors based on strict mode
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
        let has_warnings = diagnostics.iter().any(|d| d.severity == Severity::Warning);
        let should_fail = has_errors || (self.args.strict && has_warnings);

        if diagnostics.is_empty() {
            if self.args.format == "human" {
                ui.success("Configuration is valid!");
            } else {
                // For JSON/SARIF, still output the formatted result (empty diagnostics)
                let output = self.format_output(&diagnostics);
                ui.message(&output);
            }
            event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
                exit_code: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            });
            return Ok(CommandResult::success());
        }

        // Output diagnostics in the requested format
        let output = self.format_output(&diagnostics);

        // For human format, we need to write each line
        if self.args.format == "human" {
            for line in output.lines() {
                if line.starts_with("error") {
                    ui.error(line);
                } else if line.starts_with("warning") {
                    ui.warning(line);
                } else {
                    ui.message(line);
                }
            }
        } else {
            // For JSON/SARIF, output as-is
            ui.message(&output);
        }

        let (exit_code, result) = if should_fail {
            (1, CommandResult::failure(1))
        } else {
            (0, CommandResult::success())
        };
        event_bus.emit(&crate::logging::BivvyEvent::SessionEnded {
            exit_code,
            duration_ms: start.elapsed().as_millis() as u64,
        });
        Ok(result)
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
    fn lint_command_creation() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);

        assert_eq!(cmd.project_root(), temp.path());
    }

    #[test]
    fn lint_no_config() {
        let temp = TempDir::new().unwrap();
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn lint_valid_config() {
        let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn lint_applies_config_default_output() {
        let config = r#"
app_name: test-app
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
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        cmd.execute(&mut ui).unwrap();

        assert_eq!(ui.output_mode(), crate::ui::OutputMode::Quiet);
    }

    #[test]
    fn lint_invalid_config_circular_dependency() {
        let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [b]
  b:
    command: echo b
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
    }

    #[test]
    fn lint_detects_self_dependency() {
        let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [a]
workflows:
  default:
    steps: [a]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
    }

    #[test]
    fn lint_detects_undefined_dependency() {
        let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [nonexistent]
workflows:
  default:
    steps: [a]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(!result.success);
    }

    #[test]
    fn lint_json_format() {
        let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            format: "json".to_string(),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn lint_sarif_format() {
        let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            format: "sarif".to_string(),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        assert!(result.success);
    }

    #[test]
    fn lint_strict_mode_fails_on_warnings() {
        let config = r#"
app_name: My App With Spaces
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs {
            strict: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        // App name with spaces produces a warning
        assert!(!result.success);
    }

    #[test]
    fn lint_without_strict_passes_on_warnings() {
        let config = r#"
app_name: My App With Spaces
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
        let temp = setup_project(config);
        let args = LintArgs::default();
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();

        // Without strict mode, warnings don't cause failure
        assert!(result.success);
    }

    #[test]
    fn lint_targeted_workflow_does_not_parse_other_workflow_files() {
        // A malformed sibling workflow file must NOT block targeted lint of
        // a different workflow.
        let temp = setup_project("app_name: Test\n");
        let workflows_dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(
            workflows_dir.join("good.yml"),
            r#"
steps:
  hello:
    command: echo hello
workflow:
  steps:
    - hello
"#,
        )
        .unwrap();
        fs::write(
            workflows_dir.join("broken.yml"),
            "this: is: not: valid: yaml: at all",
        )
        .unwrap();

        let args = LintArgs {
            target: Some("good".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
    }

    #[test]
    fn lint_unknown_target_errors_with_available_list() {
        let temp = setup_project("app_name: Test\n");
        let workflows_dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(workflows_dir.join("ci.yml"), "steps: []").unwrap();

        let args = LintArgs {
            target: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();

        let result = cmd.execute(&mut ui).unwrap();
        assert!(!result.success);
        assert!(ui
            .messages()
            .iter()
            .chain(ui.errors().iter())
            .any(|m| m.contains("nonexistent")));
    }

    #[test]
    fn lint_step_target_loads_step_file() {
        let temp = setup_project(
            r#"
app_name: Test
steps:
  other:
    command: "echo other"
"#,
        );
        let steps_dir = temp.path().join(".bivvy").join("steps");
        fs::create_dir_all(&steps_dir).unwrap();
        fs::write(
            steps_dir.join("deps.yml"),
            "command: yarn install\ntitle: Install deps\n",
        )
        .unwrap();

        let args = LintArgs {
            target: Some("deps".to_string()),
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
    }

    #[test]
    fn lint_all_flag_uses_full_merge() {
        let temp = setup_project(
            r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
        );
        let workflows_dir = temp.path().join(".bivvy").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(
            workflows_dir.join("ci.yml"),
            "description: CI\nsteps: [hello]\n",
        )
        .unwrap();

        let args = LintArgs {
            all: true,
            ..Default::default()
        };
        let cmd = LintCommand::new(temp.path(), args);
        let mut ui = MockUI::new();
        let result = cmd.execute(&mut ui).unwrap();
        assert!(result.success);
    }
}
