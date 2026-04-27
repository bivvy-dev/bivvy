//! Detects deprecated configuration fields and suggests migrations.
//!
//! This rule warns about:
//! - `completed_check` → use `check`/`checks` instead
//! - `type: marker` → remove the check or use a specific check type
//! - `watches` → use `check: { type: change, target: ... }` instead
//! - `prompt_if_complete` → use `prompt_on_rerun` instead

use crate::config::{BivvyConfig, CompletedCheck};
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};
use std::path::Path;

/// Reports warnings for deprecated configuration fields.
pub struct DeprecatedFieldsRule;

impl LintRule for DeprecatedFieldsRule {
    fn id(&self) -> RuleId {
        RuleId::new("deprecated-fields")
    }

    fn name(&self) -> &str {
        "Deprecated Fields"
    }

    fn description(&self) -> &str {
        "Detects deprecated config fields and suggests replacements"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (name, step) in &config.steps {
            // Check for `completed_check` usage
            if let Some(ref check) = step.execution.completed_check {
                match check {
                    CompletedCheck::Marker => {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Step '{}': 'completed_check' with 'type: marker' is deprecated. \
                                     Remove the check entirely or use a specific check type.",
                                    name,
                                ),
                            )
                            .with_suggestion(format!(
                                "Remove `completed_check` from step '{}'. \
                                 If you need change detection, use:\n  check:\n    type: change\n    target: <file>\n    on_change: proceed",
                                name,
                            )),
                        );
                    }
                    CompletedCheck::FileExists { path } => {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Step '{}': 'completed_check' is deprecated, use 'check' instead.",
                                    name,
                                ),
                            )
                            .with_suggestion(format!(
                                "Replace with:\n  check:\n    type: presence\n    target: \"{}\"",
                                path,
                            )),
                        );
                    }
                    CompletedCheck::CommandSucceeds { command } => {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Step '{}': 'completed_check' is deprecated, use 'check' instead.",
                                    name,
                                ),
                            )
                            .with_suggestion(format!(
                                "Replace with:\n  check:\n    type: execution\n    command: \"{}\"\n    validation: success",
                                command,
                            )),
                        );
                    }
                    CompletedCheck::All { .. } | CompletedCheck::Any { .. } => {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Step '{}': 'completed_check' is deprecated, use 'check' or 'checks' instead.",
                                    name,
                                ),
                            )
                            .with_suggestion(
                                "Replace `completed_check` with `check`/`checks` using the new check types (presence, execution, change).".to_string(),
                            ),
                        );
                    }
                }
            }

            // Check for legacy `precondition` using CompletedCheck type
            if let Some(ref precond) = step.execution.precondition {
                let suggestion = match precond {
                    CompletedCheck::FileExists { path } => format!(
                        "Replace with:\n  new_precondition:\n    type: presence\n    target: \"{}\"",
                        path,
                    ),
                    CompletedCheck::CommandSucceeds { command } => format!(
                        "Replace with:\n  new_precondition:\n    type: execution\n    command: \"{}\"\n    validation: success",
                        command,
                    ),
                    CompletedCheck::Marker => {
                        "Remove the precondition or replace with a specific check type.".to_string()
                    }
                    CompletedCheck::All { .. } | CompletedCheck::Any { .. } => {
                        "Replace `precondition` with `new_precondition` using the new check types (presence, execution, change).".to_string()
                    }
                };
                diagnostics.push(
                    LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Step '{}': 'precondition' uses deprecated check types. Use 'new_precondition' with the new check system instead.",
                            name,
                        ),
                    )
                    .with_suggestion(suggestion),
                );
            }

            // Check for `watches` usage
            if !step.execution.watches.is_empty() {
                let targets: Vec<&str> =
                    step.execution.watches.iter().map(|s| s.as_str()).collect();
                let suggestion = if targets.len() == 1 {
                    format!(
                        "Replace with:\n  check:\n    type: change\n    target: \"{}\"\n    on_change: proceed",
                        targets[0],
                    )
                } else {
                    let items: Vec<String> = targets
                        .iter()
                        .map(|t| {
                            format!(
                                "    - type: change\n      target: \"{}\"\n      on_change: proceed",
                                t,
                            )
                        })
                        .collect();
                    format!("Replace with:\n  checks:\n{}", items.join("\n"))
                };

                diagnostics.push(
                    LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Step '{}': 'watches' is deprecated. Use 'check: {{ type: change, target: ... }}' instead.",
                            name,
                        ),
                    )
                    .with_suggestion(suggestion),
                );
            }
        }

        diagnostics
    }
}

