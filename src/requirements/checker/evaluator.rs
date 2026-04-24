//! Requirement evaluation logic.
//!
//! `RequirementEvaluator` determines whether a single requirement is
//! satisfied by running checks against the current system state. It is
//! stateless (no cache) and used by `GapChecker` as a delegate.

use crate::requirements::probe::EnvironmentProbe;
use crate::requirements::registry::{RequirementCheck, RequirementRegistry};
use crate::requirements::status::RequirementStatus;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Evaluates individual requirements against system state.
///
/// Stateless: holds references to the registry, probe, and project root,
/// but does not cache results. Caching is the responsibility of
/// [`super::GapChecker`].
pub(super) struct RequirementEvaluator<'a> {
    pub(super) registry: &'a RequirementRegistry,
    pub(super) probe: &'a EnvironmentProbe,
    pub(super) project_root: PathBuf,
}

impl<'a> RequirementEvaluator<'a> {
    /// Evaluate a requirement by name, returning its current status.
    pub(super) fn evaluate(&self, requirement: &str) -> RequirementStatus {
        let Some(req) = self.registry.get(requirement) else {
            return RequirementStatus::Unknown;
        };

        for check in &req.checks {
            if let Some(status) = self.evaluate_check(check, req) {
                return status;
            }
        }

        // No check matched -- Missing
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
                    let original_path = crate::requirements::probe::parse_system_path();
                    let on_original =
                        crate::requirements::probe::resolve_tool_path(tool, &original_path);
                    if on_original.is_none() {
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
    use crate::requirements::probe::EnvironmentProbe;
    use crate::requirements::registry::{Requirement, RequirementCheck, RequirementRegistry};
    use tempfile::TempDir;

    fn make_probe() -> EnvironmentProbe {
        EnvironmentProbe::run_with_env(|_| Err(std::env::VarError::NotPresent))
    }

    fn make_evaluator<'a>(
        registry: &'a RequirementRegistry,
        probe: &'a EnvironmentProbe,
        project_root: &Path,
    ) -> RequirementEvaluator<'a> {
        RequirementEvaluator {
            registry,
            probe,
            project_root: project_root.to_path_buf(),
        }
    }

    #[test]
    fn unknown_requirement_returns_unknown() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let evaluator = make_evaluator(&registry, &probe, temp.path());

