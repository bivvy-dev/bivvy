//! Lint command implementation.
//!
//! The `bivvy lint` command validates configuration files using the lint rule system.

use std::path::{Path, PathBuf};

use crate::cli::args::LintArgs;
use crate::config::{load_merged_config, ConfigPaths};
use crate::error::{BivvyError, Result};
use crate::lint::{
    Fix, FixEngine, HumanFormatter, JsonFormatter, LintDiagnostic, LintFormatter, RuleRegistry,
    SarifFormatter, Severity, TemplateInputsRule, UndefinedTemplateRule,
};
use crate::registry::Registry;
use crate::ui::{OutputMode, UserInterface};

use super::dispatcher::{Command, CommandResult};

/// The lint command implementation.
pub struct LintCommand {
    project_root: PathBuf,
    args: LintArgs,
}

impl LintCommand {
    /// Create a new lint command.
    pub fn new(project_root: &Path, args: LintArgs) -> Self {
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
        // Check if config exists
        let paths = ConfigPaths::discover(&self.project_root);
        if !paths.has_project_config() {
            ui.error("No configuration found. Run 'bivvy init' first.");
            return Ok(CommandResult::failure(2));
        }

        // Load configuration
        let config = match load_merged_config(&self.project_root) {
            Ok(c) => c,
            Err(BivvyError::ConfigParseError { path, message }) => {
                ui.error(&format!("Parse error in {}: {}", path.display(), message));
                return Ok(CommandResult::failure(1));
            }
            Err(e) => return Err(e),
        };

        // Apply config default_output when no CLI flag was explicitly set
        if ui.output_mode() == OutputMode::Normal {
            ui.set_output_mode(config.settings.default_output.into());
        }

        // Create rule registry with built-in rules
        let mut rule_registry = RuleRegistry::with_builtins();

        // Add template-related rules if we can load the template registry
        if let Ok(template_registry) = Registry::new(Some(&self.project_root)) {
            rule_registry.register(Box::new(UndefinedTemplateRule::new(
                template_registry.clone(),
            )));
            rule_registry.register(Box::new(TemplateInputsRule::new(template_registry)));
        }

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

        if should_fail {
            Ok(CommandResult::failure(1))
        } else {
            Ok(CommandResult::success())
        }
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
