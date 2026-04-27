//! Lint command implementation.
//!
//! The `bivvy lint` command validates configuration files using the lint rule system.

use std::path::{Path, PathBuf};

use crate::cli::args::LintArgs;
use crate::config::{load_config, ConfigPaths};
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

        // Load configuration
        let config = match load_config(&self.project_root, self.config_override.as_deref()) {
            Ok(c) => c,
            Err(BivvyError::ConfigParseError { path, message }) => {
                ui.error(&format!("Parse error in {}: {}", path.display(), message));
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
            ui.set_output_mode(config.settings.output.default_output.into());
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
  default_output: quiet
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
}
