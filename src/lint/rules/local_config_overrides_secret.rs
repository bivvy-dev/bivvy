//! Detects secrets whose handler is overridden in `.bivvy/config.local.yml`.
//!
//! `.bivvy/config.local.yml` is gitignored and intended for personal
//! overrides. When it redefines a `secrets.<name>.command:` previously
//! declared in the project's main `.bivvy/config.yml`, that's worth
//! surfacing during security review — a teammate's audit of the
//! committed config would not see the change. This rule is informational
//! (Hint) and only fires when both files exist and the local override
//! differs from the project value.

use std::path::{Path, PathBuf};

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity, Span};

/// Detects secret command overrides in the local config.
pub struct LocalConfigOverridesSecretRule;

impl LintRule for LocalConfigOverridesSecretRule {
    fn id(&self) -> RuleId {
        RuleId::new("local-config-overrides-secret")
    }

    fn name(&self) -> &str {
        "Local Config Overrides Secret"
    }

    fn description(&self) -> &str {
        "Detects secret commands redefined in .bivvy/config.local.yml"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, _config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let Ok(cwd) = std::env::current_dir() else {
            return Vec::new();
        };
        let Some(root) = find_project_root(&cwd) else {
            return Vec::new();
        };
        scan_for_overrides(&root, &self.id(), self.default_severity())
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

/// Compare `.bivvy/config.yml` against `.bivvy/config.local.yml` and
/// produce a diagnostic for each secret whose `command` differs.
pub(crate) fn scan_for_overrides(
    project_root: &Path,
    rule_id: &RuleId,
    severity: Severity,
) -> Vec<LintDiagnostic> {
    let project_path = project_root.join(".bivvy/config.yml");
    let local_path = project_root.join(".bivvy/config.local.yml");
    if !project_path.is_file() || !local_path.is_file() {
        return Vec::new();
    }

    let Ok(project_content) = std::fs::read_to_string(&project_path) else {
        return Vec::new();
    };
    let Ok(local_content) = std::fs::read_to_string(&local_path) else {
        return Vec::new();
    };

    let Ok(project_value) = serde_yaml::from_str::<serde_yaml::Value>(&project_content) else {
        return Vec::new();
    };
    let Ok(local_value) = serde_yaml::from_str::<serde_yaml::Value>(&local_content) else {
        return Vec::new();
    };

    let project_secrets = project_value
        .get("secrets")
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();
    let local_secrets = local_value
        .get("secrets")
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();

    if local_secrets.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for (key, local_def) in &local_secrets {
        let Some(name) = key.as_str() else {
            continue;
        };
        let local_command = local_def
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let project_command = project_secrets
            .get(serde_yaml::Value::String(name.to_string()))
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str());

        if let Some(project_cmd) = project_command {
            if project_cmd != local_command {
                let line = find_secret_line(&local_content, name).unwrap_or(1);
                diagnostics.push(
                    LintDiagnostic::new(
                        rule_id.clone(),
                        severity,
                        format!(
                            "Local config overrides secret '{}' command from project config",
                            name
                        ),
                    )
                    .with_span(Span::line(&local_path, line))
                    .with_suggestion(format!(
                        "Confirm '{}' override is intentional. Local edits are gitignored \
                         and won't show up in audits of the committed config.",
                        name
                    )),
                );
            }
        }
    }

    diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
    diagnostics
}

/// Best-effort line lookup for a `secrets.<name>:` mapping inside a
/// YAML file.
fn find_secret_line(content: &str, name: &str) -> Option<usize> {
    let head = format!("{}:", name);
    let mut in_secrets = false;
    for (idx, raw) in content.lines().enumerate() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        if indent == 0 && trimmed.starts_with("secrets:") {
            in_secrets = true;
            continue;
        }
        if in_secrets {
            if !trimmed.is_empty() && indent == 0 {
                in_secrets = false;
                continue;
            }
            if trimmed.starts_with(&head) {
                return Some(idx + 1);
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
        let rule = LocalConfigOverridesSecretRule;
        scan_for_overrides(temp.path(), &rule.id(), rule.default_severity())
    }

    #[test]
    fn flags_overridden_secret_command() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "secrets:\n  api_key:\n    command: op read api_key\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/config.local.yml"),
            "secrets:\n  api_key:\n    command: cat ~/.secrets/api_key\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].rule_id,
            RuleId::new("local-config-overrides-secret")
        );
        assert_eq!(diags[0].severity, Severity::Hint);
        assert!(diags[0].message.contains("api_key"));
        assert!(diags[0].suggestion.is_some());
    }

    #[test]
    fn ignores_identical_secret_command() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "secrets:\n  api_key:\n    command: op read api_key\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/config.local.yml"),
            "secrets:\n  api_key:\n    command: op read api_key\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_local_only_secret() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(temp.path().join(".bivvy/config.yml"), "app_name: demo\n").unwrap();
        fs::write(
            temp.path().join(".bivvy/config.local.yml"),
            "secrets:\n  personal:\n    command: cat ~/personal\n",
        )
        .unwrap();

        // Local introduces a NEW secret — not an override. The rule
        // shouldn't fire.
        let diags = run_in_temp(&temp);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_when_local_config_missing() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "secrets:\n  api_key:\n    command: op read api_key\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert!(diags.is_empty());
    }

    #[test]
    fn span_points_at_local_secret_line() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "secrets:\n  token:\n    command: original\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/config.local.yml"),
            "app_name: demo\nsecrets:\n  token:\n    command: overridden\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 1);
        let span = diags[0].span.as_ref().unwrap();
        assert_eq!(span.start_line, 3);
    }

    #[test]
    fn flags_multiple_overrides() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
        fs::write(
            temp.path().join(".bivvy/config.yml"),
            "secrets:\n  a:\n    command: x\n  b:\n    command: y\n",
        )
        .unwrap();
        fs::write(
            temp.path().join(".bivvy/config.local.yml"),
            "secrets:\n  a:\n    command: x_local\n  b:\n    command: y_local\n",
        )
        .unwrap();

        let diags = run_in_temp(&temp);
        assert_eq!(diags.len(), 2);
    }
}
