//! Requirement validation rules.
//!
//! These rules validate `requires` declarations on steps against the
//! requirement registry to catch misconfigured or unknown requirements early.

use std::collections::HashSet;

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};
use crate::requirements::registry::{RequirementCheck, RequirementRegistry};

/// Detects `requires` entries that reference unknown requirements.
///
/// A requirement is "known" if it exists in the built-in registry or
/// has been defined as a custom requirement in the config.
pub struct UnknownRequirementRule {
    registry: RequirementRegistry,
}

impl UnknownRequirementRule {
    /// Create a new rule with the given requirement registry.
    pub fn new(registry: RequirementRegistry) -> Self {
        Self { registry }
    }
}

impl LintRule for UnknownRequirementRule {
    fn id(&self) -> RuleId {
        RuleId::new("unknown-requirement")
    }

    fn name(&self) -> &str {
        "Unknown Requirement"
    }

    fn description(&self) -> &str {
        "Ensures all requires entries reference known requirements"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let known: HashSet<&str> = self.registry.known_names().into_iter().collect();

        for (step_name, step_config) in &config.steps {
            for req in &step_config.requires {
                if !known.contains(req.as_str()) {
                    diagnostics.push(
                        LintDiagnostic::new(
                            self.id(),
                            self.default_severity(),
                            format!(
                                "Step '{}' requires unknown requirement '{}'",
                                step_name, req
                            ),
                        )
                        .with_suggestion(format!(
                            "Define '{}' in the requirements section or use a built-in name",
                            req
                        )),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Detects circular dependencies in the requirement dependency chain.
///
/// For example, if requirement A depends on B and B depends on A,
/// this rule will flag the cycle.
pub struct CircularRequirementDepRule {
    registry: RequirementRegistry,
}

impl CircularRequirementDepRule {
    /// Create a new rule with the given requirement registry.
    pub fn new(registry: RequirementRegistry) -> Self {
        Self { registry }
    }
}

impl LintRule for CircularRequirementDepRule {
    fn id(&self) -> RuleId {
        RuleId::new("circular-requirement-dep")
    }

    fn name(&self) -> &str {
        "Circular Requirement Dependency"
    }

    fn description(&self) -> &str {
        "Detects circular dependencies in requirement chains"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut checked = HashSet::new();

        // Collect all requirement names referenced across steps
        let mut all_reqs = HashSet::new();
        for step_config in config.steps.values() {
            for req in &step_config.requires {
                all_reqs.insert(req.as_str());
            }
        }

        // Check each referenced requirement for circular deps
        for req_name in all_reqs {
            if checked.contains(req_name) {
                continue;
            }
            if let Some(cycle) = self.detect_cycle(req_name) {
                diagnostics.push(LintDiagnostic::new(
                    self.id(),
                    self.default_severity(),
                    format!("Circular requirement dependency: {}", cycle),
                ));
            }
            checked.insert(req_name);
        }

        diagnostics
    }
}

impl CircularRequirementDepRule {
    fn detect_cycle(&self, start: &str) -> Option<String> {
        let mut path = vec![start.to_string()];
        let mut visited = HashSet::new();
        visited.insert(start.to_string());

        self.walk_deps(start, &mut path, &mut visited)
    }

    fn walk_deps(
        &self,
        current: &str,
        path: &mut Vec<String>,
        visited: &mut HashSet<String>,
    ) -> Option<String> {
        if let Some(req) = self.registry.get(current) {
            for dep in &req.depends_on {
                if !visited.insert(dep.clone()) {
                    // Already visited — cycle found
                    path.push(dep.clone());
                    return Some(path.join(" -> "));
                }
                path.push(dep.clone());
                if let Some(cycle) = self.walk_deps(dep, path, visited) {
                    return Some(cycle);
                }
                path.pop();
            }
        }
        None
    }
}

/// Warns when a requirement has no install_template defined.
///
/// Without an install template, bivvy cannot offer to auto-install
/// the missing requirement.
pub struct InstallTemplateMissingRule {
    registry: RequirementRegistry,
}

impl InstallTemplateMissingRule {
    /// Create a new rule with the given requirement registry.
    pub fn new(registry: RequirementRegistry) -> Self {
        Self { registry }
    }
}

impl LintRule for InstallTemplateMissingRule {
    fn id(&self) -> RuleId {
        RuleId::new("install-template-missing")
    }

    fn name(&self) -> &str {
        "Install Template Missing"
    }

    fn description(&self) -> &str {
        "Warns when a requirement has no install template"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut checked = HashSet::new();

        for step_config in config.steps.values() {
            for req_name in &step_config.requires {
                if !checked.insert(req_name.clone()) {
                    continue;
                }
                if let Some(req) = self.registry.get(req_name) {
                    if req.install_template.is_none() {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!("Requirement '{}' has no install template", req_name),
                            )
                            .with_suggestion(format!(
                                "Add an install_template to the '{}' requirement definition",
                                req_name
                            )),
                        );
                    }
                }
            }
        }

        diagnostics
    }
}

/// Warns when a service requirement has no install_hint.
///
/// Service requirements (those using ServiceReachable checks) should
/// have an install_hint so users know how to start or install the service.
pub struct ServiceRequirementWithoutHintRule {
    registry: RequirementRegistry,
}

impl ServiceRequirementWithoutHintRule {
    /// Create a new rule with the given requirement registry.
    pub fn new(registry: RequirementRegistry) -> Self {
        Self { registry }
    }
}

impl LintRule for ServiceRequirementWithoutHintRule {
    fn id(&self) -> RuleId {
        RuleId::new("service-requirement-without-hint")
    }

    fn name(&self) -> &str {
        "Service Requirement Without Hint"
    }

    fn description(&self) -> &str {
        "Warns when a service requirement has no install hint"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut checked = HashSet::new();

        for step_config in config.steps.values() {
            for req_name in &step_config.requires {
                if !checked.insert(req_name.clone()) {
                    continue;
                }
                if let Some(req) = self.registry.get(req_name) {
                    if is_service_requirement(req) && req.install_hint.is_none() {
                        diagnostics.push(
                            LintDiagnostic::new(
                                self.id(),
                                self.default_severity(),
                                format!(
                                    "Service requirement '{}' has no install hint",
                                    req_name
                                ),
                            )
                            .with_suggestion(format!(
                                "Add an install_hint to the '{}' requirement to help users start the service",
                                req_name
                            )),
                        );
                    }
                }
            }
        }

        diagnostics
    }
}

/// Check if a requirement uses service-type checks.
fn is_service_requirement(req: &crate::requirements::registry::Requirement) -> bool {
    req.checks.iter().any(is_service_check)
}

fn is_service_check(check: &RequirementCheck) -> bool {
    match check {
        RequirementCheck::ServiceReachable(_) => true,
        RequirementCheck::Any(checks) => checks.iter().any(is_service_check),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CustomRequirement, CustomRequirementCheck, StepConfig};
    use std::collections::HashMap;

    fn make_registry() -> RequirementRegistry {
        RequirementRegistry::new()
    }

    fn make_registry_with_custom(
        custom: HashMap<String, CustomRequirement>,
    ) -> RequirementRegistry {
        RequirementRegistry::new().with_custom(&custom)
    }

    // --- UnknownRequirementRule tests ---

    #[test]
    fn unknown_requirement_detects_unknown() {
        let registry = make_registry();
        let rule = UnknownRequirementRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["nonexistent-tool-xyz".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("nonexistent-tool-xyz"));
        assert!(diagnostics[0].suggestion.is_some());
    }

    #[test]
    fn unknown_requirement_passes_with_builtin() {
        let registry = make_registry();
        let rule = UnknownRequirementRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["ruby".to_string()],
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
    fn unknown_requirement_passes_with_custom() {
        let mut custom = HashMap::new();
        custom.insert(
            "my-tool".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::CommandSucceeds {
                    command: "my-tool --version".to_string(),
                },
                install_template: None,
                install_hint: None,
            },
        );
        let registry = make_registry_with_custom(custom);
        let rule = UnknownRequirementRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["my-tool".to_string()],
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
    fn unknown_requirement_no_requires_no_diagnostics() {
        let registry = make_registry();
        let rule = UnknownRequirementRule::new(registry);

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

    #[test]
    fn unknown_requirement_multiple_unknowns() {
        let registry = make_registry();
        let rule = UnknownRequirementRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["foo".to_string(), "bar".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 2);
    }

    // --- CircularRequirementDepRule tests ---

    #[test]
    fn circular_dep_no_cycles_in_builtins() {
        let registry = make_registry();
        let rule = CircularRequirementDepRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["ruby".to_string(), "node".to_string()],
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
    fn circular_dep_no_requires_no_diagnostics() {
        let registry = make_registry();
        let rule = CircularRequirementDepRule::new(registry);

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

    #[test]
    fn circular_dep_unknown_requirement_no_crash() {
        let registry = make_registry();
        let rule = CircularRequirementDepRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["unknown-thing".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        // Should not panic, just produce no cycle diagnostics
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- InstallTemplateMissingRule tests ---

    #[test]
    fn install_template_missing_passes_for_builtins() {
        let registry = make_registry();
        let rule = InstallTemplateMissingRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["ruby".to_string()],
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
    fn install_template_missing_warns_for_custom_without_template() {
        let mut custom = HashMap::new();
        custom.insert(
            "my-tool".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::CommandSucceeds {
                    command: "my-tool --version".to_string(),
                },
                install_template: None,
                install_hint: Some("Install my-tool manually".to_string()),
            },
        );
        let registry = make_registry_with_custom(custom);
        let rule = InstallTemplateMissingRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["my-tool".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("my-tool"));
        assert_eq!(diagnostics[0].severity, Severity::Hint);
    }

    #[test]
    fn install_template_missing_no_duplicate_for_same_requirement() {
        let mut custom = HashMap::new();
        custom.insert(
            "my-tool".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::CommandSucceeds {
                    command: "my-tool --version".to_string(),
                },
                install_template: None,
                install_hint: None,
            },
        );
        let registry = make_registry_with_custom(custom);
        let rule = InstallTemplateMissingRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "step_a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                requires: vec!["my-tool".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "step_b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                requires: vec!["my-tool".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        // Should only produce one diagnostic even though two steps reference it
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn install_template_missing_no_requires_no_diagnostics() {
        let registry = make_registry();
        let rule = InstallTemplateMissingRule::new(registry);

        let config = BivvyConfig::default();
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    // --- ServiceRequirementWithoutHintRule tests ---

    #[test]
    fn service_without_hint_passes_for_builtins() {
        let registry = make_registry();
        let rule = ServiceRequirementWithoutHintRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["postgres-server".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        // Built-in postgres-server has an install_hint
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn service_without_hint_warns_for_custom_service() {
        let mut custom = HashMap::new();
        custom.insert(
            "my-service".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::ServiceReachable {
                    command: "my-service-check".to_string(),
                },
                install_template: None,
                install_hint: None,
            },
        );
        let registry = make_registry_with_custom(custom);
        let rule = ServiceRequirementWithoutHintRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["my-service".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("my-service"));
    }

    #[test]
    fn service_without_hint_passes_for_non_service() {
        let mut custom = HashMap::new();
        custom.insert(
            "my-tool".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::CommandSucceeds {
                    command: "my-tool --version".to_string(),
                },
                install_template: None,
                install_hint: None,
            },
        );
        let registry = make_registry_with_custom(custom);
        let rule = ServiceRequirementWithoutHintRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["my-tool".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        // CommandSucceeds is not a service check
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn service_without_hint_passes_when_hint_provided() {
        let mut custom = HashMap::new();
        custom.insert(
            "my-service".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::ServiceReachable {
                    command: "my-service-check".to_string(),
                },
                install_template: None,
                install_hint: Some("Start with: my-service start".to_string()),
            },
        );
        let registry = make_registry_with_custom(custom);
        let rule = ServiceRequirementWithoutHintRule::new(registry);

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            StepConfig {
                command: Some("echo test".to_string()),
                requires: vec!["my-service".to_string()],
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
    fn service_without_hint_no_requires_no_diagnostics() {
        let registry = make_registry();
        let rule = ServiceRequirementWithoutHintRule::new(registry);

        let config = BivvyConfig::default();
        let diagnostics = rule.check(&config);
        assert!(diagnostics.is_empty());
    }
}
