//! Rule registry for managing lint rules.
//!
//! The [`RuleRegistry`] stores all available lint rules and provides
//! methods for registering, retrieving, and iterating over them.

use std::collections::HashMap;

use super::rule::{LintRule, RuleId};
use super::rules::{
    AppNameRule, CircularDependencyRule, CustomEnvironmentShadowsBuiltinRule,
    EnvironmentDefaultWorkflowMissingRule, RequiredFieldsRule, SelfDependencyRule,
    UndefinedDependencyRule, UnknownEnvironmentInOnlyRule, UnknownEnvironmentInStepRule,
    UnreachableEnvironmentOverrideRule,
};

/// Registry of all available lint rules.
pub struct RuleRegistry {
    rules: HashMap<RuleId, Box<dyn LintRule>>,
}

impl RuleRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// Create a registry with all built-in rules.
    ///
    /// This registers all the default validation rules that come with Bivvy.
    /// Note: Template-related rules (UndefinedTemplateRule, TemplateInputsRule)
    /// require a Registry and must be registered separately.
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(AppNameRule));
        registry.register(Box::new(RequiredFieldsRule));
        registry.register(Box::new(CircularDependencyRule));
        registry.register(Box::new(SelfDependencyRule));
        registry.register(Box::new(UndefinedDependencyRule));
        registry.register(Box::new(UnknownEnvironmentInStepRule));
        registry.register(Box::new(UnknownEnvironmentInOnlyRule));
        registry.register(Box::new(EnvironmentDefaultWorkflowMissingRule));
        registry.register(Box::new(UnreachableEnvironmentOverrideRule));
        registry.register(Box::new(CustomEnvironmentShadowsBuiltinRule));
        registry
    }

    /// Register a lint rule.
    pub fn register(&mut self, rule: Box<dyn LintRule>) {
        self.rules.insert(rule.id(), rule);
    }

    /// Get a rule by ID.
    pub fn get(&self, id: &RuleId) -> Option<&dyn LintRule> {
        self.rules.get(id).map(|r| r.as_ref())
    }

    /// Iterate over all rules.
    pub fn iter(&self) -> impl Iterator<Item = &dyn LintRule> {
        self.rules.values().map(|r| r.as_ref())
    }

    /// Get the number of registered rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BivvyConfig;
    use crate::lint::{LintDiagnostic, Severity};

    struct MockRule {
        id: RuleId,
    }

    impl LintRule for MockRule {
        fn id(&self) -> RuleId {
            self.id.clone()
        }
        fn name(&self) -> &str {
            "Mock Rule"
        }
        fn description(&self) -> &str {
            "A mock rule for testing"
        }
        fn default_severity(&self) -> Severity {
            Severity::Warning
        }
        fn check(&self, _config: &BivvyConfig) -> Vec<LintDiagnostic> {
            vec![]
        }
    }

    #[test]
    fn registry_new_is_empty() {
        let registry = RuleRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_register_and_get() {
        let mut registry = RuleRegistry::new();
        let rule = MockRule {
            id: RuleId::new("mock"),
        };

        registry.register(Box::new(rule));

        assert!(!registry.is_empty());
        assert!(registry.get(&RuleId::new("mock")).is_some());
        assert!(registry.get(&RuleId::new("unknown")).is_none());
    }

    #[test]
    fn registry_iteration() {
        let mut registry = RuleRegistry::new();
        registry.register(Box::new(MockRule {
            id: RuleId::new("rule1"),
        }));
        registry.register(Box::new(MockRule {
            id: RuleId::new("rule2"),
        }));

        assert_eq!(registry.len(), 2);
        assert_eq!(registry.iter().count(), 2);
    }

    #[test]
    fn registry_default_is_empty() {
        let registry = RuleRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_with_builtins_has_rules() {
        let registry = RuleRegistry::with_builtins();
        assert!(!registry.is_empty());
        // Should have at least 10 built-in rules
        assert!(registry.len() >= 10);
        // Verify some specific rules are registered
        assert!(registry.get(&RuleId::new("app-name-format")).is_some());
        assert!(registry.get(&RuleId::new("required-fields")).is_some());
        assert!(registry.get(&RuleId::new("circular-dependency")).is_some());
        assert!(registry.get(&RuleId::new("self-dependency")).is_some());
        assert!(registry.get(&RuleId::new("undefined-dependency")).is_some());
        assert!(registry
            .get(&RuleId::new("unknown-environment-in-step"))
            .is_some());
        assert!(registry
            .get(&RuleId::new("unknown-environment-in-only"))
            .is_some());
        assert!(registry
            .get(&RuleId::new("environment-default-workflow-missing"))
            .is_some());
        assert!(registry
            .get(&RuleId::new("unreachable-environment-override"))
            .is_some());
        assert!(registry
            .get(&RuleId::new("custom-environment-shadows-builtin"))
            .is_some());
    }
}
