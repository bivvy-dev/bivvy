//! Requirement installation during workflow execution.
//!
//! Handles interactive remediation of requirement gaps found during
//! a workflow run: PATH activation for inactive tools, starting
//! services, and offering to install missing requirements.

use crate::error::{BivvyError, Result};
use crate::requirements::checker::GapChecker;
use crate::requirements::status::{GapResult, RequirementStatus};
use crate::ui::{Prompt, PromptType, UserInterface};
use std::path::Path;

/// Mockable dependencies for the installer.
pub struct InstallerContext<'a> {
    /// Run a shell command, returning true on success.
    pub run_command: &'a dyn Fn(&str) -> bool,
    /// Check whether the network is reachable.
    pub check_network: &'a dyn Fn() -> bool,
    /// Prepend a directory to the process PATH.
    pub prepend_path: &'a dyn Fn(&Path),
}

/// Handle all requirement gaps for a step.
///
/// Returns `Ok(true)` if the step can proceed, `Ok(false)` if
/// it should be skipped, or `Err` with `RequirementMissing`.
pub fn handle_gaps(
    gaps: &[GapResult],
    checker: &mut GapChecker<'_>,
    ui: &mut dyn UserInterface,
    interactive: bool,
    ctx: &InstallerContext<'_>,
) -> Result<bool> {
    if gaps.is_empty() {
        return Ok(true);
    }

    let mut blocking = Vec::new();

    for gap in gaps {
        let outcome = if interactive {
            handle_gap_interactive(gap, checker, ui, ctx)
        } else {
            handle_gap_non_interactive(gap, ui)
        };

        match outcome {
            Outcome::Resolved | Outcome::CanProceed => {}
            Outcome::Skip { reason } => {
                ui.warning(&reason);
                return Ok(false);
            }
            Outcome::Blocked { requirement } => {
                blocking.push(requirement);
            }
        }
    }

    if blocking.is_empty() {
        Ok(true)
    } else {
        let names: Vec<&str> = blocking.iter().map(|s| s.as_str()).collect();
        Err(BivvyError::RequirementMissing {
            requirement: names.join(", "),
            message: format!(
                "Missing requirements: {}. Run 'bivvy lint' for details.",
                names.join(", ")
            ),
        })
    }
}

/// Build the default `InstallerContext` for production use.
pub fn default_context() -> InstallerContext<'static> {
    InstallerContext {
        run_command: &|cmd| {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .status()
                .is_ok_and(|s| s.success())
        },
        check_network: &|| {
            use std::net::TcpStream;
            use std::time::Duration;
            let timeout = Duration::from_secs(2);
            let addr: std::net::SocketAddr = "1.1.1.1:443".parse().unwrap();
            TcpStream::connect_timeout(&addr, timeout).is_ok()
        },
        prepend_path: &|dir| {
            let current = std::env::var("PATH").unwrap_or_default();
            let new_path = format!("{}:{}", dir.display(), current);
            // SAFETY: single-threaded during gap handling
            unsafe { std::env::set_var("PATH", new_path) };
        },
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Resolved,
    CanProceed,
    Skip { reason: String },
    Blocked { requirement: String },
}

fn handle_gap_interactive(
    gap: &GapResult,
    checker: &mut GapChecker<'_>,
    ui: &mut dyn UserInterface,
    ctx: &InstallerContext<'_>,
) -> Outcome {
    match &gap.status {
        RequirementStatus::Satisfied => Outcome::Resolved,
        RequirementStatus::Inactive {
            binary_path,
            activation_hint,
            ..
        } => handle_inactive(&gap.requirement, binary_path, activation_hint, checker, ctx),
        RequirementStatus::ServiceDown {
            start_command,
            start_hint,
            ..
        } => handle_service_down(
            &gap.requirement,
            start_command.as_deref(),
            start_hint,
            checker,
            ui,
            ctx,
        ),
        RequirementStatus::Missing {
            install_template,
            install_hint,
        } => handle_missing(
            &gap.requirement,
            install_template.as_deref(),
            install_hint.as_deref(),
            checker,
            ui,
            ctx,
        ),
        RequirementStatus::SystemOnly {
            warning,
            install_template,
            ..
        } => handle_system_only(
            &gap.requirement,
            warning,
            install_template.as_deref(),
            checker,
            ui,
            ctx,
        ),
        RequirementStatus::Unknown => Outcome::Blocked {
            requirement: gap.requirement.clone(),
        },
    }
}

