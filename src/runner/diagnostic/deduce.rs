//! Stage 6: Deduce resolutions from diagnosis + context.
//!
//! Generates heuristic resolution candidates keyed on category + context
//! signals. These supplement whatever Stage 5 found. When Stage 5 found
//! high-quality resolutions, the deduced ones rank lower. When Stage 5
//! found nothing, the deduced ones are all we have.

use super::classify::ErrorCategory;
use super::contextualize;
use super::{
    CategoryMatch, DiagnosticDetails, Platform, ResolutionCandidate, ResolutionSource, StepContext,
    WorkflowState,
};

/// Generate deduced resolution candidates from diagnosis + context.
pub fn deduce_resolutions(
    categories: &[CategoryMatch],
    details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) -> Vec<ResolutionCandidate> {
    let mut resolutions = Vec::new();

    for cat in categories {
        match cat.category {
            ErrorCategory::NotFound => {
                deduce_not_found(&mut resolutions, details, step_ctx, workflow_state);
            }
            ErrorCategory::ConnectionRefused => {
                deduce_connection_refused(&mut resolutions, details, step_ctx, workflow_state);
            }
            ErrorCategory::VersionMismatch => {
                deduce_version_mismatch(&mut resolutions, details);
            }
            ErrorCategory::SyncIssue => {
                deduce_sync_issue(&mut resolutions, step_ctx);
            }
            ErrorCategory::PermissionDenied => {
                deduce_permission_denied(&mut resolutions, details, step_ctx);
            }
            ErrorCategory::PortConflict => {
                deduce_port_conflict(&mut resolutions, details, step_ctx);
            }
            ErrorCategory::BuildFailure => {
                // Build failure resolutions are usually best extracted from
                // tool output (Stage 5). Only add generic advice here.
                if let Some(target) = &details.target {
                    resolutions.push(ResolutionCandidate {
                        label: format!("rebuild {}", target),
                        command: None,
                        explanation: format!(
                            "Build of '{}' failed — check build dependencies",
                            target
                        ),
                        confidence: 0.3,
                        source: ResolutionSource::Deduced,
                        platform: None,
                    });
                }
            }
            ErrorCategory::SystemConstraint => {
                deduce_system_constraint(&mut resolutions, step_ctx);
            }
            ErrorCategory::AuthFailure => {
                deduce_auth_failure(&mut resolutions, step_ctx);
            }
            ErrorCategory::ResourceLimit => {
                resolutions.push(ResolutionCandidate {
                    label: "free up disk space or increase resource limits".to_string(),
                    command: None,
                    explanation: "A resource limit was exceeded".to_string(),
                    confidence: 0.3,
                    source: ResolutionSource::Deduced,
                    platform: None,
                });
            }
        }
    }

    resolutions
}

