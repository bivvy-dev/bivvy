//! Detects malformed `${...}` interpolation in string-valued config fields.
//!
//! Bivvy uses a flat namespace for variable interpolation (`${name}`).
//! The rule scans every string-valued config field and reports:
//!
//! - Unterminated `${...` (no closing brace)
//! - Empty references `${}`
//! - Dotted references whose namespace is not recognized
//!   (e.g. `${unknown.foo}` — Bivvy doesn't have namespaces, so any
//!   `${ns.key}` is flagged as a likely typo for a flat name or a
//!   reference to a system that doesn't exist)
//! - Plain references that aren't defined as a `vars:` entry, a
//!   user-recognized built-in, or an environment variable name
//!   pattern
//!
//! Fold-in note: this rule subsumes `var-references-undefined-var`.
//! When a flat reference like `${foo}` does not resolve to any known
//! variable, the diagnostic explicitly mentions vars to keep guidance
//! actionable.

use std::collections::HashSet;

use crate::config::interpolation::extract_variables;
use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Built-in variable names always available during interpolation.
const BUILTINS: &[&str] = &["bivvy_version", "project_name", "project_root"];

/// Recognized "namespaces" in dotted interpolation references. Bivvy
/// itself doesn't use namespaces, but we recognize a small allowlist
/// so users who reach for `${env.X}` or `${vars.X}` get a more
/// targeted diagnostic.
const KNOWN_NAMESPACES: &[&str] = &["vars", "env", "secrets", "inputs", "prompts"];

/// Detects malformed interpolation in string-valued config fields.
pub struct InterpolationSyntaxErrorRule;

impl LintRule for InterpolationSyntaxErrorRule {
    fn id(&self) -> RuleId {
        RuleId::new("interpolation-syntax-error")
    }

    fn name(&self) -> &str {
        "Interpolation Syntax Error"
    }

    fn description(&self) -> &str {
        "Detects malformed ${...} interpolation in string-valued config fields"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let known = KnownNames {
            vars: config.vars.keys().cloned().collect(),
            secrets: config.secrets.keys().cloned().collect(),
            prompts: config
                .steps
                .values()
                .flat_map(|s| s.output_settings.prompts.iter())
                .map(|p| p.key.clone())
                .collect(),
        };
        let ctx = InspectCtx {
            rule_id: self.id(),
            severity: self.default_severity(),
            known: &known,
        };

        let mut diagnostics = Vec::new();

        // Visit each string field, tagged with a path label.
        for (step_name, step) in &config.steps {
            if let Some(ref cmd) = step.execution.command {
                inspect_string(
                    cmd,
                    &format!("steps.{}.command", step_name),
                    &ctx,
                    &mut diagnostics,
                );
            }
            for (key, val) in &step.env_vars.env {
                inspect_string(
                    val,
                    &format!("steps.{}.env.{}", step_name, key),
                    &ctx,
                    &mut diagnostics,
                );
            }
            for (idx, before) in step.hooks.before.iter().enumerate() {
                inspect_string(
                    before,
                    &format!("steps.{}.before[{}]", step_name, idx),
                    &ctx,
                    &mut diagnostics,
                );
            }
            for (idx, after) in step.hooks.after.iter().enumerate() {
                inspect_string(
                    after,
                    &format!("steps.{}.after[{}]", step_name, idx),
                    &ctx,
                    &mut diagnostics,
                );
            }
        }

        for (key, val) in &config.settings.env_vars.env {
            inspect_string(
                val,
                &format!("settings.env.{}", key),
                &ctx,
                &mut diagnostics,
            );
        }

        for (workflow_name, workflow) in &config.workflows {
            for (key, val) in &workflow.env {
                inspect_string(
                    val,
                    &format!("workflows.{}.env.{}", workflow_name, key),
                    &ctx,
                    &mut diagnostics,
                );
            }
        }

        diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
        diagnostics
    }
}

/// Names known to the interpolation resolver, used to validate `${...}` references.
struct KnownNames {
    vars: HashSet<String>,
    secrets: HashSet<String>,
    prompts: HashSet<String>,
}