fn handle_gap_non_interactive(gap: &GapResult, ui: &mut dyn UserInterface) -> Outcome {
    match &gap.status {
        RequirementStatus::Satisfied => Outcome::Resolved,
        RequirementStatus::SystemOnly { warning, .. } => {
            ui.warning(warning);
            Outcome::CanProceed
        }
        RequirementStatus::Inactive {
            manager,
            activation_hint,
            ..
        } => {
            ui.warning(&format!(
                "'{}' found via {} but not activated. {}",
                gap.requirement, manager, activation_hint
            ));
            Outcome::Blocked {
                requirement: gap.requirement.clone(),
            }
        }
        RequirementStatus::ServiceDown { .. }
        | RequirementStatus::Missing { .. }
        | RequirementStatus::Unknown => Outcome::Blocked {
            requirement: gap.requirement.clone(),
        },
    }
}

// 6A: Inactive — PATH activation
fn handle_inactive(
    requirement: &str,
    binary_path: &Path,
    activation_hint: &str,
    checker: &mut GapChecker<'_>,
    ctx: &InstallerContext<'_>,
) -> Outcome {
    if let Some(parent) = binary_path.parent() {
        (ctx.prepend_path)(parent);
        checker.invalidate(requirement);
        Outcome::Resolved
    } else {
        Outcome::Skip {
            reason: format!("'{}' activation failed. {}", requirement, activation_hint),
        }
    }
}

// 6B: ServiceDown — offer to start
fn handle_service_down(
    requirement: &str,
    start_command: Option<&str>,
    start_hint: &str,
    checker: &mut GapChecker<'_>,
    ui: &mut dyn UserInterface,
    ctx: &InstallerContext<'_>,
) -> Outcome {
    let Some(cmd) = start_command else {
        return Outcome::Skip {
            reason: format!("Service '{}' is not running. {}", requirement, start_hint),
        };
    };

    let prompt = Prompt {
        key: format!("start_{}", requirement),
        question: format!("Start {}? ({})", requirement, cmd),
        prompt_type: PromptType::Confirm,
        default: Some("yes".to_string()),
    };

    let confirmed = ui
        .prompt(&prompt)
        .ok()
        .and_then(|r| r.as_bool())
        .unwrap_or(false);

    if !confirmed {
        return Outcome::Skip {
            reason: format!("Service '{}' is not running. {}", requirement, start_hint),
        };
    }

    if (ctx.run_command)(cmd) {
        checker.invalidate(requirement);
        Outcome::Resolved
    } else {
        Outcome::Skip {
            reason: format!("Failed to start '{}'. {}", requirement, start_hint),
        }
    }
}

// 6C: Missing — install offer
fn handle_missing(
    requirement: &str,
    install_template: Option<&str>,
    install_hint: Option<&str>,
    checker: &mut GapChecker<'_>,
    ui: &mut dyn UserInterface,
    ctx: &InstallerContext<'_>,
) -> Outcome {
    let hint = install_hint.unwrap_or("Install manually");

    if install_template.is_none() {
        return Outcome::Skip {
            reason: format!("Missing requirement '{}'. {}", requirement, hint),
        };
    }

    if !(ctx.check_network)() {
        return Outcome::Skip {
            reason: format!(
                "Installation of '{}' requires network access, which isn't available.",
                requirement
            ),
        };
    }

    match checker.resolve_install_deps(requirement) {
        Ok(deps) => {
            for dep in &deps {
                if dep == requirement {
                    continue;
                }
                let dep_status = checker.check_one(dep);
                if dep_status.is_satisfied() || dep_status.can_proceed() {
                    continue;
                }
                let dep_prompt = Prompt {
                    key: format!("install_{}", dep),
                    question: format!("Install dependency '{}'?", dep),
                    prompt_type: PromptType::Confirm,
                    default: Some("yes".to_string()),
                };
                let confirmed = ui
                    .prompt(&dep_prompt)
                    .ok()
                    .and_then(|r| r.as_bool())
                    .unwrap_or(false);
                if !confirmed {
                    return Outcome::Skip {
                        reason: format!(
                            "Dependency '{}' for '{}' not installed.",
                            dep, requirement
                        ),
                    };
                }
                checker.invalidate(dep);
            }
        }
        Err(e) => {
            ui.warning(&format!(
                "Could not resolve dependencies for '{}': {}",
                requirement, e
            ));
        }
    }

    let prompt = Prompt {
        key: format!("install_{}", requirement),
        question: format!("Install {}?", requirement),
        prompt_type: PromptType::Confirm,
        default: Some("yes".to_string()),
    };

    let confirmed = ui
        .prompt(&prompt)
        .ok()
        .and_then(|r| r.as_bool())
        .unwrap_or(false);

    if !confirmed {
        return Outcome::Skip {
            reason: format!("Missing requirement '{}'. {}", requirement, hint),
        };
    }

    let install_success = if let Some(hint_cmd) = install_hint {
        (ctx.run_command)(hint_cmd)
    } else {
        false
    };

    if !install_success {
        return Outcome::Skip {
            reason: format!(
                "Installation of '{}' failed. Check output above.",
                requirement
            ),
        };
    }

    // Install exited 0 — invalidate cache and verify the tool is now on PATH
    checker.invalidate(requirement);
    let status = checker.check_one(requirement);

    if status.is_satisfied() {
        Outcome::Resolved
    } else {
        Outcome::Skip {
            reason: format!(
                "Installed '{}' (exit 0) but not found on PATH. May need shell restart.",
                requirement
            ),
        }
    }
}