fn deduce_not_found(
    resolutions: &mut Vec<ResolutionCandidate>,
    details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) {
    let target = details.target.as_deref().unwrap_or("unknown");

    // Check if install step for this ecosystem already succeeded — the issue
    // is likely a wrong package name or missing entry in the dependency manifest.
    if contextualize::has_install_step_succeeded(step_ctx, workflow_state) && target != "unknown" {
        resolutions.push(ResolutionCandidate {
            label: format!("check that '{}' is in your dependency manifest", target),
            command: None,
            explanation: format!(
                "An install step for this ecosystem already succeeded, \
                 but '{}' was still not found — it may not be listed in your dependencies",
                target
            ),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
        return;
    }

    // Check if install step was skipped
    if let Some(skipped_step) = contextualize::was_install_step_skipped(step_ctx, workflow_state) {
        resolutions.push(ResolutionCandidate {
            label: format!("re-run the '{}' step", skipped_step),
            command: None,
            explanation: format!(
                "The install step '{}' was skipped — re-running it may resolve this",
                skipped_step
            ),
            confidence: 0.6,
            source: ResolutionSource::Deduced,
            platform: None,
        });
        return;
    }

    // Check if no install step exists
    if !contextualize::has_install_step(step_ctx, workflow_state)
        && !workflow_state.steps.is_empty()
    {
        resolutions.push(ResolutionCandidate {
            label: "add an install step to your config".to_string(),
            command: None,
            explanation: "No install step found for this ecosystem in your workflow".to_string(),
            confidence: 0.4,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }

    // Database not found
    if step_ctx.command.contains("db:")
        || step_ctx.command.contains("migrate")
        || step_ctx.command.contains("createdb")
    {
        resolutions.push(ResolutionCandidate {
            label: format!("createdb {}", target),
            command: Some(format!("createdb {}", target)),
            explanation: format!("Database '{}' does not exist", target),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
        return;
    }

    // Generic install suggestion
    if target != "unknown" {
        // If this looks like a command or package name
        if !target.contains('.') && !target.contains('/') && target.len() < 30 {
            resolutions.push(ResolutionCandidate {
                label: format!("install {}", target),
                command: None,
                explanation: format!("'{}' was not found", target),
                confidence: 0.4,
                source: ResolutionSource::Deduced,
                platform: None,
            });
        }
    }
}

fn deduce_connection_refused(
    resolutions: &mut Vec<ResolutionCandidate>,
    details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) {
    let service = details.service.as_deref().or_else(|| {
        step_ctx
            .requires
            .iter()
            .find(|r| {
                r.contains("postgres")
                    || r.contains("redis")
                    || r.contains("mysql")
                    || r.contains("mongo")
            })
            .map(|s| s.as_str())
    });

    // Check if service-start step already succeeded → service may have crashed
    if contextualize::has_service_step_succeeded(step_ctx, workflow_state) {
        let service_name = service.unwrap_or("service");
        resolutions.push(ResolutionCandidate {
            label: format!("check {} logs — it may have crashed", service_name),
            command: None,
            explanation: "The service started but may have crashed or be on a different port"
                .to_string(),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }

    if let Some(svc) = service {
        let svc_short = extract_service_short_name(svc);
        // Platform-specific start commands
        resolutions.push(ResolutionCandidate {
            label: format!("brew services start {}", svc_short),
            command: Some(format!("brew services start {}", svc_short)),
            explanation: format!("{} is not running", svc),
            confidence: 0.6,
            source: ResolutionSource::Deduced,
            platform: Some(Platform::MacOS),
        });
        resolutions.push(ResolutionCandidate {
            label: format!("systemctl start {}", svc_short),
            command: Some(format!("systemctl start {}", svc_short)),
            explanation: format!("{} is not running", svc),
            confidence: 0.6,
            source: ResolutionSource::Deduced,
            platform: Some(Platform::Linux),
        });
    } else {
        resolutions.push(ResolutionCandidate {
            label: "start the required service".to_string(),
            command: None,
            explanation: "A service connection was refused".to_string(),
            confidence: 0.4,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

fn deduce_version_mismatch(
    resolutions: &mut Vec<ResolutionCandidate>,
    details: &DiagnosticDetails,
) {
    if let (Some(have), Some(need)) = (&details.version_have, &details.version_need) {
        // Extract major version from need for brew formula
        let major = need.split('.').next().unwrap_or(need);

        resolutions.push(ResolutionCandidate {
            label: format!("add postgresql@{}/bin to PATH", major),
            command: Some(format!(
                "export PATH=\"$(brew --prefix postgresql@{})/bin:$PATH\"",
                major
            )),
            explanation: format!(
                "pg_dump version {} does not match server version {} — update PATH",
                have, need
            ),
            confidence: 0.6,
            source: ResolutionSource::Deduced,
            platform: Some(Platform::MacOS),
        });

        resolutions.push(ResolutionCandidate {
            label: format!("brew install postgresql@{}", major),
            command: Some(format!("brew install postgresql@{}", major)),
            explanation: format!(
                "Install PostgreSQL {} client tools to match server version",
                major
            ),
            confidence: 0.55,
            source: ResolutionSource::Deduced,
            platform: Some(Platform::MacOS),
        });

        resolutions.push(ResolutionCandidate {
            label: "update PATH to point to the correct version".to_string(),
            command: None,
            explanation: format!(
                "Tool version {} does not match required version {}",
                have, need
            ),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    } else {
        resolutions.push(ResolutionCandidate {
            label: "update to the required version".to_string(),
            command: None,
            explanation: "A version mismatch was detected".to_string(),
            confidence: 0.4,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

fn deduce_sync_issue(resolutions: &mut Vec<ResolutionCandidate>, step_ctx: &StepContext<'_>) {
    let cmd = step_ctx.command;
    if cmd.contains("bundle") {
        resolutions.push(ResolutionCandidate {
            label: "bundle lock".to_string(),
            command: Some("bundle lock".to_string()),
            explanation: "Gemfile.lock may be out of sync with Gemfile".to_string(),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    } else if cmd.contains("poetry") {
        resolutions.push(ResolutionCandidate {
            label: "poetry lock".to_string(),
            command: Some("poetry lock".to_string()),
            explanation: "poetry.lock may be out of sync with pyproject.toml".to_string(),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    } else if cmd.contains("npm") || cmd.contains("yarn") {
        let lock_cmd = if cmd.contains("yarn") {
            "yarn install"
        } else {
            "npm install"
        };
        resolutions.push(ResolutionCandidate {
            label: lock_cmd.to_string(),
            command: Some(lock_cmd.to_string()),
            explanation: "Lock file may be out of sync with package.json".to_string(),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    } else {
        resolutions.push(ResolutionCandidate {
            label: "re-run the lock/sync command".to_string(),
            command: None,
            explanation: "Lock file or dependencies may be out of sync".to_string(),
            confidence: 0.4,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

fn deduce_permission_denied(
    resolutions: &mut Vec<ResolutionCandidate>,
    details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
) {
    if let Some(target) = &details.target {
        // If target looks like a script file
        if target.starts_with("./")
            || target.ends_with(".sh")
            || target.contains("gradlew")
            || target.contains("mvnw")
        {
            resolutions.push(ResolutionCandidate {
                label: format!("chmod +x {}", target),
                command: Some(format!("chmod +x {}", target)),
                explanation: format!("'{}' is not executable", target),
                confidence: 0.7,
                source: ResolutionSource::Deduced,
                platform: None,
            });
            return;
        }

        // If target looks like a system path
        if target.starts_with('/') {
            resolutions.push(ResolutionCandidate {
                label: "check file ownership and permissions".to_string(),
                command: None,
                explanation: format!("Permission denied for '{}'", target),
                confidence: 0.3,
                source: ResolutionSource::Deduced,
                platform: None,
            });
            return;
        }
    }

    // Also check the command itself for script patterns
    let cmd = step_ctx.command;
    if cmd.starts_with("./") || cmd.contains("gradlew") || cmd.contains("mvnw") {
        let script = cmd.split_whitespace().next().unwrap_or(cmd);
        resolutions.push(ResolutionCandidate {
            label: format!("chmod +x {}", script),
            command: Some(format!("chmod +x {}", script)),
            explanation: format!("'{}' may not be executable", script),
            confidence: 0.6,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    } else {
        resolutions.push(ResolutionCandidate {
            label: "check file permissions".to_string(),
            command: None,
            explanation: "Permission denied".to_string(),
            confidence: 0.3,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

fn deduce_port_conflict(
    resolutions: &mut Vec<ResolutionCandidate>,
    details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
) {
    if let Some(port) = details.port {
        resolutions.push(ResolutionCandidate {
            label: format!("find what's using port {}", port),
            command: Some(format!("lsof -i :{}", port)),
            explanation: format!("Port {} is already in use", port),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }

    if step_ctx.command.contains("docker") {
        resolutions.push(ResolutionCandidate {
            label: "docker compose down".to_string(),
            command: Some("docker compose down".to_string()),
            explanation: "Stop running containers that may be using the port".to_string(),
            confidence: 0.4,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

fn deduce_system_constraint(
    resolutions: &mut Vec<ResolutionCandidate>,
    step_ctx: &StepContext<'_>,
) {
    if step_ctx.command.contains("pip") || step_ctx.command.contains("python") {
        resolutions.push(ResolutionCandidate {
            label: "create a virtual environment".to_string(),
            command: Some("python3 -m venv .venv && source .venv/bin/activate".to_string()),
            explanation: "System Python is externally managed — use a virtual environment"
                .to_string(),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

fn deduce_auth_failure(resolutions: &mut Vec<ResolutionCandidate>, step_ctx: &StepContext<'_>) {
    if step_ctx.command.contains("git") || step_ctx.command.contains("ssh") {
        resolutions.push(ResolutionCandidate {
            label: "check SSH keys".to_string(),
            command: Some("ssh -T git@github.com".to_string()),
            explanation: "SSH authentication failed — verify your SSH key is registered"
                .to_string(),
            confidence: 0.3,
            source: ResolutionSource::Deduced,
            platform: None,
        });
    }
}

/// Extract a short service name from a requirement string.
fn extract_service_short_name(service: &str) -> &str {
    if service.contains("postgres") {
        "postgresql"
    } else if service.contains("redis") {
        "redis"
    } else if service.contains("mysql") {
        "mysql"
    } else if service.contains("mongo") {
        "mongodb-community"
    } else if service.contains("elasticsearch") {
        "elasticsearch"
    } else if service.contains("rabbitmq") {
        "rabbitmq"
    } else if service.contains("memcached") {
        "memcached"
    } else {
        service
    }
}

/// Merge deduced resolutions into extracted resolutions, deduplicating.
///
/// When a deduced resolution has the same command as an existing one, keep
/// the one with higher confidence and drop the other.
pub fn merge_resolutions(
    existing: &mut Vec<ResolutionCandidate>,
    deduced: Vec<ResolutionCandidate>,
) {
    for candidate in deduced {
        // Filter by platform first
        if let Some(platform) = candidate.platform {
            if !matches_current_platform(platform) {
                continue;
            }
        }

        // Check for duplicates: same command (if both have commands)
        let dup_idx = candidate.command.as_ref().and_then(|cmd| {
            existing.iter().position(|e| {
                e.command
                    .as_ref()
                    .is_some_and(|ec| commands_equivalent(ec, cmd))
            })
        });

        match dup_idx {
            Some(idx) => {
                // Keep the one with higher confidence
                if candidate.confidence > existing[idx].confidence {
                    existing[idx] = candidate;
                }
            }
            None => {
                existing.push(candidate);
            }
        }
    }
}

/// Check if two commands are semantically equivalent.
fn commands_equivalent(a: &str, b: &str) -> bool {
    // Exact match
    if a == b {
        return true;
    }
    // Normalize whitespace and compare
    let a_normalized: Vec<&str> = a.split_whitespace().collect();
    let b_normalized: Vec<&str> = b.split_whitespace().collect();
    a_normalized == b_normalized
}

fn matches_current_platform(platform: Platform) -> bool {
    match platform {
        Platform::Any => true,
        Platform::MacOS => cfg!(target_os = "macos"),
        Platform::Linux => cfg!(target_os = "linux"),
        Platform::Windows => cfg!(target_os = "windows"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_ctx<'a>(command: &'a str) -> StepContext<'a> {
        StepContext {
            name: "test",
            command,
            requires: &[],
            template: None,
        }
    }

    #[test]
    fn permission_denied_script_suggests_chmod() {
        let details = DiagnosticDetails {
            target: Some("./gradlew".to_string()),
            ..Default::default()
        };
        let cats = vec![CategoryMatch {
            category: ErrorCategory::PermissionDenied,
            confidence: 0.3,
        }];
        let ctx = make_ctx("./gradlew build");
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let res = deduce_resolutions(&cats, &details, &ctx, &ws);
        assert!(res
            .iter()
            .any(|r| r.command.as_deref() == Some("chmod +x ./gradlew")));
        let chmod_res = res
            .iter()
            .find(|r| r.command.as_deref() == Some("chmod +x ./gradlew"))
            .unwrap();
        assert_eq!(chmod_res.confidence, 0.7);
    }

    #[test]
    fn connection_refused_with_service() {
        let details = DiagnosticDetails {
            service: Some("postgres-server".to_string()),
            ..Default::default()
        };
        let cats = vec![CategoryMatch {
            category: ErrorCategory::ConnectionRefused,
            confidence: 0.3,
        }];
        let requires = vec!["postgres-server".to_string()];
        let ctx = StepContext {
            name: "db",
            command: "rails db:create",
            requires: &requires,
            template: None,
        };
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let res = deduce_resolutions(&cats, &details, &ctx, &ws);
        let has_start = res.iter().any(|r| {
            r.command
                .as_deref()
                .map(|c| c.contains("postgresql"))
                .unwrap_or(false)
        });
        assert!(has_start);
    }

    #[test]
    fn version_mismatch_with_versions() {
        let details = DiagnosticDetails {
            version_have: Some("14.21".to_string()),
            version_need: Some("16.13".to_string()),
            ..Default::default()
        };
        let cats = vec![CategoryMatch {
            category: ErrorCategory::VersionMismatch,
            confidence: 0.5,
        }];
        let ctx = make_ctx("rails db:prepare");
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let res = deduce_resolutions(&cats, &details, &ctx, &ws);
        // Should have both "update PATH" and "brew install" resolutions
        assert!(res.len() >= 2);
        assert!(res.iter().any(|r| r.label.contains("PATH")));
    }

    #[test]
    fn port_conflict_suggests_lsof() {
        let details = DiagnosticDetails {
            port: Some(3000),
            ..Default::default()
        };
        let cats = vec![CategoryMatch {
            category: ErrorCategory::PortConflict,
            confidence: 0.3,
        }];
        let ctx = make_ctx("npm start");
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let res = deduce_resolutions(&cats, &details, &ctx, &ws);
        assert!(res
            .iter()
            .any(|r| r.command.as_deref() == Some("lsof -i :3000")));
    }

    #[test]
    fn system_constraint_suggests_venv() {
        let details = DiagnosticDetails::default();
        let cats = vec![CategoryMatch {
            category: ErrorCategory::SystemConstraint,
            confidence: 0.5,
        }];
        let ctx = make_ctx("pip install flask");
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let res = deduce_resolutions(&cats, &details, &ctx, &ws);
        assert!(res.iter().any(|r| r
            .command
            .as_ref()
            .map(|c| c.contains("venv"))
            .unwrap_or(false)));
    }

    #[test]
    fn merge_deduplicates_same_command() {
        let mut existing = vec![ResolutionCandidate {
            label: "fix".to_string(),
            command: Some("chmod +x ./gradlew".to_string()),
            explanation: "test".to_string(),
            confidence: 0.5,
            source: ResolutionSource::Extracted,
            platform: None,
        }];
        let deduced = vec![ResolutionCandidate {
            label: "chmod +x ./gradlew".to_string(),
            command: Some("chmod +x ./gradlew".to_string()),
            explanation: "test".to_string(),
            confidence: 0.7,
            source: ResolutionSource::Deduced,
            platform: None,
        }];

        merge_resolutions(&mut existing, deduced);
        assert_eq!(existing.len(), 1); // No duplicate added
    }

    #[test]
    fn merge_keeps_complementary() {
        let mut existing = vec![ResolutionCandidate {
            label: "find port".to_string(),
            command: Some("lsof -i :3000".to_string()),
            explanation: "diagnostic".to_string(),
            confidence: 0.5,
            source: ResolutionSource::Deduced,
            platform: None,
        }];
        let deduced = vec![ResolutionCandidate {
            label: "docker compose down".to_string(),
            command: Some("docker compose down".to_string()),
            explanation: "curative".to_string(),
            confidence: 0.4,
            source: ResolutionSource::Deduced,
            platform: None,
        }];

        merge_resolutions(&mut existing, deduced);
        assert_eq!(existing.len(), 2); // Both kept
    }

    #[test]
    fn sync_issue_poetry() {
        let details = DiagnosticDetails::default();
        let cats = vec![CategoryMatch {
            category: ErrorCategory::SyncIssue,
            confidence: 0.3,
        }];
        let ctx = make_ctx("poetry install");
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let res = deduce_resolutions(&cats, &details, &ctx, &ws);
        assert!(res
            .iter()
            .any(|r| r.command.as_deref() == Some("poetry lock")));
    }
}