/// Per-call context bundling the rule's identity and the names it should accept.
struct InspectCtx<'a> {
    rule_id: RuleId,
    severity: Severity,
    known: &'a KnownNames,
}

/// Apply syntactic + name-resolution checks to a single string value.
fn inspect_string(value: &str, path: &str, ctx: &InspectCtx<'_>, out: &mut Vec<LintDiagnostic>) {
    let rule_id = &ctx.rule_id;
    let severity = ctx.severity;
    let known_vars = &ctx.known.vars;
    let known_secrets = &ctx.known.secrets;
    let known_prompts = &ctx.known.prompts;
    // 1. Unterminated `${...`. Find every `${` start that isn't escaped
    //    (`$${`) and verify a closing brace exists before EOF.
    let bytes = value.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' {
            // Check for escape: previous char is also '$' (i.e. `$${`).
            // The `$$` form is the escape.
            if i > 0 && bytes[i - 1] == b'$' {
                i += 2;
                continue;
            }
            // Look for closing brace.
            if value[i + 2..].find('}').is_none() {
                out.push(
                    LintDiagnostic::new(
                        rule_id.clone(),
                        severity,
                        format!(
                            "Unterminated interpolation `${{...` in {} (no closing brace)",
                            path
                        ),
                    )
                    .with_suggestion("Close the interpolation with `}`".to_string()),
                );
                // After reporting the unterminated start, stop scanning
                // this string — extracted variables below will likely
                // pick up nonsense for the unclosed segment.
                return;
            }
            i += 2;
            continue;
        }
        i += 1;
    }

    // 2. Empty `${}`.
    if value.contains("${}") {
        out.push(
            LintDiagnostic::new(
                rule_id.clone(),
                severity,
                format!("Empty interpolation `${{}}` in {}", path),
            )
            .with_suggestion("Replace with a variable name, e.g. `${app_name}`".to_string()),
        );
    }

    // 3. Variable name resolution. `extract_variables` returns names
    //    found between `${` and `}` (escapes already handled).
    let names = extract_variables(value);
    for name in names {
        if name.is_empty() {
            continue; // already reported above
        }
        // Dotted reference?
        if let Some((ns, key)) = name.split_once('.') {
            if KNOWN_NAMESPACES.contains(&ns) {
                // Recognized namespace; verify key.
                let known = match ns {
                    "vars" => known_vars.contains(key),
                    "secrets" => known_secrets.contains(key),
                    "prompts" => known_prompts.contains(key),
                    // env and inputs are populated at runtime; we can't
                    // verify them here. Treat as accepted.
                    "env" | "inputs" => true,
                    _ => false,
                };
                if !known {
                    out.push(
                        LintDiagnostic::new(
                            rule_id.clone(),
                            severity,
                            format!(
                                "Reference `${{{}}}` in {} is unknown — '{}.{}' is not defined",
                                name, path, ns, key
                            ),
                        )
                        .with_suggestion(format!(
                            "Define '{}' in `{}` or remove the reference",
                            key, ns
                        )),
                    );
                }
            } else {
                out.push(
                    LintDiagnostic::new(
                        rule_id.clone(),
                        severity,
                        format!(
                            "Reference `${{{}}}` in {} uses unknown namespace '{}'",
                            name, path, ns
                        ),
                    )
                    .with_suggestion(format!(
                        "Bivvy interpolation is flat — use `${{{}}}` or one of: {}",
                        key,
                        KNOWN_NAMESPACES.join(", ")
                    )),
                );
            }
        } else {
            // Flat reference. Check if it's a known var, secret, prompt,
            // built-in, or env-var-shaped name (uppercase + underscores).
            let is_known = known_vars.contains(&name)
                || known_secrets.contains(&name)
                || known_prompts.contains(&name)
                || BUILTINS.contains(&name.as_str())
                || looks_like_env_var(&name);
            if !is_known {
                out.push(
                    LintDiagnostic::new(
                        rule_id.clone(),
                        severity,
                        format!(
                            "Reference `${{{}}}` in {} resolves to nothing — '{}' is not a defined var, secret, prompt, or built-in",
                            name, path, name
                        ),
                    )
                    .with_suggestion(format!(
                        "Add '{}' to `vars:` or fix the typo",
                        name
                    )),
                );
            }
        }
    }
}

