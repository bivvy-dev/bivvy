//! Detects deprecated configuration fields via raw YAML scanning.
//!
//! Since old config types (`CompletedCheck`, `watches`, etc.) have been removed,
//! this rule only scans raw YAML text for deprecated field names that would
//! otherwise be silently ignored by serde deserialization:
//! - `completed_check` → use `check`/`checks` instead
//! - `type: marker` → remove the check or use a specific check type
//! - `type: file_exists` → use `type: presence` instead
//! - `type: command_succeeds` → use `type: execution` instead
//! - `watches` → use `check: { type: change, target: ... }` instead
//! - `prompt_if_complete` → use `prompt_on_rerun` instead
//! - `log_path` → removed, JSONL logs go to ~/.bivvy/logs/

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};
use std::path::Path;

/// Reports warnings for deprecated configuration fields.
///
/// Since old types have been fully removed from the schema, the `LintRule::check`
/// implementation has no structured fields to inspect. All deprecated field
/// detection is done via raw YAML scanning in [`collect_raw_yaml_deprecation_warnings`].
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

    fn check(&self, _config: &BivvyConfig) -> Vec<LintDiagnostic> {
        // All deprecated fields have been removed from the typed schema.
        // Detection is now done via raw YAML scanning in
        // collect_raw_yaml_deprecation_warnings().
        Vec::new()
    }
}

/// Collect deprecation warnings from a parsed config.
///
/// Returns an empty list since all deprecated fields have been removed from
/// the typed schema. Kept for API compatibility with callers.
pub fn collect_deprecation_warnings(_config: &BivvyConfig) -> Vec<String> {
    Vec::new()
}

/// Scan raw YAML config files for deprecated field names.
///
/// This catches fields that serde would silently ignore because they no longer
/// exist in the typed config structs. Returns human-readable warning strings.
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

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // `completed_check` → use `check`/`checks`
            if trimmed.starts_with("completed_check:") || trimmed.starts_with("completed_check :") {
                warnings.push(format!(
                    "{} (line {}): 'completed_check' is deprecated, use 'check' or 'checks' instead.",
                    filename,
                    line_num + 1,
                ));
            }

            // `type: marker` → removed
            if trimmed == "type: marker" || trimmed == "type:marker" {
                warnings.push(format!(
                    "{} (line {}): 'type: marker' is removed. Use a specific check type or remove the check entirely.",
                    filename,
                    line_num + 1,
                ));
            }

            // `type: file_exists` → use `type: presence`
            if trimmed == "type: file_exists" || trimmed == "type:file_exists" {
                warnings.push(format!(
                    "{} (line {}): 'type: file_exists' is deprecated, use 'type: presence' instead.",
                    filename,
                    line_num + 1,
                ));
            }

            // `type: command_succeeds` → use `type: execution`
            if trimmed == "type: command_succeeds" || trimmed == "type:command_succeeds" {
                warnings.push(format!(
                    "{} (line {}): 'type: command_succeeds' is deprecated, use 'type: execution' instead.",
                    filename,
                    line_num + 1,
                ));
            }

            // `watches:` → use change checks
            // Only flag if it looks like a step-level field (indented)
            if (trimmed.starts_with("watches:") || trimmed.starts_with("watches :"))
                && line.starts_with(' ')
            {
                warnings.push(format!(
                    "{} (line {}): 'watches' is deprecated. Use 'check: {{ type: change, target: ... }}' instead.",
                    filename,
                    line_num + 1,
                ));
            }

            // `prompt_if_complete` → use `prompt_on_rerun`
            if trimmed.starts_with("prompt_if_complete:")
                || trimmed.starts_with("prompt_if_complete :")
            {
                warnings.push(format!(
                    "{} (line {}): 'prompt_if_complete' is deprecated, use 'prompt_on_rerun' instead.",
                    filename,
                    line_num + 1,
                ));
            }

            // `log_path` → removed
            if trimmed.starts_with("log_path:") || trimmed.starts_with("log_path :") {
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
    use crate::config::StepConfig;
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
    fn collect_warnings_empty_for_clean_config() {
        let config = config_with_step(StepConfig::default());
        assert!(collect_deprecation_warnings(&config).is_empty());
    }

    // --- collect_raw_yaml_deprecation_warnings tests ---

    #[test]
    fn raw_yaml_detects_completed_check() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        std::fs::write(
            &config_path,
            "steps:\n  setup:\n    command: echo hi\n    completed_check:\n      type: file_exists\n      path: node_modules\n",
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[config_path.as_path()]);
        assert!(warnings
            .iter()
            .any(|w| w.contains("'completed_check' is deprecated")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("'type: file_exists' is deprecated")));
    }

    #[test]
    fn raw_yaml_detects_marker_type() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        std::fs::write(
            &config_path,
            "steps:\n  setup:\n    command: echo hi\n    completed_check:\n      type: marker\n",
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[config_path.as_path()]);
        assert!(warnings
            .iter()
            .any(|w| w.contains("'type: marker' is removed")));
    }

    #[test]
    fn raw_yaml_detects_watches() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.yml");
        std::fs::write(
            &config_path,
            "steps:\n  setup:\n    command: echo hi\n    watches:\n      - Gemfile.lock\n",
        )
        .unwrap();

        let warnings = collect_raw_yaml_deprecation_warnings(&[config_path.as_path()]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'watches' is deprecated"));
    }

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