// 6D: SystemOnly — warn + optional managed install
fn handle_system_only(
    requirement: &str,
    warning: &str,
    install_template: Option<&str>,
    checker: &mut GapChecker<'_>,
    ui: &mut dyn UserInterface,
    ctx: &InstallerContext<'_>,
) -> Outcome {
    ui.warning(warning);

    if install_template.is_none() {
        return Outcome::CanProceed;
    }

    let prompt = Prompt {
        key: format!("managed_install_{}", requirement),
        question: format!("Install a managed version of {}?", requirement),
        prompt_type: PromptType::Confirm,
        default: Some("no".to_string()),
    };

    let confirmed = ui
        .prompt(&prompt)
        .ok()
        .and_then(|r| r.as_bool())
        .unwrap_or(false);

    if !confirmed {
        return Outcome::CanProceed;
    }

    let result = handle_missing(requirement, install_template, None, checker, ui, ctx);
    match result {
        Outcome::Resolved => Outcome::Resolved,
        _ => Outcome::CanProceed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requirements::probe::EnvironmentProbe;
    use crate::requirements::registry::RequirementRegistry;
    use crate::requirements::status::RequirementStatus;
    use crate::ui::MockUI;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_probe() -> EnvironmentProbe {
        EnvironmentProbe::run_with_env(|_| Err(std::env::VarError::NotPresent))
    }

    fn stub_ctx(command_succeeds: bool, network_ok: bool) -> InstallerContext<'static> {
        let run_cmd: &'static dyn Fn(&str) -> bool = if command_succeeds {
            &|_| true
        } else {
            &|_| false
        };
        let net: &'static dyn Fn() -> bool = if network_ok { &|| true } else { &|| false };
        InstallerContext {
            run_command: run_cmd,
            check_network: net,
            prepend_path: &|_| {},
        }
    }

    fn make_checker<'a>(
        registry: &'a RequirementRegistry,
        probe: &'a EnvironmentProbe,
        temp: &TempDir,
    ) -> GapChecker<'a> {
        GapChecker::new(registry, probe, temp.path())
    }

    // --- 6A: Inactive tests ---

    #[test]
    fn inactive_prepends_path_and_resolves() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        let ctx = InstallerContext {
            run_command: &|_| true,
            check_network: &|| true,
            prepend_path: &|_| {},
        };

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Inactive {
                manager: "rbenv".to_string(),
                binary_path: PathBuf::from("/home/user/.rbenv/versions/3.2/bin/ruby"),
                activation_hint: "eval \"$(rbenv init -)\"".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
        assert!(!checker.cache.contains_key("ruby"));
    }

    #[test]
    fn inactive_with_no_parent_skips() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let ctx = stub_ctx(true, true);

        let outcome = handle_inactive("tool", Path::new(""), "activate it", &mut checker, &ctx);
        assert!(matches!(outcome, Outcome::Skip { .. }));
    }

    // --- 6B: ServiceDown tests ---

    #[test]
    fn service_down_offers_to_start() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("start_postgres", "yes");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "postgres".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("brew services start postgresql@16".to_string()),
                start_hint: "Start PostgreSQL manually".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
        assert!(ui.prompts_shown().contains(&"start_postgres".to_string()));
    }

    #[test]
    fn service_down_start_success_invalidates_cache() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("start_redis", "yes");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "redis".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("brew services start redis".to_string()),
                start_hint: "Start Redis manually".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
        assert!(!checker.cache.contains_key("redis"));
    }

    #[test]
    fn service_down_start_failure_skips() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("start_postgres", "yes");
        let ctx = stub_ctx(false, true);

        let gaps = vec![GapResult {
            requirement: "postgres".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("brew services start postgresql@16".to_string()),
                start_hint: "Start PostgreSQL manually".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
        assert!(ui.has_warning("Failed to start"));
    }

    #[test]
    fn service_down_no_start_command_skips() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "postgres".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: false,
                install_template: None,
                start_command: None,
                start_hint: "Install and start PostgreSQL".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
        assert!(ui.has_warning("not running"));
    }

    #[test]
    fn service_down_decline_skips() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("start_postgres", "no");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "postgres".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("brew services start postgresql@16".to_string()),
                start_hint: "Start PostgreSQL manually".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
    }

    // --- 6C: Missing tests ---

    #[test]
    fn missing_prompts_install() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("install_mise", "yes");
        ui.set_prompt_response("install_ruby", "yes");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via mise".to_string()),
            },
        }];

        let _ = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(ui.prompts_shown().contains(&"install_ruby".to_string()));
    }

    #[test]
    fn missing_skips_on_decline() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("install_mise", "yes");
        ui.set_prompt_response("install_ruby", "no");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via mise".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
        assert!(ui.has_warning("Missing requirement"));
    }

    #[test]
    fn missing_no_network_skips() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        let ctx = stub_ctx(true, false);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via mise".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
        assert!(ui.has_warning("network access"));
    }

    #[test]
    fn missing_install_failure() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("install_mise", "yes");
        ui.set_prompt_response("install_ruby", "yes");
        let ctx = stub_ctx(false, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via mise".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
        assert!(ui.has_warning("failed"));
    }

    #[test]
    fn missing_no_template_shows_hint() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "custom-tool".to_string(),
            status: RequirementStatus::Missing {
                install_template: None,
                install_hint: Some("Download from company intranet".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(!result.unwrap());
        assert!(ui.has_warning("Download from company intranet"));
    }

    // --- 6D: SystemOnly tests ---

    #[test]
    fn system_only_warns_and_proceeds() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::SystemOnly {
                path: PathBuf::from("/usr/bin/ruby"),
                install_template: None,
                warning: "System Ruby detected at /usr/bin/ruby".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
        assert!(ui.has_warning("System Ruby"));
    }

    #[test]
    fn system_only_offers_managed_install() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("managed_install_ruby", "no");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::SystemOnly {
                path: PathBuf::from("/usr/bin/ruby"),
                install_template: Some("mise-ruby".to_string()),
                warning: "System Ruby detected".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
        assert!(ui
            .prompts_shown()
            .contains(&"managed_install_ruby".to_string()));
    }

    // --- 6E: Cache invalidation tests ---

    #[test]
    fn cache_invalidated_after_service_start() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        checker.cache.insert(
            "redis".to_string(),
            RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("redis-server".to_string()),
                start_hint: "Start redis".to_string(),
            },
        );

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("start_redis", "yes");
        let ctx = stub_ctx(true, true);

        let gap = GapResult {
            requirement: "redis".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("redis-server".to_string()),
                start_hint: "Start redis".to_string(),
            },
        };

        let _ = handle_gap_interactive(&gap, &mut checker, &mut ui, &ctx);
        assert!(!checker.cache.contains_key("redis"));
    }

    #[test]
    fn cache_not_invalidated_for_unrelated_requirement() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        checker
            .cache
            .insert("ruby".to_string(), RequirementStatus::Satisfied);
        checker.cache.insert(
            "redis".to_string(),
            RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("redis-server".to_string()),
                start_hint: "Start redis".to_string(),
            },
        );

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("start_redis", "yes");
        let ctx = stub_ctx(true, true);

        let gap = GapResult {
            requirement: "redis".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("redis-server".to_string()),
                start_hint: "Start redis".to_string(),
            },
        };

        let _ = handle_gap_interactive(&gap, &mut checker, &mut ui, &ctx);
        assert!(!checker.cache.contains_key("redis"));
        assert!(checker.cache.contains_key("ruby"));
    }

    // --- 6F: Non-interactive tests ---

    #[test]
    fn non_interactive_system_only_warns_and_proceeds() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::SystemOnly {
                path: PathBuf::from("/usr/bin/ruby"),
                install_template: None,
                warning: "System Ruby".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, false, &ctx);
        assert!(result.unwrap());
        assert!(ui.has_warning("System Ruby"));
    }

    #[test]
    fn non_interactive_missing_blocks() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, false, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn non_interactive_service_down_blocks() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "postgres".to_string(),
            status: RequirementStatus::ServiceDown {
                binary_present: true,
                install_template: None,
                start_command: Some("pg_ctl start".to_string()),
                start_hint: "Start PostgreSQL".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, false, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn non_interactive_unknown_blocks() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "mystery-tool".to_string(),
            status: RequirementStatus::Unknown,
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, false, &ctx);
        assert!(result.is_err());
    }

    // --- 6C-extra: Install success but not on PATH ---

    #[test]
    fn missing_install_success_but_not_on_path() {
        // Scenario: A requirement is Missing, the user confirms install,
        // the install command exits 0 (success), but after invalidating
        // cache and re-checking, the tool is STILL not found on PATH.
        // Expected: Outcome::Skip with message containing "may need shell restart".
        //
        // We use a custom requirement registered via with_custom whose check
        // command always fails ("false"), so after install + re-check the
        // tool remains missing.
        use crate::config::{CustomRequirement, CustomRequirementCheck};
        use std::collections::HashMap;

        let mut custom = HashMap::new();
        custom.insert(
            "phantom-tool".to_string(),
            CustomRequirement {
                check: CustomRequirementCheck::CommandSucceeds {
                    command: "false".to_string(),
                },
                install_template: Some("phantom-install".to_string()),
                install_hint: Some("install-phantom-tool".to_string()),
            },
        );

        let registry = RequirementRegistry::new().with_custom(&custom);
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("install_phantom-tool", "yes");

        // run_command returns true (install "succeeds" exit 0),
        // but network is available and the check still fails after install
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "phantom-tool".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("phantom-install".to_string()),
                install_hint: Some("install-phantom-tool".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        // Should skip (not error) because install exited 0 but tool not found
        assert!(!result.unwrap(), "should return false (skip)");
        assert!(
            ui.has_warning("not found on PATH")
                || ui.has_warning("may need shell restart")
                || ui.has_warning("May need shell restart"),
            "should warn about PATH or shell restart, warnings: {:?}",
            ui.warnings()
        );
    }

    // --- 6F-extra: Non-interactive Inactive blocks ---

    #[test]
    fn non_interactive_inactive_blocks() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Inactive {
                manager: "rbenv".to_string(),
                binary_path: PathBuf::from("/home/user/.rbenv/versions/3.2/bin/ruby"),
                activation_hint: "eval \"$(rbenv init -)\"".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, false, &ctx);
        assert!(result.is_err(), "Inactive in non-interactive should block");
    }

    #[test]
    fn non_interactive_inactive_shows_warning() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Inactive {
                manager: "rbenv".to_string(),
                binary_path: PathBuf::from("/home/user/.rbenv/versions/3.2/bin/ruby"),
                activation_hint: "eval \"$(rbenv init -)\"".to_string(),
            },
        }];

        let _ = handle_gaps(&gaps, &mut checker, &mut ui, false, &ctx);
        assert!(
            ui.has_warning("not activated"),
            "should warn about activation, warnings: {:?}",
            ui.warnings()
        );
    }

    // --- 6C-extra: User declines install ---

    #[test]
    fn missing_install_declined_skips() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // User declines the install prompt
        ui.set_prompt_response("install_ruby", "no");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Missing {
                install_template: Some("mise-ruby".to_string()),
                install_hint: Some("Install Ruby via mise".to_string()),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(
            !result.unwrap(),
            "should skip when user declines install"
        );
        assert!(
            ui.has_warning("ruby") || ui.has_warning("Skipping"),
            "should warn about skipped install, warnings: {:?}",
            ui.warnings()
        );
    }

    // --- 6D-extra: SystemOnly user accepts managed install ---

    #[test]
    fn system_only_accepts_managed_install() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // User accepts managed install
        ui.set_prompt_response("managed_install_ruby", "yes");
        // handle_missing will be called, which prompts for deps and install
        ui.set_prompt_response("install_mise", "yes");
        ui.set_prompt_response("install_ruby", "yes");
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::SystemOnly {
                path: PathBuf::from("/usr/bin/ruby"),
                install_template: Some("mise-ruby".to_string()),
                warning: "System Ruby detected".to_string(),
            },
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        // Should proceed regardless (SystemOnly always can proceed)
        assert!(result.unwrap());
        assert!(ui
            .prompts_shown()
            .contains(&"managed_install_ruby".to_string()));
    }

    // --- Edge case tests ---

    #[test]
    fn empty_gaps_proceeds() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let result = handle_gaps(&[], &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
    }

    #[test]
    fn satisfied_gap_proceeds() {
        let registry = RequirementRegistry::new();
        let probe = make_probe();
        let temp = TempDir::new().unwrap();
        let mut checker = make_checker(&registry, &probe, &temp);
        let mut ui = MockUI::new();
        let ctx = stub_ctx(true, true);

        let gaps = vec![GapResult {
            requirement: "ruby".to_string(),
            status: RequirementStatus::Satisfied,
        }];

        let result = handle_gaps(&gaps, &mut checker, &mut ui, true, &ctx);
        assert!(result.unwrap());
    }
}
