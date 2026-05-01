//! Detects step names defined in multiple files with diverging bodies.
//!
//! When the same step name is defined in two or more files (e.g. one in
//! `.bivvy/config.yml` and another in `.bivvy/steps/setup.yml`) the merge
//! pipeline silently picks one definition. If the bodies differ, the lost
//! definition is invisible — and the result depends on merge order.
//!
//! This rule walks up from `std::env::current_dir()` to find a `.bivvy/`
//! directory, then scans `.bivvy/config.yml`, `.bivvy/steps/*.yml`, and
//! `.bivvy/workflows/*.yml` for raw `steps.<name>` definitions. When the
//! same name appears in multiple files with non-byte-equal bodies, the
//! rule emits a warning per collision.
//!
//! When the project root cannot be located (e.g. tests run outside any
//! project), the rule emits no diagnostics — it relies on filesystem
//! state that may not exist.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity, Span};

/// Detects step name collisions across config files.
pub struct StepNameCollisionRule;

impl LintRule for StepNameCollisionRule {
    fn id(&self) -> RuleId {
        RuleId::new("step-name-collision")
    }

    fn name(&self) -> &str {
        "Step Name Collision"
    }

    fn description(&self) -> &str {
        "Detects steps defined in multiple files with diverging bodies"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, _config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let Ok(cwd) = std::env::current_dir() else {
            return Vec::new();
        };
        let Some(root) = find_project_root(&cwd) else {
            return Vec::new();
        };
        scan_for_collisions(&root, &self.id(), self.default_severity())
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

/// One step definition discovered in a particular source file.
#[derive(Debug, Clone)]
struct StepDef {
    file: PathBuf,
    body: String,
}

/// Scan project files for step name collisions.
pub(crate) fn scan_for_collisions(
    project_root: &Path,
    rule_id: &RuleId,
    severity: Severity,
) -> Vec<LintDiagnostic> {
    let bivvy_dir = project_root.join(".bivvy");
    let mut by_name: BTreeMap<String, Vec<StepDef>> = BTreeMap::new();

    // Files that contain `steps:` blocks.
    let mut files: Vec<PathBuf> = Vec::new();

    let main_config = bivvy_dir.join("config.yml");
    if main_config.is_file() {
        files.push(main_config);
    }

    let local_config = bivvy_dir.join("config.local.yml");
    if local_config.is_file() {
        files.push(local_config);
    }

    push_yaml_files_from(&bivvy_dir.join("steps"), &mut files);
    push_yaml_files_from(&bivvy_dir.join("workflows"), &mut files);

    for file in files {
        collect_steps_from_file(&file, &mut by_name);
    }

    let mut diagnostics = Vec::new();
    for (name, defs) in by_name {
        if defs.len() < 2 {
            continue;
        }
        // Compare bodies byte-for-byte (after normalizing).
        let canonical = defs[0].body.clone();
        if !defs.iter().all(|d| d.body == canonical) {
            // Build a message listing all the source files.
            let files_list: Vec<String> = defs
                .iter()
                .map(|d| {
                    d.file
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| d.file.display().to_string())
                })
                .collect();

            let mut diag = LintDiagnostic::new(
                rule_id.clone(),
                severity,
                format!(
                    "Step '{}' is defined with different bodies in {} files: {}",
                    name,
                    defs.len(),
                    files_list.join(", ")
                ),
            )
            .with_suggestion(format!(
                "Pick a single definition for '{}' or rename the others",
                name
            ));

            // Use the first file as the primary span, others as related.
            diag = diag.with_span(Span::line(&defs[0].file, 1));
            for d in defs.iter().skip(1) {
                diag = diag.with_related(Span::line(&d.file, 1), "also defined here");
            }
            diagnostics.push(diag);
        }
    }

    diagnostics
}

fn push_yaml_files_from(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "yml" || ext == "yaml")
            {
                out.push(path);
            }
        }
    }
}

/// Pull step definitions from a YAML file, recording each by name.
///
/// Looks for both `steps:` blocks (top-level or inside a workflow file) so
/// the rule covers main configs, local overrides, split steps, and
/// workflow files alike. Step bodies are serialized back to a canonical
/// YAML string so byte-level equality is meaningful.
fn collect_steps_from_file(path: &Path, out: &mut BTreeMap<String, Vec<StepDef>>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
        return;
    };

    if let Some(steps) = value.get("steps").and_then(|v| v.as_mapping()) {
        for (key, val) in steps {
            if let Some(name) = key.as_str() {
                if let Ok(body) = serde_yaml::to_string(val) {
                    out.entry(name.to_string()).or_default().push(StepDef {
                        file: path.to_path_buf(),
                        body,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn run_in_temp(temp: &TempDir) -> Vec<LintDiagnostic> {
        let rule = StepNameCollisionRule;
        scan_for_collisions(temp.path(), &rule.id(), rule.default_severity())
    }

    #[test]
    fn flags_collision_with_diverging_bodies() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/steps")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "steps:\n  setup:\n    command: cargo build\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/steps/setup.yml"),
            "steps:\n  setup:\n    command: cargo test\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("step-name-collision"));
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("setup"));
        assert!(diags[0].message.contains("config.yml"));
        assert!(diags[0].message.contains("setup.yml"));
        assert!(!diags[0].related.is_empty());
    }

    #[test]
    fn ignores_collision_with_identical_bodies() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/steps")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "steps:\n  setup:\n    command: cargo build\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/steps/setup.yml"),
            "steps:\n  setup:\n    command: cargo build\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn ignores_unique_step_definitions() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/steps")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "steps:\n  build:\n    command: cargo build\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/steps/test.yml"),
            "steps:\n  test:\n    command: cargo test\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn detects_collision_with_workflow_file_steps() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/workflows")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "steps:\n  bump:\n    command: cargo bump --patch\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/workflows/release.yml"),
            "steps:\n  bump:\n    command: cargo bump --major\nworkflow:\n  steps:\n    - bump\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bump"));
    }

    #[test]
    fn no_diagnostics_without_bivvy_dir() {
        let temp = TempDir::new().unwrap();
        let diags = run_in_temp(&temp);
        assert!(diags.is_empty());
    }

    #[test]
    fn suggestion_proposes_resolution_strategy() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy/steps")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "steps:\n  setup:\n    command: cargo build\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/steps/setup.yml"),
            "steps:\n  setup:\n    command: cargo install\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("setup"));
        assert!(
            suggestion.to_lowercase().contains("rename")
                || suggestion.to_lowercase().contains("single")
        );
    }
}
