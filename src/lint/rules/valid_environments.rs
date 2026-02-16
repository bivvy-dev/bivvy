//! Environment validation rules.
//!
//! These rules validate environment-related configuration to catch
//! misconfigurations like referencing undefined environments, missing
//! workflows, or unreachable step overrides.

use std::collections::HashSet;

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Built-in environment names that bivvy auto-detects.
const BUILTIN_ENVIRONMENTS: &[&str] = &["ci", "docker", "codespace", "development"];

/// Collect all environment names defined in config settings.
fn defined_environments(config: &BivvyConfig) -> HashSet<&str> {
    let mut envs: HashSet<&str> = config
        .settings
        .environments
        .keys()
        .map(|s| s.as_str())
        .collect();
    // Built-in environments are always valid references
    for builtin in BUILTIN_ENVIRONMENTS {
        envs.insert(builtin);
    }
    envs
}

/// Detects step environment overrides that reference undefined environments.
///
/// If a step has an `environments:` block with a key that doesn't match
/// any environment defined in `settings.environments` or a built-in
/// environment, this rule flags it.
pub struct UnknownEnvironmentInStepRule;

impl LintRule for UnknownEnvironmentInStepRule {
    fn id(&self) -> RuleId {
        RuleId::new("unknown-environment-in-step")
    }

    fn name(&self) -> &str {
        "Unknown Environment in Step"
    }