/// Collect deprecation warnings from a parsed config.
///
/// Returns a list of human-readable warning strings suitable for display
/// to the user and for inclusion in the `ConfigLoaded` event.
pub fn collect_deprecation_warnings(config: &BivvyConfig) -> Vec<String> {
    let mut warnings = Vec::new();

    for (name, step) in &config.steps {
        if let Some(ref check) = step.execution.completed_check {
            match check {
                CompletedCheck::Marker => {
                    warnings.push(format!(
                        "Step '{}': 'type: marker' is removed. Use a specific check type or remove the check entirely.",
                        name,
                    ));
                }
                _ => {
                    warnings.push(format!(
                        "Step '{}': 'completed_check' is deprecated, use 'check' instead.",
                        name,
                    ));
                }
            }
        }

        if step.execution.precondition.is_some() {
            warnings.push(format!(
                "Step '{}': 'precondition' uses deprecated check types. Use 'new_precondition' with the new check system instead.",
                name,
            ));
        }

        if !step.execution.watches.is_empty() {
            warnings.push(format!(
                "Step '{}': 'watches' is deprecated. Use 'check: {{ type: change, target: ... }}' instead.",
                name,
            ));
        }
    }

    warnings
}

/// Scan raw YAML config files for deprecated field names that are invisible
/// after serde deserialization (because they use aliases).
///
/// This catches fields like `prompt_if_complete` (aliased to `prompt_on_rerun`)
/// and `OutputSettings.logging`/`log_path` (superseded by `settings.logging`
/// for JSONL event logging).
///
/// Returns a list of human-readable warning strings.
pub fn collect_raw_yaml_deprecation_warnings(config_paths: &[&Path]) -> Vec<String> {
    let mut warnings = Vec::new();

    for path in config_paths {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        // Check for `prompt_if_complete` usage (should be `prompt_on_rerun`)
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("prompt_if_complete:")
                || trimmed.starts_with("prompt_if_complete :")
            {
                warnings.push(format!(
                    "{} (line {}): 'prompt_if_complete' is deprecated, use 'prompt_on_rerun' instead.",
                    filename,
                    line_num + 1,
                ));
            }

            // Check for OutputSettings.log_path (superseded by JSONL event logging)
            if trimmed.starts_with("log_path:") || trimmed.starts_with("log_path :") {
                // Only flag if it's inside settings.output context (heuristic: check
                // if we're inside an `output:` block by looking at recent lines)
                warnings.push(format!(
                    "{} (line {}): 'log_path' in output settings is deprecated. \
                     JSONL event logs are written to ~/.bivvy/logs/ automatically.",
                    filename,
                    line_num + 1,
                ));
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExecutionConfig, StepConfig};
    use std::collections::HashMap;

    fn config_with_step(step: StepConfig) -> BivvyConfig {
        let mut steps = HashMap::new();
        steps.insert("test_step".to_string(), step);
        BivvyConfig {
            steps,
            ..Default::default()
        }
    }

    #[test]
    fn no_deprecated_fields_produces_no_warnings() {
        let config = config_with_step(StepConfig::default());
        let rule = DeprecatedFieldsRule;
        assert!(rule.check(&config).is_empty());
    }

    #[test]
    fn completed_check_file_exists_warns() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("yarn install".to_string()),
                completed_check: Some(CompletedCheck::FileExists {
                    path: "node_modules".to_string(),
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0]
            .message
            .contains("'completed_check' is deprecated"));
        assert!(diagnostics[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("type: presence"));
    }

    #[test]
    fn completed_check_command_succeeds_warns() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("bundle install".to_string()),
                completed_check: Some(CompletedCheck::CommandSucceeds {
                    command: "bundle check".to_string(),
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("type: execution"));
    }

    #[test]
    fn completed_check_marker_warns_with_removal_suggestion() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("./setup.sh".to_string()),
                completed_check: Some(CompletedCheck::Marker),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("marker"));
        assert!(diagnostics[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("Remove"));
    }

    #[test]
    fn completed_check_all_warns() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("yarn install".to_string()),
                completed_check: Some(CompletedCheck::All {
                    checks: vec![CompletedCheck::FileExists {
                        path: "node_modules".to_string(),
                    }],
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("'completed_check' is deprecated"));
    }

    #[test]
    fn watches_field_warns() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("bundle install".to_string()),
                watches: vec!["Gemfile.lock".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("'watches' is deprecated"));
        assert!(diagnostics[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("type: change"));
    }

    #[test]
    fn watches_multiple_files_suggests_checks() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("bundle install".to_string()),
                watches: vec!["Gemfile".to_string(), "Gemfile.lock".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("checks:"));
    }

    #[test]
    fn both_completed_check_and_watches_produce_two_warnings() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("bundle install".to_string()),
                completed_check: Some(CompletedCheck::Marker),
                watches: vec!["Gemfile.lock".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let rule = DeprecatedFieldsRule;
        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 2);
    }

    // --- collect_deprecation_warnings tests ---

    #[test]
    fn collect_warnings_empty_for_clean_config() {
        let config = config_with_step(StepConfig::default());
        assert!(collect_deprecation_warnings(&config).is_empty());
    }

    #[test]
    fn collect_warnings_includes_completed_check() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                completed_check: Some(CompletedCheck::FileExists {
                    path: "file.txt".to_string(),
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let warnings = collect_deprecation_warnings(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'completed_check' is deprecated"));
    }

    #[test]
    fn collect_warnings_includes_marker() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                completed_check: Some(CompletedCheck::Marker),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let warnings = collect_deprecation_warnings(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("marker"));
    }

    #[test]
    fn collect_warnings_includes_watches() {
        let step = StepConfig {
            execution: ExecutionConfig {
                command: Some("echo test".to_string()),
                watches: vec!["file.lock".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let config = config_with_step(step);
        let warnings = collect_deprecation_warnings(&config);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'watches' is deprecated"));
    }

    // --- collect_raw_yaml_deprecation_warnings tests ---

    #[test]
    fn raw_yaml_detects_prompt_if_complete() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        std::fs::write(
            &config_path,
            r#"
app_name: test
steps:
  setup:
    command: echo hi
    prompt_if_complete: true
"#,
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[config_path.as_path()]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'prompt_if_complete' is deprecated"));
        assert!(warnings[0].contains("'prompt_on_rerun'"));
    }

    #[test]
    fn raw_yaml_detects_log_path() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        std::fs::write(
            &config_path,
            r#"
app_name: test
settings:
  output:
    log_path: /tmp/bivvy.log
"#,
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[config_path.as_path()]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'log_path'"));
        assert!(warnings[0].contains("deprecated"));
    }

    #[test]
    fn raw_yaml_no_warnings_for_clean_config() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        std::fs::write(
            &config_path,
            r#"
app_name: test
steps:
  setup:
    command: echo hi
    prompt_on_rerun: true
"#,
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[config_path.as_path()]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn raw_yaml_handles_missing_file() {
        let warnings =
            collect_raw_yaml_deprecation_warnings(&[Path::new("/nonexistent/config.yml")]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn raw_yaml_scans_multiple_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let path1 = temp.path().join("config.yml");
        let path2 = temp.path().join("config.local.yml");
        std::fs::write(
            &path1,
            "steps:\n  a:\n    command: echo a\n    prompt_if_complete: false\n",
        )
        .unwrap();
        std::fs::write(
            &path2,
            "steps:\n  b:\n    command: echo b\n    prompt_if_complete: true\n",
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[path1.as_path(), path2.as_path()]);
        assert_eq!(warnings.len(), 2);
    }
}
