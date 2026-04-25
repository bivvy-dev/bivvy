//! Gap checker for requirement evaluation.
//!
//! The `GapChecker` evaluates whether system-level requirements are met,
//! caching results within a run to avoid redundant command executions.

mod deps;
mod evaluator;
mod network;

use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::RequirementRegistry;
use crate::requirements::status::{GapResult, RequirementStatus};
use crate::steps::resolved::ResolvedStep;
use deps::DependencyResolver;
use evaluator::RequirementEvaluator;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Checks whether requirements are satisfied on the system.
///
/// Caches results per-run so the same requirement checked multiple times
/// only executes commands once.
pub struct GapChecker<'a> {
    evaluator: RequirementEvaluator<'a>,
    resolver: DependencyResolver<'a>,
    pub(crate) cache: HashMap<String, RequirementStatus>,
}

impl<'a> GapChecker<'a> {
    /// Create a new gap checker.
    pub fn new(
        registry: &'a RequirementRegistry,
        probe: &'a EnvironmentProbe,
        project_root: &Path,
    ) -> Self {
        Self {
            evaluator: RequirementEvaluator {
                registry,
                probe,
                project_root: project_root.to_path_buf(),
            },
            resolver: DependencyResolver { registry, probe },
            cache: HashMap::new(),
        }
    }

    /// Check all requirements for a step, returning only non-satisfied gaps.
    ///
    /// `provided_requirements` is authoritative — if the environment says
    /// a requirement is provided, skip the check regardless of what the
    /// step's requires list says. Until environments exist, callers pass `None`.
    pub fn check_step(
        &mut self,
        step: &ResolvedStep,
        provided: Option<&HashSet<String>>,
    ) -> Vec<GapResult> {
        let mut gaps = Vec::new();
        for req_name in &step.requires {
            if let Some(provided_set) = provided {
                if provided_set.contains(req_name) {
                    continue;
                }
            }

            let status = self.check_one(req_name);
            if !status.is_satisfied() {
                gaps.push(GapResult {
                    requirement: req_name.clone(),
                    status,
                });
            }
        }
        gaps
    }

    /// Check a single requirement, using cache when available.
    pub fn check_one(&mut self, requirement: &str) -> RequirementStatus {
        if let Some(cached) = self.cache.get(requirement) {
            return cached.clone();
        }

        let status = self.evaluator.evaluate(requirement);
        self.cache.insert(requirement.to_string(), status.clone());
        status
    }

    /// Invalidate a cached result for a specific requirement.
    pub fn invalidate(&mut self, requirement: &str) {
        self.cache.remove(requirement);
    }

    /// Invalidate all cached results.
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Resolve the install dependency chain for a requirement.
    ///
    /// Returns the ordered list of requirements that need to be installed,
    /// with dependencies before dependents. Max depth 5, detects circular deps.
    pub fn resolve_install_deps(&self, requirement: &str) -> Result<Vec<String>, String> {
        self.resolver.resolve(requirement)
    }

    /// Check whether the network is reachable.
    ///
    /// Attempts a TCP connection to a well-known host with a 2-second timeout.
    /// Returns `true` if the connection succeeds, `false` otherwise.
    pub fn check_network(&self) -> bool {
        network::check_network()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StepConfig;
    use crate::requirements::registry::RequirementRegistry;
    use tempfile::TempDir;

    fn make_probe() -> EnvironmentProbe {
        EnvironmentProbe::run_with_env(|_| Err(std::env::VarError::NotPresent))
    }

    fn make_resolved_step(requires: Vec<String>) -> ResolvedStep {
        let config = StepConfig {
            command: Some("echo test".to_string()),
            requires,
            ..Default::default()
        };
        ResolvedStep::from_config("test", &config, None)
    }

    // --- Cache tests ---

    #[test]
    fn unknown_requirement_returns_unknown() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let status = checker.check_one("nonexistent-tool-xyz");
        assert!(matches!(status, RequirementStatus::Unknown));
    }

    #[test]
    fn gap_checker_caches_results() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let status1 = checker.check_one("nonexistent-tool-xyz");
        assert!(matches!(status1, RequirementStatus::Unknown));

        let status2 = checker.check_one("nonexistent-tool-xyz");
        assert!(matches!(status2, RequirementStatus::Unknown));

        assert!(checker.cache.contains_key("nonexistent-tool-xyz"));
    }

    #[test]
    fn gap_checker_invalidate_clears_cache() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        checker.check_one("nonexistent-tool-xyz");
        assert!(checker.cache.contains_key("nonexistent-tool-xyz"));

        checker.invalidate("nonexistent-tool-xyz");
        assert!(!checker.cache.contains_key("nonexistent-tool-xyz"));
    }

    #[test]
    fn gap_checker_invalidate_all_clears_all() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        checker.check_one("nonexistent-tool-xyz");
        checker.check_one("another-fake-tool");
        assert_eq!(checker.cache.len(), 2);

        checker.invalidate_all();
        assert!(checker.cache.is_empty());
    }

    // --- Step tests ---

    #[test]
    fn check_step_with_provided_skips_requirements() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let step = make_resolved_step(vec!["ruby".to_string(), "postgres-server".to_string()]);

        let mut provided = HashSet::new();
        provided.insert("postgres-server".to_string());

        let gaps = checker.check_step(&step, Some(&provided));

        assert!(!gaps.iter().any(|g| g.requirement == "postgres-server"));
    }

    #[test]
    fn check_step_with_none_provided_checks_all() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let step = make_resolved_step(vec!["nonexistent-tool-xyz".to_string()]);

        let gaps = checker.check_step(&step, None);

        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].requirement, "nonexistent-tool-xyz");
        assert!(matches!(gaps[0].status, RequirementStatus::Unknown));
    }

    #[test]
    fn check_step_filters_out_satisfied() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        checker
            .cache
            .insert("fake-tool".to_string(), RequirementStatus::Satisfied);

        let step = make_resolved_step(vec!["fake-tool".to_string()]);
        let gaps = checker.check_step(&step, None);

        assert!(gaps.is_empty());
    }

    #[test]
    fn check_step_empty_requires_returns_no_gaps() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let step = make_resolved_step(vec![]);
        let gaps = checker.check_step(&step, None);

        assert!(gaps.is_empty());
    }

    // --- Provided/override integration tests ---

    #[test]
    fn provided_and_override_compose() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let step = make_resolved_step(vec!["ruby".to_string(), "postgres-server".to_string()]);

        let mut provided = HashSet::new();
        provided.insert("postgres-server".to_string());

        let gaps = checker.check_step(&step, Some(&provided));

        assert!(
            !gaps.iter().any(|g| g.requirement == "postgres-server"),
            "provided requirement should be skipped"
        );
    }

    #[test]
    fn provided_wins_over_step_requires() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let step = make_resolved_step(vec!["nonexistent-tool-xyz".to_string()]);

        let mut provided = HashSet::new();
        provided.insert("nonexistent-tool-xyz".to_string());

        let gaps = checker.check_step(&step, Some(&provided));

        assert!(gaps.is_empty(), "provided requirement should suppress gap");
    }
}