    fn description(&self) -> &str {
        "Ensures step environment overrides reference known environments"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let known = defined_environments(config);
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            for env_name in step_config.environments.keys() {
                if !known.contains(env_name.as_str()) {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Step '{}' has override for unknown environment '{}'",
                                step_name, env_name
                            ),
                        )
                        .with_suggestion(format!(
                            "Define '{}' in settings.environments or use a built-in name (ci, docker, codespace)",
                            env_name
                        )),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Detects `only_environments` entries that reference undefined environments.
pub struct UnknownEnvironmentInOnlyRule;

impl LintRule for UnknownEnvironmentInOnlyRule {
    fn id(&self) -> RuleId {
        RuleId::new("unknown-environment-in-only")
    }

    fn name(&self) -> &str {
        "Unknown Environment in only_environments"
    }

    fn description(&self) -> &str {
        "Ensures only_environments entries reference known environments"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let known = defined_environments(config);
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            for env_name in &step_config.only_environments {
                if !known.contains(env_name.as_str()) {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Step '{}' only_environments references unknown environment '{}'",
                                step_name, env_name
                            ),
                        )
                        .with_suggestion(format!(
                            "Define '{}' in settings.environments or use a built-in name",
                            env_name
                        )),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Detects environments whose `default_workflow` references a nonexistent workflow.
pub struct EnvironmentDefaultWorkflowMissingRule;

impl LintRule for EnvironmentDefaultWorkflowMissingRule {
    fn id(&self) -> RuleId {
        RuleId::new("environment-default-workflow-missing")
    }

    fn name(&self) -> &str {
        "Environment Default Workflow Missing"
    }

    fn description(&self) -> &str {
        "Ensures environment default_workflow references an existing workflow"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (env_name, env_config) in &config.settings.environments {
            if let Some(ref workflow) = env_config.default_workflow {
                if !config.workflows.contains_key(workflow) {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Environment '{}' default_workflow '{}' does not exist",
                                env_name, workflow
                            ),
                        )
                        .with_suggestion(format!(
                            "Define workflow '{}' in the workflows section",
                            workflow
                        )),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Detects step environment overrides that can never be reached.
///
/// If a step has `only_environments: [a, b]` and also has an environment
/// override for `c`, the override for `c` is unreachable because the step
/// will never run in environment `c`.
pub struct UnreachableEnvironmentOverrideRule;

impl LintRule for UnreachableEnvironmentOverrideRule {
    fn id(&self) -> RuleId {
        RuleId::new("unreachable-environment-override")
    }

    fn name(&self) -> &str {
        "Unreachable Environment Override"
    }

    fn description(&self) -> &str {
        "Detects environment overrides on steps that only_environments excludes"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            // only_environments: [] means "all", so no overrides are unreachable
            if step_config.only_environments.is_empty() {
                continue;
            }

            let allowed: HashSet<&str> = step_config
                .only_environments
                .iter()
                .map(|s| s.as_str())
                .collect();

            for env_name in step_config.environments.keys() {
                if !allowed.contains(env_name.as_str()) {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Step '{}' has override for '{}' but only_environments excludes it",
                                step_name, env_name
                            ),
                        )
                        .with_suggestion(format!(
                            "Add '{}' to only_environments or remove the override",
                            env_name
                        )),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Warns when a custom environment name shadows a built-in environment.
///
/// Built-in environments (ci, docker, codespace) have auto-detection.
/// Defining a custom environment with the same name works but may
/// cause confusion about detection behavior.
pub struct CustomEnvironmentShadowsBuiltinRule;

impl LintRule for CustomEnvironmentShadowsBuiltinRule {
    fn id(&self) -> RuleId {
        RuleId::new("custom-environment-shadows-builtin")
    }

    fn name(&self) -> &str {
        "Custom Environment Shadows Builtin"
    }

    fn description(&self) -> &str {
        "Warns when a custom environment name matches a built-in name"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let builtins: HashSet<&str> = BUILTIN_ENVIRONMENTS.iter().copied().collect();

        for env_name in config.settings.environments.keys() {
            if builtins.contains(env_name.as_str()) {
                diagnostics.push(
                    LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Custom environment '{}' shadows built-in environment",
                            env_name
                        ),
                    )
                    .with_suggestion(format!(
                        "'{}' is auto-detected by bivvy. Custom config here extends (not replaces) built-in detection.",
                        env_name
                    )),
                );
            }
        }

        diagnostics
    }
}

/// Detects redundant environment overrides where override values match the base.
pub struct RedundantEnvironmentOverrideRule;

impl LintRule for RedundantEnvironmentOverrideRule {
    fn id(&self) -> RuleId {
        RuleId::new("redundant-environment-override")
    }

    fn name(&self) -> &str {
        "Redundant Environment Override"
    }

    fn description(&self) -> &str {
        "Detects environment overrides that are identical to the base step"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            for (env_name, overrides) in &step_config.environments {
                let mut redundant_fields = Vec::new();

                if let Some(ref cmd) = overrides.command {
                    if step_config.command.as_deref() == Some(cmd.as_str()) {
                        redundant_fields.push("command");
                    }
                }
                if let Some(ref title) = overrides.title {
                    if step_config.title.as_deref() == Some(title.as_str()) {
                        redundant_fields.push("title");
                    }
                }
                if let Some(ref desc) = overrides.description {
                    if step_config.description.as_deref() == Some(desc.as_str()) {
                        redundant_fields.push("description");
                    }
                }

                if !redundant_fields.is_empty() {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Step '{}' environment '{}' override has redundant fields: {}",
                                step_name,
                                env_name,
                                redundant_fields.join(", ")
                            ),
                        )
                        .with_suggestion(
                            "Remove fields that are identical to the base step configuration"
                                .to_string(),
                        ),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Detects null env entries that remove nonexistent keys.
pub struct RedundantEnvNullRule;

impl LintRule for RedundantEnvNullRule {
    fn id(&self) -> RuleId {
        RuleId::new("redundant-env-null")
    }

    fn name(&self) -> &str {
        "Redundant Env Null"
    }

    fn description(&self) -> &str {
        "Detects env null entries that remove keys not in the base step"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for (step_name, step_config) in &config.steps {
            for (env_name, overrides) in &step_config.environments {
                for (key, value) in &overrides.env {
                    if value.is_none() && !step_config.env.contains_key(key) {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Step '{}' environment '{}' removes '{}' but it's not in the base env",
                                    step_name, env_name, key
                                ),
                            )
                            .with_suggestion(format!(
                                "Remove the null entry for '{}' or add it to the base step env",
                                key
                            )),
                        );
                    }
                }
            }
        }

        diagnostics
    }
}

/// Detects circular dependencies created by per-environment dependency overrides.
pub struct EnvironmentCircularDependencyRule;

impl EnvironmentCircularDependencyRule {
    /// Check if a dependency graph has a cycle using DFS.
    fn has_cycle(deps: &std::collections::HashMap<&str, Vec<&str>>) -> Option<String> {
        enum State {
            Visiting,
            Visited,
        }

        let mut states: std::collections::HashMap<&str, State> = std::collections::HashMap::new();

        fn dfs<'a>(
            node: &'a str,
            deps: &std::collections::HashMap<&'a str, Vec<&'a str>>,
            states: &mut std::collections::HashMap<&'a str, State>,
            path: &mut Vec<&'a str>,
        ) -> Option<String> {
            match states.get(node) {
                Some(State::Visiting) => {
                    path.push(node);
                    let cycle_start = path.iter().position(|&n| n == node).unwrap();
                    return Some(path[cycle_start..].join(" -> "));
                }
                Some(State::Visited) => return None,
                None => {}
            }

            states.insert(node, State::Visiting);
            path.push(node);

            if let Some(neighbors) = deps.get(node) {
                for &neighbor in neighbors {
                    if let Some(cycle) = dfs(neighbor, deps, states, path) {
                        return Some(cycle);
                    }
                }
            }

            path.pop();
            states.insert(node, State::Visited);
            None
        }

        for &node in deps.keys() {
            if !states.contains_key(node) {
                let mut path = Vec::new();
                if let cycle @ Some(_) = dfs(node, deps, &mut states, &mut path) {
                    return cycle;
                }
            }
        }

        None
    }
}

impl LintRule for EnvironmentCircularDependencyRule {
    fn id(&self) -> RuleId {
        RuleId::new("environment-circular-dependency")
    }

    fn name(&self) -> &str {
        "Environment Circular Dependency"
    }

    fn description(&self) -> &str {
        "Detects circular dependencies in per-environment dependency graphs"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        // Collect all environment names that have overrides with depends_on
        let mut env_names: HashSet<&str> = HashSet::new();
        for step_config in config.steps.values() {
            for (env_name, overrides) in &step_config.environments {
                if overrides.depends_on.is_some() {
                    env_names.insert(env_name.as_str());
                }
            }
        }

        // For each environment, build the effective dependency graph and check for cycles
        for env_name in &env_names {
            let mut deps: std::collections::HashMap<&str, Vec<&str>> =
                std::collections::HashMap::new();

            for (step_name, step_config) in &config.steps {
                let base_deps = &step_config.depends_on;
                let effective_deps = step_config
                    .environments
                    .get(*env_name)
                    .and_then(|o| o.depends_on.as_ref())
                    .unwrap_or(base_deps);

                deps.insert(
                    step_name.as_str(),
                    effective_deps.iter().map(|s| s.as_str()).collect(),
                );
            }

            if let Some(cycle) = Self::has_cycle(&deps) {
                diagnostics.push(
                    LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!(
                            "Circular dependency in '{}' environment: {}",
                            env_name, cycle
                        ),
                    )
                    .with_suggestion(
                        "Remove or adjust depends_on overrides to break the cycle".to_string(),
                    ),
                );
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{
        EnvironmentConfig, EnvironmentDetectRule, StepConfig, StepEnvironmentOverride,
        WorkflowConfig,
    };
    use std::collections::HashMap;

    // --- UnknownEnvironmentInStepRule tests ---

    #[test]
    fn unknown_env_in_step_detects_unknown() {
        let rule = UnknownEnvironmentInStepRule;

        let mut envs = HashMap::new();
        envs.insert(
            "staging".to_string(),
            StepEnvironmentOverride {
                command: Some("echo staging".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: envs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("staging"));
        assert!(diagnostics[0].message.contains("test"));
    }

    #[test]
    fn unknown_env_in_step_passes_for_builtin() {
        let rule = UnknownEnvironmentInStepRule;

        let mut envs = HashMap::new();
        envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: envs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unknown_env_in_step_passes_for_defined() {
        let rule = UnknownEnvironmentInStepRule;

        let mut settings_envs = HashMap::new();
        settings_envs.insert(
            "staging".to_string(),
            EnvironmentConfig {
                detect: vec![EnvironmentDetectRule {
                    env: "STAGING".to_string(),
                    value: None,
                }],
                ..Default::default()
            },
        );

        let mut step_envs = HashMap::new();
        step_envs.insert(
            "staging".to_string(),
            StepEnvironmentOverride {
                command: Some("echo staging".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                ..Default::default()
            },
        );
        let mut config = BivvyConfig {
            steps,
            ..Default::default()
        };
        config.settings.environments = settings_envs;

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unknown_env_in_step_no_environments_no_diagnostics() {
        let rule = UnknownEnvironmentInStepRule;

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- UnknownEnvironmentInOnlyRule tests ---

    #[test]
    fn unknown_env_in_only_detects_unknown() {
        let rule = UnknownEnvironmentInOnlyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                only_environments: vec!["staging".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("staging"));
    }

    #[test]
    fn unknown_env_in_only_passes_for_builtin() {
        let rule = UnknownEnvironmentInOnlyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                only_environments: vec!["ci".to_string(), "docker".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unknown_env_in_only_empty_list_no_diagnostics() {
        let rule = UnknownEnvironmentInOnlyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                only_environments: vec![],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- EnvironmentDefaultWorkflowMissingRule tests ---

    #[test]
    fn env_default_workflow_missing_detects_missing() {
        let rule = EnvironmentDefaultWorkflowMissingRule;

        let mut envs = HashMap::new();
        envs.insert(
            "ci".to_string(),
            EnvironmentConfig {
                default_workflow: Some("nonexistent".to_string()),
                ..Default::default()
            },
        );
        let mut config = BivvyConfig::default();
        config.settings.environments = envs;
        config
            .workflows
            .insert("default".to_string(), WorkflowConfig::default());

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("nonexistent"));
        assert!(diagnostics[0].message.contains("ci"));
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn env_default_workflow_missing_passes_when_exists() {
        let rule = EnvironmentDefaultWorkflowMissingRule;

        let mut envs = HashMap::new();
        envs.insert(
            "ci".to_string(),
            EnvironmentConfig {
                default_workflow: Some("quick".to_string()),
                ..Default::default()
            },
        );
        let mut config = BivvyConfig::default();
        config.settings.environments = envs;
        config
            .workflows
            .insert("quick".to_string(), WorkflowConfig::default());

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn env_default_workflow_missing_passes_when_none() {
        let rule = EnvironmentDefaultWorkflowMissingRule;

        let mut envs = HashMap::new();
        envs.insert(
            "ci".to_string(),
            EnvironmentConfig {
                default_workflow: None,
                ..Default::default()
            },
        );
        let mut config = BivvyConfig::default();
        config.settings.environments = envs;

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- UnreachableEnvironmentOverrideRule tests ---

    #[test]
    fn unreachable_override_detects_excluded() {
        let rule = UnreachableEnvironmentOverrideRule;

        let mut step_envs = HashMap::new();
        step_envs.insert(
            "staging".to_string(),
            StepEnvironmentOverride {
                command: Some("echo staging".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                only_environments: vec!["ci".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("staging"));
        assert!(diagnostics[0].message.contains("only_environments"));
    }

    #[test]
    fn unreachable_override_passes_when_included() {
        let rule = UnreachableEnvironmentOverrideRule;

        let mut step_envs = HashMap::new();
        step_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                only_environments: vec!["ci".to_string(), "staging".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unreachable_override_empty_only_means_all() {
        let rule = UnreachableEnvironmentOverrideRule;

        let mut step_envs = HashMap::new();
        step_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci".to_string()),
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                only_environments: vec![], // empty = all environments
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- CustomEnvironmentShadowsBuiltinRule tests ---

    #[test]
    fn shadows_builtin_detects_ci() {
        let rule = CustomEnvironmentShadowsBuiltinRule;

        let mut envs = HashMap::new();
        envs.insert(
            "ci".to_string(),
            EnvironmentConfig {
                provided_requirements: vec!["ruby".to_string()],
                ..Default::default()
            },
        );
        let mut config = BivvyConfig::default();
        config.settings.environments = envs;

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("ci"));
        assert!(diagnostics[0].message.contains("shadows"));
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn shadows_builtin_passes_for_custom() {
        let rule = CustomEnvironmentShadowsBuiltinRule;

        let mut envs = HashMap::new();
        envs.insert("staging".to_string(), EnvironmentConfig::default());
        let mut config = BivvyConfig::default();
        config.settings.environments = envs;

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn shadows_builtin_no_environments_no_diagnostics() {
        let rule = CustomEnvironmentShadowsBuiltinRule;

        let config = BivvyConfig::default();
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- RedundantEnvironmentOverrideRule tests ---

    #[test]
    fn lint_redundant_environment_override() {
        let rule = RedundantEnvironmentOverrideRule;

        let mut step_envs = HashMap::new();
        step_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo test".to_string()), // Same as base
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("redundant"));
        assert!(diagnostics[0].message.contains("command"));
        assert_eq!(diagnostics[0].severity, Severity::Hint);
    }

    #[test]
    fn lint_redundant_override_passes_when_different() {
        let rule = RedundantEnvironmentOverrideRule;

        let mut step_envs = HashMap::new();
        step_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                command: Some("echo ci".to_string()), // Different from base
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- RedundantEnvNullRule tests ---

    #[test]
    fn lint_ambiguous_env_null() {
        let rule = RedundantEnvNullRule;

        let mut env_overrides = HashMap::new();
        env_overrides.insert("FOO".to_string(), None); // null, but FOO not in base
        let mut step_envs = HashMap::new();
        step_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                env: env_overrides,
                ..Default::default()
            },
        );
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                environments: step_envs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("FOO"));
        assert_eq!(diagnostics[0].severity, Severity::Hint);
    }

    #[test]
    fn lint_env_null_passes_when_key_exists_in_base() {
        let rule = RedundantEnvNullRule;

        let mut env_overrides = HashMap::new();
        env_overrides.insert("FOO".to_string(), None); // null, and FOO IS in base
        let mut step_envs = HashMap::new();
        step_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                env: env_overrides,
                ..Default::default()
            },
        );
        let mut base_env = HashMap::new();
        base_env.insert("FOO".to_string(), "bar".to_string());
        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                env: base_env,
                environments: step_envs,
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- EnvironmentCircularDependencyRule tests ---

    #[test]
    fn lint_env_circular_dependency_per_environment() {
        let rule = EnvironmentCircularDependencyRule;

        // Base graph: a -> b (no cycle)
        // CI override: b -> a (creates cycle a -> b -> a)
        let mut step_a_envs = HashMap::new();
        step_a_envs.insert("ci".to_string(), StepEnvironmentOverride::default());
        let mut step_b_envs = HashMap::new();
        step_b_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                depends_on: Some(vec!["a".to_string()]),
                ..Default::default()
            },
        );

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec![],
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                depends_on: vec!["a".to_string()],
                environments: step_b_envs,
                ..Default::default()
            },
        );
        // In CI: a depends on nothing, b depends on a (from base)
        // Wait - we need b -> a AND a -> b to create a cycle.
        // Let's make the CI override for "a" depend on "b"
        let mut steps = HashMap::new();
        let mut step_a_envs = HashMap::new();
        step_a_envs.insert(
            "ci".to_string(),
            StepEnvironmentOverride {
                depends_on: Some(vec!["b".to_string()]),
                ..Default::default()
            },
        );
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec![],
                environments: step_a_envs,
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                depends_on: vec!["a".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("ci"));
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn lint_env_circular_dependency_base_ok() {
        let rule = EnvironmentCircularDependencyRule;

        // Base graph: a -> b -> c (no cycle, no env overrides)
        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                depends_on: vec!["a".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "c".to_string(),
            StepConfig {
                command: Some("echo c".to_string()),
                depends_on: vec!["b".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }
}
