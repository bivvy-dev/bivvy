//! Dependency resolution for requirements.
//!
//! `DependencyResolver` walks the dependency graph for a requirement,
//! producing an ordered install chain with cycle detection and depth
//! limiting.

use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::{InstallContext, Platform, RequirementRegistry};
use std::collections::HashSet;

/// Resolves install dependency chains for requirements.
///
/// Stateless: holds references to the registry and probe but does not
/// cache results. Used by [`super::GapChecker`] as a delegate.
pub(super) struct DependencyResolver<'a> {
    pub(super) registry: &'a RequirementRegistry,
    pub(super) probe: &'a EnvironmentProbe,
}

impl<'a> DependencyResolver<'a> {
    /// Resolve the install dependency chain for a requirement.
    ///
    /// Returns the ordered list of requirements that need to be installed,
    /// with dependencies before dependents. Max depth 5, detects circular deps.
    pub(super) fn resolve(&self, requirement: &str) -> Result<Vec<String>, String> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        self.resolve_recursive(requirement, &mut chain, &mut visited, 0)?;
        Ok(chain)
    }

    fn resolve_recursive(
        &self,
        requirement: &str,
        chain: &mut Vec<String>,
        visited: &mut HashSet<String>,
        depth: usize,
    ) -> Result<(), String> {
        const MAX_DEPTH: usize = 5;

        if depth > MAX_DEPTH {
            return Err(format!(
                "Requirement dependency chain exceeds max depth of {} for '{}'",
                MAX_DEPTH, requirement
            ));
        }

        if visited.contains(requirement) {
            return Err(format!(
                "Circular dependency detected: '{}' appears twice in the chain",
                requirement
            ));
        }

        let Some(req) = self.registry.get(requirement) else {
            // Unknown requirement -- add it to chain, let caller handle
            if !chain.contains(&requirement.to_string()) {
                chain.push(requirement.to_string());
            }
            return Ok(());
        };

        visited.insert(requirement.to_string());

        // Resolve static dependencies
        for dep in &req.depends_on {
            self.resolve_recursive(dep, chain, visited, depth + 1)?;
        }

        // Resolve dynamic dependencies
        if let Some(install_requires_fn) = req.install_requires {
            let ctx = self.build_install_context();
            let dynamic_deps = install_requires_fn(&ctx);
            for dep in &dynamic_deps {
                if !visited.contains(dep) {
                    self.resolve_recursive(dep, chain, visited, depth + 1)?;
                }
            }
        }

        // Add this requirement after its dependencies
        if !chain.contains(&requirement.to_string()) {
            chain.push(requirement.to_string());
        }

        visited.remove(requirement);
        Ok(())
    }

    fn build_install_context(&self) -> InstallContext {
        let detected_managers = self
            .probe
            .inactive_managers()
            .iter()
            .map(|m| m.name.clone())
            .collect::<Vec<_>>();
        InstallContext {
            detected_managers,
            platform: Platform::current(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requirements::probe::EnvironmentProbe;
    use crate::requirements::registry::{Requirement, RequirementCheck, RequirementRegistry};

    fn make_probe() -> EnvironmentProbe {
        EnvironmentProbe::run_with_env(|_| Err(std::env::VarError::NotPresent))
    }

    fn make_resolver<'a>(
        registry: &'a RequirementRegistry,
        probe: &'a EnvironmentProbe,
    ) -> DependencyResolver<'a> {
        DependencyResolver { registry, probe }
    }

    #[test]
    fn ruby_depends_on_mise_when_no_manager() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("ruby").unwrap();
        assert_eq!(chain, vec!["mise", "ruby"]);
    }

    #[test]
    fn dependency_chain_resolves_in_order() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("ruby").unwrap();
        let mise_idx = chain.iter().position(|s| s == "mise").unwrap();
        let ruby_idx = chain.iter().position(|s| s == "ruby").unwrap();
        assert!(mise_idx < ruby_idx, "mise should come before ruby in chain");
    }

    #[test]
    fn node_depends_on_mise_when_no_manager() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("node").unwrap();
        assert_eq!(chain, vec!["mise", "node"]);
    }

    #[test]
    fn unknown_requirement_in_deps_still_included() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("nonexistent-tool-xyz").unwrap();
        assert_eq!(chain, vec!["nonexistent-tool-xyz"]);
    }

    #[test]
    fn requirement_with_no_deps_resolves_to_self() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("mise").unwrap();
        assert_eq!(chain, vec!["mise"]);
    }

    #[test]
    fn max_depth_prevents_infinite_loop() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        for name in registry.known_names() {
            let result = resolver.resolve(name);
            assert!(
                result.is_ok(),
                "Failed to resolve deps for {}: {:?}",
                name,
                result.err()
            );
        }
    }

    #[test]
    fn resolve_deps_does_not_duplicate() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("ruby").unwrap();
        let mise_count = chain.iter().filter(|s| s.as_str() == "mise").count();
        assert_eq!(mise_count, 1, "mise should appear exactly once in chain");
    }

    #[test]
    fn python_depends_on_mise_when_no_manager() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let chain = resolver.resolve("python").unwrap();
        assert_eq!(chain, vec!["mise", "python"]);
    }

    #[test]
    fn circular_install_deps_detected() {
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "tool_a".to_string(),
            Requirement {
                name: "tool_a".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds("false".to_string())],
                install_template: None,
                install_hint: None,
                depends_on: vec!["tool_b".to_string()],
                install_requires: None,
            },
        );
        registry.insert(
            "tool_b".to_string(),
            Requirement {
                name: "tool_b".to_string(),
                checks: vec![RequirementCheck::CommandSucceeds("false".to_string())],
                install_template: None,
                install_hint: None,
                depends_on: vec!["tool_a".to_string()],
                install_requires: None,
            },
        );

        let probe = make_probe();
        let resolver = make_resolver(&registry, &probe);

        let result = resolver.resolve("tool_a");
        assert!(result.is_err(), "circular deps should be detected");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("Circular dependency"),
            "error should mention circular dependency, got: {}",
            err_msg
        );
    }
}
