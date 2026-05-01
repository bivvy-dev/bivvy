//! Detects workflow definitions written as bare YAML sequences.
//!
//! A workflow value must be a mapping with a `steps:` key. Some users
//! mistakenly write the workflow as a bare list of step names — that
//! looks correct at first glance but the schema rejects it. This rule
//! detects those cases via raw YAML inspection so the diagnostic points
//! at the offending file and line.
//!
//! The rule scans:
//! - `.bivvy/config.yml` top-level `workflows:` (each entry's value)
//! - `.bivvy/workflows/*.yml` top-level `workflow:` value
//!
//! Detection is done at lint time by walking the project root that the
//! lint command exposes via `cwd`. If no `.bivvy/` directory is found,
//! the rule emits no diagnostics.

use std::path::{Path, PathBuf};

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity, Span};

/// Detects workflow values written as bare YAML sequences.
pub struct WorkflowShapeShorthandRule;

impl LintRule for WorkflowShapeShorthandRule {
    fn id(&self) -> RuleId {
        RuleId::new("workflow-shape-shorthand")
    }

    fn name(&self) -> &str {
        "Workflow Shape Shorthand"
    }

    fn description(&self) -> &str {
        "Detects workflows written as a bare YAML sequence instead of a mapping with steps:"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let Ok(cwd) = std::env::current_dir() else {
            return Vec::new();
        };
        let Some(root) = find_project_root(&cwd) else {
            return Vec::new();
        };
        scan_project_for_shorthand(&root, &self.id(), self.default_severity())
    }
}

/// Walk up from `start` to find the directory containing `.bivvy/`.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".bivvy").is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Run the shorthand scan against a known project root.
///
/// Exposed (crate-private) so tests can drive the scan against a tempdir.
pub(crate) fn scan_project_for_shorthand(
    project_root: &Path,
    rule_id: &RuleId,
    severity: Severity,
) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let bivvy_dir = project_root.join(".bivvy");

    let config_path = bivvy_dir.join("config.yml");
    if config_path.is_file() {
        check_main_config(&config_path, rule_id, severity, &mut diagnostics);
    }

    let workflows_dir = bivvy_dir.join("workflows");
    if workflows_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&workflows_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext == "yml" || ext == "yaml")
                {
                    check_workflow_file(&path, rule_id, severity, &mut diagnostics);
                }
            }
        }
    }

    diagnostics
}

/// Inspect `.bivvy/config.yml` for workflow entries that are bare sequences.
fn check_main_config(
    path: &Path,
    rule_id: &RuleId,
    severity: Severity,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
        // Parse errors are reported by other paths. Skip silently.
        return;
    };
    let Some(workflows) = value.get("workflows").and_then(|v| v.as_mapping()) else {
        return;
    };

    for (key, val) in workflows {
        if val.is_sequence() {
            let name = key.as_str().unwrap_or("<unnamed>");
            let line = find_workflow_line(&content, name).unwrap_or(1);
            diagnostics.push(
                LintDiagnostic::new(
                    rule_id.clone(),
                    severity,
                    format!(
                        "Workflow '{}' is a bare list; wrap it in a mapping with a `steps:` key",
                        name
                    ),
                )
                .with_span(Span::line(path, line))
                .with_suggestion(format!("{}:\n  steps:\n    - <step-name>", name)),
            );
        }
    }
}

/// Inspect a `.bivvy/workflows/<file>.yml` for a top-level `workflow:` that is a bare sequence.
fn check_workflow_file(
    path: &Path,
    rule_id: &RuleId,
    severity: Severity,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
        return;
    };
    let Some(workflow) = value.get("workflow") else {
        return;
    };
    if workflow.is_sequence() {
        let line = find_top_level_key_line(&content, "workflow").unwrap_or(1);
        diagnostics.push(
            LintDiagnostic::new(
                rule_id.clone(),
                severity,
                format!(
                    "Workflow file '{}' has a bare-list `workflow:`; wrap it in a mapping with a `steps:` key",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
            )
            .with_span(Span::line(path, line))
            .with_suggestion("workflow:\n  steps:\n    - <step-name>".to_string()),
        );
    }
}

/// Find the 1-based line number where a workflow entry is declared inside
/// `.bivvy/config.yml`. Returns `None` if not found.
fn find_workflow_line(content: &str, workflow_name: &str) -> Option<usize> {
    let mut in_workflows = false;
    let mut workflows_indent: Option<usize> = None;
    for (idx, raw) in content.lines().enumerate() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        if trimmed.starts_with("workflows:") && indent == 0 {
            in_workflows = true;
            workflows_indent = Some(0);
            continue;
        }
        if in_workflows {
            // Detect end of workflows block.
            if !trimmed.is_empty() && indent == workflows_indent.unwrap_or(0) {
                in_workflows = false;
                continue;
            }
            // Lines inside workflows: keys are nested deeper.
            let head = format!("{}:", workflow_name);
            if trimmed.starts_with(&head) {
                return Some(idx + 1);
            }
        }
    }
    None
}

/// Find the 1-based line number of a top-level YAML key.
fn find_top_level_key_line(content: &str, key: &str) -> Option<usize> {
    let head = format!("{}:", key);
    for (idx, raw) in content.lines().enumerate() {
        if raw.starts_with(&head) {
            return Some(idx + 1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn run_in_temp(temp: &TempDir) -> Vec<LintDiagnostic> {
        let rule = WorkflowShapeShorthandRule;
        scan_project_for_shorthand(temp.path(), &rule.id(), rule.default_severity())
    }

    #[test]
    fn flags_bare_sequence_in_main_config() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "app_name: demo\nworkflows:\n  default:\n    - build\n    - test\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("workflow-shape-shorthand"));
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("default"));
        assert!(diags[0].message.contains("bare list"));
        assert!(diags[0].suggestion.is_some());
        assert!(diags[0].suggestion.as_ref().unwrap().contains("steps:"));
    }

    #[test]
    fn flags_bare_sequence_in_workflow_file() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/workflows")).unwrap();
        fs::write(
            temp.path().join(".bivvy/workflows/release.yml"),
            "workflow:\n  - bump\n  - tag\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("release.yml"));
        assert!(diags[0].suggestion.is_some());
        assert!(diags[0].suggestion.as_ref().unwrap().contains("workflow:"));
    }

    #[test]
    fn ignores_correctly_shaped_workflows() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/workflows")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "app_name: demo\nworkflows:\n  default:\n    steps:\n      - build\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/workflows/release.yml"),
            "workflow:\n  steps:\n    - bump\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    }

    #[test]
    fn ignores_missing_bivvy_dir() {
        let temp = TempDir::new().unwrap();
        let diags = run_in_temp(&temp);
        assert!(diags.is_empty());
    }

    #[test]
    fn span_points_at_offending_workflow() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "app_name: demo\nworkflows:\n  good:\n    steps:\n      - a\n  bad:\n    - b\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        let span = diags[0].span.as_ref().unwrap();
        assert_eq!(span.start_line, 6);
    }
}