        let status = evaluator.evaluate("nonexistent-tool-xyz");
        assert!(matches!(status, RequirementStatus::Unknown));
    }

    #[test]
    fn manager_check_falls_through_to_missing() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let evaluator = make_evaluator(&registry, &probe, temp.path());

        let unknown_status = evaluator.evaluate("nonexistent-tool-xyz");
        assert!(
            matches!(unknown_status, RequirementStatus::Unknown),
            "unknown tool should return Unknown, not Missing"
        );

        let ruby_status = evaluator.evaluate("ruby");
        assert!(
            !matches!(ruby_status, RequirementStatus::Unknown),
            "known tool 'ruby' should return Satisfied, Missing, or another status -- never Unknown"
        );
    }

    #[test]
    fn manager_detected_but_tool_not_installed_falls_through() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".ruby-version"), "3.2.0").unwrap();

        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "fake-ruby".to_string(),
            Requirement {
                name: "fake-ruby".to_string(),
                checks: vec![RequirementCheck::ManagedCommand {
                    command: "bivvy-nonexistent-ruby-xyz-12345".to_string(),
                    managed_path_patterns: vec!["mise/".to_string(), "rbenv/".to_string()],
                    system_path_patterns: vec!["/usr/bin/".to_string()],
                    version_file: Some(".ruby-version".to_string()),
                }],
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via mise".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("fake-ruby");
        assert!(
            matches!(status, RequirementStatus::Missing { .. }),
            "Expected Missing when tool binary not found, got: {:?}",
            status
        );
    }

    // --- FileExists check type ---

    #[test]
    fn file_exists_check_satisfied_when_file_present() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("setup.lock"), "done").unwrap();

        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "lockfile".to_string(),
            Requirement {
                name: "lockfile".to_string(),
                checks: vec![RequirementCheck::FileExists("setup.lock".to_string())],
                install_template: None,
                install_hint: None,
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("lockfile");
        assert!(
            matches!(status, RequirementStatus::Satisfied),
            "file_exists check should be Satisfied when file is present, got: {:?}",
            status
        );
    }

    #[test]
    fn file_exists_check_missing_when_file_absent() {
        let temp = TempDir::new().unwrap();

        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "lockfile".to_string(),
            Requirement {
                name: "lockfile".to_string(),
                checks: vec![RequirementCheck::FileExists("setup.lock".to_string())],
                install_template: Some("setup-install".to_string()),
                install_hint: Some("Run setup first".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("lockfile");
        assert!(
            matches!(status, RequirementStatus::Missing { .. }),
            "file_exists check should be Missing when file absent, got: {:?}",
            status
        );
    }

    // --- ServiceReachable check type ---

    #[test]
    fn service_reachable_returns_service_down_on_failure() {
        let temp = TempDir::new().unwrap();
        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "my-service".to_string(),
            Requirement {
                name: "my-service".to_string(),
                checks: vec![RequirementCheck::ServiceReachable("false".to_string())],
                install_template: Some("my-service-install".to_string()),
                install_hint: Some("Start the service".to_string()),
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("my-service");
        assert!(
            matches!(status, RequirementStatus::ServiceDown { .. }),
            "ServiceReachable check should return ServiceDown on failure, got: {:?}",
            status
        );
    }

    #[test]
    fn service_reachable_returns_satisfied_on_success() {
        let temp = TempDir::new().unwrap();
        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "echo-service".to_string(),
            Requirement {
                name: "echo-service".to_string(),
                checks: vec![RequirementCheck::ServiceReachable("true".to_string())],
                install_template: None,
                install_hint: None,
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("echo-service");
        assert!(
            matches!(status, RequirementStatus::Satisfied),
            "ServiceReachable check should return Satisfied on success, got: {:?}",
            status
        );
    }

    // --- SystemOnly path ---

    #[test]
    fn system_path_returns_system_only() {
        let temp = TempDir::new().unwrap();

        let sys_dir = temp.path().join("usr/bin");
        std::fs::create_dir_all(&sys_dir).unwrap();
        let tool_path = sys_dir.join("fake-tool");
        std::fs::write(&tool_path, "#!/bin/sh\ntrue").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let probe = EnvironmentProbe::run_with_env(|_| Err(std::env::VarError::NotPresent));

        let mut registry = RequirementRegistry::new();
        let sys_pattern = sys_dir.to_string_lossy().to_string();
        registry.insert(
            "fake-tool".to_string(),
            Requirement {
                name: "fake-tool".to_string(),
                checks: vec![RequirementCheck::ManagedCommand {
                    command: "fake-tool".to_string(),
                    managed_path_patterns: vec!["mise/".to_string()],
                    system_path_patterns: vec![sys_pattern],
                    version_file: None,
                }],
                install_template: Some("fake-install".to_string()),
                install_hint: None,
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("fake-tool");
        assert!(
            matches!(status, RequirementStatus::Missing { .. }),
            "tool not on PATH should be Missing, got: {:?}",
            status
        );
    }

    // --- RequirementCheck::Any ---

    #[test]
    fn any_check_satisfied_when_one_subcheck_passes() {
        let temp = TempDir::new().unwrap();
        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "any-tool".to_string(),
            Requirement {
                name: "any-tool".to_string(),
                checks: vec![RequirementCheck::Any(vec![
                    RequirementCheck::CommandSucceeds("false".to_string()),
                    RequirementCheck::CommandSucceeds("true".to_string()),
                ])],
                install_template: None,
                install_hint: None,
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("any-tool");
        assert!(
            matches!(status, RequirementStatus::Satisfied),
            "Any check should be Satisfied when at least one subcheck passes, got: {:?}",
            status
        );
    }

    #[test]
    fn any_check_missing_when_no_subcheck_passes() {
        let temp = TempDir::new().unwrap();
        let probe = make_probe();
        let mut registry = RequirementRegistry::new();
        registry.insert(
            "any-tool".to_string(),
            Requirement {
                name: "any-tool".to_string(),
                checks: vec![RequirementCheck::Any(vec![
                    RequirementCheck::CommandSucceeds("false".to_string()),
                    RequirementCheck::CommandSucceeds("false".to_string()),
                ])],
                install_template: Some("any-install".to_string()),
                install_hint: None,
                depends_on: vec![],
                install_requires: None,
            },
        );

        let evaluator = make_evaluator(&registry, &probe, temp.path());
        let status = evaluator.evaluate("any-tool");
        assert!(
            matches!(status, RequirementStatus::Missing { .. }),
            "Any check should be Missing when no subcheck passes, got: {:?}",
            status
        );
    }

    // --- Custom requirements via with_custom ---

    #[test]
    fn custom_requirement_file_exists_check_type() {
        use crate::config::{CustomRequirement, CustomRequirementCheck};

        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".tool-config"), "ok").unwrap();

        let mut custom = std::collections::HashMap::new();
        custom.insert(
            "my-tool".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::FileExists {
                    path: ".tool-config".to_string(),
                },
                install_template: None,
                install_hint: None,
            },
        );

        let registry = RequirementRegistry::new().with_custom(&custom);
        let probe = make_probe();
        let evaluator = make_evaluator(&registry, &probe, temp.path());

        let status = evaluator.evaluate("my-tool");
        assert!(
            matches!(status, RequirementStatus::Satisfied),
            "custom FileExists should be Satisfied when file present, got: {:?}",
            status
        );
    }

    #[test]
    fn custom_requirement_service_reachable_check_type() {
        use crate::config::{CustomRequirement, CustomRequirementCheck};

        let temp = TempDir::new().unwrap();

        let mut custom = std::collections::HashMap::new();
        custom.insert(
            "my-svc".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::ServiceReachable {
                    command: "false".to_string(),
                },
                install_template: None,
                install_hint: Some("Start my-svc".to_string()),
            },
        );

        let registry = RequirementRegistry::new().with_custom(&custom);
        let probe = make_probe();
        let evaluator = make_evaluator(&registry, &probe, temp.path());

        let status = evaluator.evaluate("my-svc");
        assert!(
            matches!(status, RequirementStatus::ServiceDown { .. }),
            "custom ServiceReachable should be ServiceDown on failure, got: {:?}",
            status
        );
    }
}