/// Loose heuristic: a name made entirely of uppercase letters, digits,
/// or underscores is treated as an environment variable reference and
/// not flagged. Avoids false positives on `${HOME}`, `${PATH}`, etc.
fn looks_like_env_var(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{
        ExecutionConfig, SecretConfig, StepConfig, VarDefinition, WorkflowConfig,
    };
    use std::collections::HashMap;

    fn config_with_command(cmd: &str) -> BivvyConfig {
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                execution: ExecutionConfig {
                    command: Some(cmd.to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        BivvyConfig {
            steps,
            ..Default::default()
        }
    }

    #[test]
    fn flags_unterminated_interpolation() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${app_name");

        let diags = rule.check(&config);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, RuleId::new("interpolation-syntax-error"));
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("Unterminated"));
        assert!(diags[0].message.contains("steps.test.command"));
    }

    #[test]
    fn flags_empty_interpolation() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${}");

        let diags = rule.check(&config);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("Empty interpolation")));
    }

    #[test]
    fn flags_unknown_namespace() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${unknown.foo}");

        let diags = rule.check(&config);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("unknown namespace")));
        assert!(messages.iter().any(|m| m.contains("unknown")));
    }

    #[test]
    fn flags_undefined_var_in_known_namespace() {
        let rule = InterpolationSyntaxErrorRule;
        let mut config = config_with_command("echo ${vars.bar}");
        config.vars.insert(
            "foo".to_string(),
            VarDefinition::Static("hello".to_string()),
        );

        let diags = rule.check(&config);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("vars.bar")));
    }

    #[test]
    fn ignores_defined_var_in_namespace() {
        let rule = InterpolationSyntaxErrorRule;
        let mut config = config_with_command("echo ${vars.app_name}");
        config.vars.insert(
            "app_name".to_string(),
            VarDefinition::Static("bivvy".to_string()),
        );

        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn flags_undefined_flat_reference() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${nonexistent_var}");

        let diags = rule.check(&config);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages
            .iter()
            .any(|m| m.contains("nonexistent_var") && m.contains("resolves to nothing")));
    }

    #[test]
    fn ignores_uppercase_env_style_names() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${HOME}/${PATH}");

        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn ignores_builtin_references() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${project_name} v${bivvy_version}");

        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn ignores_defined_flat_var() {
        let rule = InterpolationSyntaxErrorRule;
        let mut config = config_with_command("echo ${greeting}");
        config.vars.insert(
            "greeting".to_string(),
            VarDefinition::Static("hello".to_string()),
        );

        let diags = rule.check(&config);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_escaped_dollar_brace() {
        let rule = InterpolationSyntaxErrorRule;
        // `$${...}` is a literal — not an interpolation.
        let config = config_with_command("echo $${literal}");
        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn flags_in_workflow_env() {
        let rule = InterpolationSyntaxErrorRule;
        let mut config = BivvyConfig::default();
        let mut workflow = WorkflowConfig::default();
        workflow
            .env
            .insert("DEBUG".to_string(), "${not_a_var}".to_string());
        config.workflows.insert("ci".to_string(), workflow);

        let diags = rule.check(&config);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages
            .iter()
            .any(|m| m.contains("workflows.ci.env.DEBUG")));
    }

    #[test]
    fn ignores_known_secret_in_namespace() {
        let rule = InterpolationSyntaxErrorRule;
        let mut config = config_with_command("echo ${secrets.api_key}");
        config.secrets.insert(
            "api_key".to_string(),
            SecretConfig {
                command: "op read api_key".to_string(),
            },
        );

        let diags = rule.check(&config);
        assert!(diags.is_empty(), "got {:?}", diags);
    }

    #[test]
    fn suggestion_proposes_vars_fix() {
        let rule = InterpolationSyntaxErrorRule;
        let config = config_with_command("echo ${typo_var}");

        let diags = rule.check(&config);
        let suggestion = diags[0].suggestion.as_ref().unwrap();
        assert!(suggestion.contains("typo_var"));
        assert!(suggestion.contains("vars"));
    }
}
