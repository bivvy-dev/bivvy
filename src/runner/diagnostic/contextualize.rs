//! Stage 4: Contextualize — refine categories using step and workflow context.
//!
//! Two context sources:
//! 1. **Step context** — command, requires, template (already available)
//! 2. **Workflow state** — prior step results, what succeeded/failed/was skipped

use crate::steps::StepStatus;

use super::classify::ErrorCategory;
use super::{CategoryMatch, DiagnosticDetails, StepContext, WorkflowState};

/// Refine category confidence and details using step + workflow context.
pub fn contextualize(
    categories: &mut [CategoryMatch],
    details: &mut DiagnosticDetails,
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) {
    for cat in categories.iter_mut() {
        // Step context reinforcement
        apply_step_context_boost(cat, details, step_ctx);

        // Workflow state heuristics
        apply_workflow_heuristics(cat, details, step_ctx, workflow_state);

        // Clamp confidence to [0.0, 1.0]
        cat.confidence = cat.confidence.clamp(0.0, 1.0);
    }

    // Re-sort by confidence after adjustments
    categories.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Extract service name from requires if we have a connection_refused
    if categories
        .iter()
        .any(|c| c.category == ErrorCategory::ConnectionRefused)
        && details.service.is_none()
    {
        for req in step_ctx.requires {
            if req.contains("postgres")
                || req.contains("redis")
                || req.contains("mysql")
                || req.contains("mongo")
                || req.contains("elasticsearch")
                || req.contains("rabbitmq")
                || req.contains("memcached")
            {
                details.service = Some(req.clone());
                break;
            }
        }
    }
}

fn apply_step_context_boost(
    cat: &mut CategoryMatch,
    details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
) {
    match cat.category {
        ErrorCategory::ConnectionRefused => {
            // Boost if step requires a service
            if step_ctx.requires.iter().any(|r| {
                r.contains("postgres")
                    || r.contains("redis")
                    || r.contains("mysql")
                    || r.contains("mongo")
                    || r.contains("server")
            }) {
                cat.confidence += 0.15;
            }
        }
        ErrorCategory::NotFound => {
            // Boost if step command is a known install/dependency command
            if is_install_command(step_ctx.command) {
                cat.confidence += 0.15;
            }
        }
        ErrorCategory::VersionMismatch => {
            // Boost when version details were extracted — having actual version
            // numbers is strong confirmation of the diagnosis.
            if details.version_have.is_some() && details.version_need.is_some() {
                cat.confidence += 0.15;
            }
        }
        ErrorCategory::SyncIssue => {
            // Boost if step command is a package manager
            if is_package_manager_command(step_ctx.command) {
                cat.confidence += 0.15;
            }
        }
        ErrorCategory::SystemConstraint => {
            // Boost if pip/python context
            if step_ctx.command.contains("pip") || step_ctx.command.contains("python") {
                cat.confidence += 0.15;
            }
        }
        ErrorCategory::AuthFailure => {
            // Boost if git/ssh context
            if step_ctx.command.contains("git") || step_ctx.command.contains("ssh") {
                cat.confidence += 0.15;
            }
        }
        _ => {}
    }
}

fn apply_workflow_heuristics(
    cat: &mut CategoryMatch,
    _details: &DiagnosticDetails,
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) {
    match cat.category {
        ErrorCategory::ConnectionRefused => {
            // If a service-start step already succeeded, the service may have crashed
            if has_service_step_succeeded(step_ctx, workflow_state) {
                cat.confidence += 0.1;
            }
        }
        ErrorCategory::NotFound => {
            // If an install step for same ecosystem already succeeded,
            // the issue is likely a wrong package name, not a missing install
            if has_install_step_succeeded(step_ctx, workflow_state) {
                cat.confidence += 0.1;
            }
        }
        _ => {}
    }
}

