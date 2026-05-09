//! Detects workflow/workflows key typos.
//!
//! In `.bivvy/config.yml` the top-level key must be `workflows:` (plural).
//! In `.bivvy/workflows/<file>.yml` the top-level key must be `workflow:`
//! (singular). Users routinely confuse the two — and because both keys
//! parse with `serde(default)`, the wrong key is silently ignored. This
//! rule scans raw YAML to catch the typo and emit a precise diagnostic.

use std::path::{Path, PathBuf};

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity, Span};

/// Detects `workflow:` vs `workflows:` typos at the top of config files.
pub struct WorkflowSingularTypoRule;

impl LintRule for WorkflowSingularTypoRule {
    fn id(&self) -> RuleId {
        RuleId::new("workflow-singular-typo")
    }

    fn name(&self) -> &str {
        "Workflow Singular/Plural Typo"
    }

    fn description(&self) -> &str {
        "Detects workflow vs workflows typos at the top of config files"
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
        scan_project_for_typos(&root, &self.id(), self.default_severity())
    }
}

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

/// Scan a project root for workflow key typos.
pub(crate) fn scan_project_for_typos(
    project_root: &Path,
    rule_id: &RuleId,
    severity: Severity,
) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let bivvy_dir = project_root.join(".bivvy");

    let main_config = bivvy_dir.join("config.yml");
    if main_config.is_file() {
        check_main_config(&main_config, rule_id, severity, &mut diagnostics);
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

fn check_main_config(
    path: &Path,
    rule_id: &RuleId,
    severity: Severity,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    if let Some(line) = find_top_level_key_line(&content, "workflow") {
        diagnostics.push(
            LintDiagnostic::new(
                rule_id.clone(),
                severity,
                "Top-level key 'workflow' is invalid in .bivvy/config.yml; use 'workflows:' (plural)",
            )
            .with_span(Span::line(path, line))
            .with_suggestion("workflows:".to_string()),
        );
    }
}

fn check_workflow_file(
    path: &Path,
    rule_id: &RuleId,
    severity: Severity,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    if let Some(line) = find_top_level_key_line(&content, "workflows") {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        diagnostics.push(
            LintDiagnostic::new(
                rule_id.clone(),
                severity,
                format!(
                    "Top-level key 'workflows' is invalid in workflow file '{}'; use 'workflow:' (singular)",
                    filename
                ),
            )
            .with_span(Span::line(path, line))
            .with_suggestion("workflow:".to_string()),
        );
    }
}

/// Find the 1-based line number of the EXACT top-level key.
///
/// Matches `<key>:` at column 0 with no leading whitespace, ignoring
/// comments. Returns `None` if not present.
fn find_top_level_key_line(content: &str, key: &str) -> Option<usize> {
    let head = format!("{}:", key);
    for (idx, raw) in content.lines().enumerate() {
        let trimmed = raw.trim_start();
        // Top-level only: no indentation.
        if raw.len() != trimmed.len() {
            continue;
        }
        if trimmed.starts_with(&head) {
            // Make sure it's not a longer key like `workflows_extra:`.
            let after = &trimmed[head.len() - 1..]; // includes the ':'
            if after.starts_with(':') {
                let next = after.chars().nth(1);
                // Exact match: ':' followed by end-of-line, space, or comment.
                if matches!(next, None | Some(' ') | Some('\t') | Some('#')) {
                    return Some(idx + 1);
                }
            }
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
        let rule = WorkflowSingularTypoRule;
        scan_project_for_typos(temp.path(), &rule.id(), rule.default_severity())
    }

    #[test]
    fn flags_singular_workflow_in_main_config() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "app_name: demo\nworkflow:\n  default:\n    steps:\n      - build\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("workflow-singular-typo"));
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("workflows"));
        assert!(diags[0].message.contains("plural"));
        assert_eq!(diags[0].suggestion.as_deref(), Some("workflows:"));
    }

    #[test]
    fn flags_plural_workflows_in_workflow_file() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/workflows")).unwrap();
        fs::write(
            temp.path().join(".bivvy/workflows/release.yml"),
            "workflows:\n  steps:\n    - bump\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("singular"));
        assert!(diags[0].message.contains("release.yml"));
        assert_eq!(diags[0].suggestion.as_deref(), Some("workflow:"));
    }

    #[test]
    fn ignores_correctly_named_keys() {
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
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn ignores_indented_workflow_keys() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        // 'workflow:' appears nested under settings.foo, not at column 0;
        // the rule must not flag it.
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "app_name: demo\nsettings:\n  notes:\n    workflow: my-fave\nworkflows:\n  default:\n    steps:\n      - a\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn span_points_at_typo_line() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "app_name: demo\nsteps:\n  build:\n    command: cargo build\nworkflow:\n  default:\n    steps:\n      - build\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        let span = diags[0].span.as_ref().unwrap();
        assert_eq!(span.start_line, 5);
    }
}
