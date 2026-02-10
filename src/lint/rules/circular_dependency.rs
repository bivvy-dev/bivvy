//! Circular dependency detection.
//!
//! This rule detects circular dependencies between steps in the configuration.

use std::collections::HashSet;

use crate::config::BivvyConfig;
use crate::lint::{LintDiagnostic, LintRule, RuleId, Severity};

/// Detects circular dependencies between steps.
pub struct CircularDependencyRule;

impl LintRule for CircularDependencyRule {
    fn id(&self) -> RuleId {
        RuleId::new("circular-dependency")
    }

    fn name(&self) -> &str {
        "Circular Dependency"
    }

    fn description(&self) -> &str {
        "Detects circular dependencies in step depends_on"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, config: &BivvyConfig) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut reported = HashSet::new();

        // Use DFS to detect cycles from each starting point
        for step_name in config.steps.keys() {
            if let Some(cycle) = self.find_cycle(config, step_name) {
                // Only report each cycle once (by the first step in the cycle)
                let cycle_key = {
                    let mut sorted = cycle.clone();
                    sorted.sort();
                    sorted.join(",")
                };
                if !reported.contains(&cycle_key) {
                    reported.insert(cycle_key);
                    diagnostics.push(LintDiagnostic::new(
                        self.id(),
                        self.default_severity(),
                        format!("Circular dependency detected: {}", cycle.join(" -> ")),
                    ));
                }
            }
        }

        diagnostics
    }
}

impl CircularDependencyRule {
    fn find_cycle(&self, config: &BivvyConfig, start: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        self.dfs(config, start, &mut visited, &mut path)
    }

    fn dfs(
        &self,
        config: &BivvyConfig,
        current: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if path.contains(&current.to_string()) {
            path.push(current.to_string());
            return Some(path.clone());
        }
        if visited.contains(current) {
            return None;
        }

        visited.insert(current.to_string());
        path.push(current.to_string());

        if let Some(step) = config.steps.get(current) {
            for dep in &step.depends_on {
                if let Some(cycle) = self.dfs(config, dep, visited, path) {
                    return Some(cycle);
                }
            }
        }

        path.pop();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StepConfig;
    use std::collections::HashMap;

    #[test]
    fn detects_simple_cycle() {
        let rule = CircularDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["b".to_string()],
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

        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("Circular dependency"));
    }

    #[test]
    fn detects_three_step_cycle() {
        let rule = CircularDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["b".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                depends_on: vec!["c".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "c".to_string(),
            StepConfig {
                command: Some("echo c".to_string()),
                depends_on: vec!["a".to_string()],
                ..Default::default()
            },
        );
        let config = BivvyConfig {
            steps,
            ..Default::default()
        };

        let diagnostics = rule.check(&config);

        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn no_cycle_in_valid_deps() {
        let rule = CircularDependencyRule;

        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["b".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
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
    fn no_cycle_in_diamond_deps() {
        let rule = CircularDependencyRule;

        // a -> b, a -> c, b -> d, c -> d (diamond pattern, no cycle)
        let mut steps = HashMap::new();
        steps.insert(
            "a".to_string(),
            StepConfig {
                command: Some("echo a".to_string()),
                depends_on: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "b".to_string(),
            StepConfig {
                command: Some("echo b".to_string()),
                depends_on: vec!["d".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "c".to_string(),
            StepConfig {
                command: Some("echo c".to_string()),
                depends_on: vec!["d".to_string()],
                ..Default::default()
            },
        );
        steps.insert(
            "d".to_string(),
            StepConfig {
                command: Some("echo d".to_string()),
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