/// Check if a step that starts a service required by the current step
/// has already succeeded.
pub(super) fn has_service_step_succeeded(
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) -> bool {
    for (name, step) in workflow_state.steps {
        if *name == step_ctx.name {
            continue;
        }
        let is_service_step = step.execution.command.contains("start")
            || step.execution.command.split_whitespace().any(|w| w == "up");
        if is_service_step {
            if let Some(status) = workflow_state.outcomes.get(*name) {
                if *status == StepStatus::Completed {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if an install/dependency step for a similar ecosystem already succeeded.
pub(super) fn has_install_step_succeeded(
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) -> bool {
    let ecosystem = detect_ecosystem(step_ctx.command);
    if ecosystem.is_empty() {
        return false;
    }

    for (name, step) in workflow_state.steps {
        if *name == step_ctx.name {
            continue;
        }
        if is_install_command(&step.execution.command)
            && detect_ecosystem(&step.execution.command) == ecosystem
        {
            if let Some(status) = workflow_state.outcomes.get(*name) {
                if *status == StepStatus::Completed {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a step that installs dependencies for the same ecosystem was skipped.
pub(super) fn was_install_step_skipped(
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) -> Option<String> {
    let ecosystem = detect_ecosystem(step_ctx.command);
    if ecosystem.is_empty() {
        return None;
    }

    for (name, step) in workflow_state.steps {
        if *name == step_ctx.name {
            continue;
        }
        if is_install_command(&step.execution.command)
            && detect_ecosystem(&step.execution.command) == ecosystem
        {
            if let Some(status) = workflow_state.outcomes.get(*name) {
                if *status == StepStatus::Skipped {
                    return Some((*name).to_string());
                }
            }
        }
    }
    None
}

/// Check if any install step exists for the same ecosystem.
pub(super) fn has_install_step(
    step_ctx: &StepContext<'_>,
    workflow_state: &WorkflowState<'_>,
) -> bool {
    let ecosystem = detect_ecosystem(step_ctx.command);
    if ecosystem.is_empty() {
        return false;
    }

    workflow_state.steps.iter().any(|(name, step)| {
        *name != step_ctx.name
            && is_install_command(&step.execution.command)
            && detect_ecosystem(&step.execution.command) == ecosystem
    })
}

fn is_install_command(cmd: &str) -> bool {
    cmd.contains("install")
        || cmd.contains("bundle")
        || cmd.contains("pip")
        || cmd.contains("yarn")
        || cmd.contains("npm i")
        || cmd.contains("composer")
        || cmd.contains("cargo build")
        || cmd.contains("mix deps")
        || cmd.contains("go mod")
        || cmd.contains("dotnet restore")
        || cmd.contains("mvn install")
}

fn is_package_manager_command(cmd: &str) -> bool {
    cmd.contains("bundle")
        || cmd.contains("npm")
        || cmd.contains("yarn")
        || cmd.contains("pip")
        || cmd.contains("poetry")
        || cmd.contains("cargo")
        || cmd.contains("mix")
        || cmd.contains("go mod")
        || cmd.contains("composer")
        || cmd.contains("maven")
        || cmd.contains("gradle")
        || cmd.contains("dotnet")
}

fn detect_ecosystem(cmd: &str) -> &str {
    if cmd.contains("bundle") || cmd.contains("gem") || cmd.contains("ruby") {
        "ruby"
    } else if cmd.contains("npm")
        || cmd.contains("yarn")
        || cmd.contains("node")
        || cmd.contains("pnpm")
    {
        "node"
    } else if cmd.contains("pip")
        || cmd.contains("python")
        || cmd.contains("poetry")
        || cmd.contains("conda")
    {
        "python"
    } else if cmd.contains("cargo") || cmd.contains("rustc") {
        "rust"
    } else if cmd.contains("go ") || cmd.contains("go.") {
        "go"
    } else if cmd.contains("mix") || cmd.contains("elixir") {
        "elixir"
    } else if cmd.contains("mvn") || cmd.contains("gradle") || cmd.contains("java") {
        "java"
    } else if cmd.contains("dotnet") || cmd.contains("nuget") {
        "dotnet"
    } else if cmd.contains("docker") {
        "docker"
    } else if cmd.contains("composer") || cmd.contains("php") {
        "php"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_ctx<'a>(command: &'a str, requires: &'a [String]) -> StepContext<'a> {
        StepContext {
            name: "test",
            command,
            requires,
            template: None,
        }
    }

    #[test]
    fn connection_refused_boosted_by_requires() {
        let requires = vec!["postgres-server".to_string()];
        let ctx = make_ctx("rails db:create", &requires);
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let mut cats = vec![CategoryMatch {
            category: ErrorCategory::ConnectionRefused,
            confidence: 0.3,
        }];
        let mut details = DiagnosticDetails::default();

        contextualize(&mut cats, &mut details, &ctx, &ws);

        assert!(cats[0].confidence > 0.3);
        assert_eq!(details.service.as_deref(), Some("postgres-server"));
    }

    #[test]
    fn not_found_boosted_by_install_command() {
        let ctx = make_ctx("pip install -r requirements.txt", &[]);
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let mut cats = vec![CategoryMatch {
            category: ErrorCategory::NotFound,
            confidence: 0.3,
        }];
        let mut details = DiagnosticDetails::default();

        contextualize(&mut cats, &mut details, &ctx, &ws);

        assert!(cats[0].confidence > 0.3);
    }

    #[test]
    fn system_constraint_boosted_by_pip() {
        let ctx = make_ctx("pip install flask", &[]);
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let mut cats = vec![CategoryMatch {
            category: ErrorCategory::SystemConstraint,
            confidence: 0.3,
        }];
        let mut details = DiagnosticDetails::default();

        contextualize(&mut cats, &mut details, &ctx, &ws);

        assert!(cats[0].confidence > 0.3);
    }

    #[test]
    fn ecosystem_detection() {
        assert_eq!(detect_ecosystem("bundle install"), "ruby");
        assert_eq!(detect_ecosystem("npm install"), "node");
        assert_eq!(detect_ecosystem("pip install flask"), "python");
        assert_eq!(detect_ecosystem("cargo build"), "rust");
        assert_eq!(detect_ecosystem("echo hello"), "");
    }

    #[test]
    fn confidence_clamped_to_1() {
        let ctx = make_ctx("pip install flask", &[]);
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let mut cats = vec![CategoryMatch {
            category: ErrorCategory::SystemConstraint,
            confidence: 0.95,
        }];
        let mut details = DiagnosticDetails::default();

        contextualize(&mut cats, &mut details, &ctx, &ws);

        assert!(cats[0].confidence <= 1.0);
    }

    #[test]
    fn resorts_after_boost() {
        let ctx = make_ctx("bundle install", &[]);
        let outcomes = HashMap::new();
        let ws = WorkflowState {
            steps: &[],
            outcomes: &outcomes,
        };

        let mut cats = vec![
            CategoryMatch {
                category: ErrorCategory::BuildFailure,
                confidence: 0.4,
            },
            CategoryMatch {
                category: ErrorCategory::NotFound,
                confidence: 0.3,
            },
        ];
        let mut details = DiagnosticDetails::default();

        contextualize(&mut cats, &mut details, &ctx, &ws);

        // NotFound gets boosted by install command, may reorder
        assert!(cats[0].confidence >= cats[1].confidence);
    }
}
