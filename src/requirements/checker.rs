//! Gap checker for requirement evaluation.
//!
//! The `GapChecker` evaluates whether system-level requirements are met,
//! caching results within a run to avoid redundant command executions.

use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::{RequirementCheck, RequirementRegistry};
use crate::requirements::status::{GapResult, RequirementStatus};
use crate::steps::resolved::ResolvedStep;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Checks whether requirements are satisfied on the system.
///
/// Caches results per-run so the same requirement checked multiple times
/// only executes commands once.
pub struct GapChecker<'a> {
    registry: &'a RequirementRegistry,
    probe: &'a EnvironmentProbe,
    project_root: PathBuf,
    cache: HashMap<String, RequirementStatus>,
}

impl<'a> GapChecker<'a> {
    /// Create a new gap checker.
    pub fn new(
        registry: &'a RequirementRegistry,
        probe: &'a EnvironmentProbe,
        project_root: &Path,
    ) -> Self {
        Self {
            registry,
            probe,
            project_root: project_root.to_path_buf(),
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

        let status = self.evaluate(requirement);
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

    fn evaluate(&self, requirement: &str) -> RequirementStatus {
        let Some(req) = self.registry.get(requirement) else {
            return RequirementStatus::Unknown;
        };

        for check in &req.checks {
            if let Some(status) = self.evaluate_check(check, req) {
                return status;
            }
        }

        // No check matched — Missing
        RequirementStatus::Missing {
            install_template: req.install_template.clone(),
            install_hint: req.install_hint.clone(),
        }
    }

    fn evaluate_check(
        &self,
        check: &RequirementCheck,
        req: &crate::requirements::registry::Requirement,
    ) -> Option<RequirementStatus> {
        match check {
            RequirementCheck::CommandSucceeds(cmd) => {
                if self.run_command_succeeds(cmd) {
                    Some(RequirementStatus::Satisfied)
                } else {
                    None
                }
            }
            RequirementCheck::FileExists(path) => {
                let full_path = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    self.project_root.join(path)
                };
                if full_path.exists() {
                    Some(RequirementStatus::Satisfied)
                } else {
                    None
                }
            }
            RequirementCheck::ServiceReachable(cmd) => {
                if self.run_command_succeeds(cmd) {
                    Some(RequirementStatus::Satisfied)
                } else {
                    // Service check is the main/only check — return ServiceDown
                    Some(RequirementStatus::ServiceDown {
                        binary_present: self.check_binary_present(cmd),
                        install_template: req.install_template.clone(),
                        start_command: None,
                        start_hint: req
                            .install_hint
                            .clone()
                            .unwrap_or_else(|| format!("Start the {} service", req.name)),
                    })
                }
            }
            RequirementCheck::ManagedCommand {
                command: tool,
                managed_path_patterns,
                system_path_patterns,
                version_file,
            } => self.evaluate_managed_command(
                tool,
                managed_path_patterns,
                system_path_patterns,
                version_file.as_deref(),
                req,
            ),
            RequirementCheck::Any(sub_checks) => {
                for sub in sub_checks {
                    if let Some(status) = self.evaluate_check(sub, req) {
                        if status.is_satisfied() || status.can_proceed() {
                            return Some(status);
                        }
                    }
                }
                None
            }
        }
    }

    fn evaluate_managed_command(
        &self,
        tool: &str,
        managed_path_patterns: &[String],
        system_path_patterns: &[String],
        version_file: Option<&str>,
        req: &crate::requirements::registry::Requirement,
    ) -> Option<RequirementStatus> {
        let full_path = self.probe.full_path();

        // Resolve where the tool is on the full (augmented) PATH
        if let Some(resolved) =
            crate::requirements::probe::resolve_tool_path(tool, full_path.as_slice())
        {
            let path_str = resolved.to_string_lossy().to_string();

            // Check 1: Managed path patterns -> Satisfied
            for pattern in managed_path_patterns {
                if path_str.contains(pattern) {
                    return Some(RequirementStatus::Satisfied);
                }
            }

            // Check 2: Inactive manager with version file
            if let Some(vf) = version_file {
                if self.project_root.join(vf).exists()
                    || self.project_root.join(".tool-versions").exists()
                {
                    // Check if the tool was found via augmented path (not original PATH)
                    let original_path = crate::requirements::probe::parse_system_path();
                    let on_original =
                        crate::requirements::probe::resolve_tool_path(tool, &original_path);
                    if on_original.is_none() {
                        // Tool is only on augmented path — it's from an inactive manager
                        for mgr in self.probe.inactive_managers() {
                            let mgr_binary = format!("{}/", mgr.name);
                            if path_str.contains(&mgr_binary)
                                || resolved.starts_with(&mgr.install_path)
                            {
                                return Some(RequirementStatus::Inactive {
                                    manager: mgr.name.clone(),
                                    binary_path: resolved,
                                    activation_hint: mgr.activation.clone(),
                                });
                            }
                        }
                    }
                }
            }

            // Check 3: System path patterns -> SystemOnly
            for pattern in system_path_patterns {
                if path_str.contains(pattern) || path_str.starts_with(pattern) {
                    return Some(RequirementStatus::SystemOnly {
                        path: resolved,
                        install_template: req.install_template.clone(),
                        warning: format!(
                            "System {} detected at {}. Consider using a version manager.",
                            tool, path_str
                        ),
                    });
                }
            }

            // Check 4: Found on PATH, none of the above -> Satisfied
            return Some(RequirementStatus::Satisfied);
        }

        // Check 5: Nothing found -> None (let caller handle as Missing)
        None
    }

    fn run_command_succeeds(&self, cmd: &str) -> bool {
        let full_path_str = self
            .probe
            .full_path()
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(":");

        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("PATH", &full_path_str)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn check_binary_present(&self, service_cmd: &str) -> bool {
        // Extract the first word of the command as the binary name
        let binary = service_cmd.split_whitespace().next().unwrap_or("");
        if binary.is_empty() {
            return false;
        }
        let full_path = self.probe.full_path();
        crate::requirements::probe::resolve_tool_path(binary, full_path.as_slice()).is_some()
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
        ResolvedStep::from_config("test", &config)
    }

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

        // First check
        let status1 = checker.check_one("nonexistent-tool-xyz");
        assert!(matches!(status1, RequirementStatus::Unknown));

        // Second check should use cache (same result)
        let status2 = checker.check_one("nonexistent-tool-xyz");
        assert!(matches!(status2, RequirementStatus::Unknown));

        // Verify it's actually cached
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

        // postgres-server should be skipped (provided)
        // ruby should be checked (and will likely be Missing or Satisfied depending on system)
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

        // Should check nonexistent-tool-xyz and return Unknown
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

        // Manually cache a satisfied result
        checker
            .cache
            .insert("fake-tool".to_string(), RequirementStatus::Satisfied);

        let step = make_resolved_step(vec!["fake-tool".to_string()]);
        let gaps = checker.check_step(&step, None);

        // Satisfied requirements should not appear in gaps
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
}
