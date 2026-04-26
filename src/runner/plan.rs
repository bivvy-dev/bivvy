//! Execution plan building for workflow runs.
//!
//! This module extracts the common plan-building logic that computes which steps
//! to run in what order. It handles dependency graph construction, skip computation,
//! topological ordering, and environment/filter application.

use std::collections::HashSet;

use crate::error::Result;
use crate::steps::ResolvedStep;

use super::dependency::DependencyGraph;
use super::workflow::RunOptions;

/// The result of building an execution plan.
///
/// Contains the ordered list of steps to run and the steps that were filtered out.
pub struct ExecutionPlan {
    /// Steps to execute, in dependency-respecting order.
    pub steps_to_run: Vec<String>,
    /// Steps skipped by the `--skip` flag (and their dependents).
    pub flag_skipped: HashSet<String>,
    /// Steps skipped by `only_environments` filtering.
    pub env_skipped: Vec<String>,
}

/// Build an execution plan for the given workflow.
///
/// This resolves the dependency graph, computes skips from flags, orders steps
/// topologically (preserving workflow declaration order for siblings), and filters
/// by `--only`, `--skip`, and `only_environments`.
pub fn build_execution_plan(
    graph: &DependencyGraph,
    workflow_steps: &[String],
    options: &RunOptions,
    resolved_steps: &std::collections::HashMap<String, ResolvedStep>,
) -> Result<ExecutionPlan> {
    // Compute skips from --skip flag
    let skipped = graph.compute_skips(&options.skip, options.skip_behavior);

    // Get execution order (stable: preserves workflow declaration order for siblings)
    let order = graph.topological_order_stable(workflow_steps)?;

    // Filter by only_environments and --only/--skip
    let mut env_skipped: Vec<String> = Vec::new();
    let steps_to_run: Vec<_> = order
        .iter()
        .filter(|s| !skipped.contains(*s))
        .filter(|s| options.only.is_empty() || options.only.contains(*s))
        .filter(|s| {
            if let Some(step) = resolved_steps.get(*s) {
                if !step.scoping.only_environments.is_empty() {
                    if let Some(ref active_env) = options.active_environment {
                        if !step
                            .scoping
                            .only_environments
                            .iter()
                            .any(|e| e == active_env)
                        {
                            env_skipped.push(s.to_string());
                            return false;
                        }
                    }
                }
            }
            true
        })
        .cloned()
        .collect();

    Ok(ExecutionPlan {
        steps_to_run,
        flag_skipped: skipped,
        env_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::dependency::{DependencyGraph, SkipBehavior};
    use crate::steps::{
        ResolvedBehavior, ResolvedEnvironmentVars, ResolvedExecution, ResolvedHooks,
        ResolvedOutput, ResolvedScoping, ResolvedStep,
    };
    use std::collections::HashMap;

    fn make_step(name: &str, depends_on: Vec<String>) -> ResolvedStep {
        ResolvedStep {
            name: name.to_string(),
            title: name.to_string(),
            description: None,
            depends_on,
            requires: vec![],
            inputs: HashMap::new(),
            execution: ResolvedExecution {
                command: format!("echo {}", name),
                ..Default::default()
            },
            env_vars: ResolvedEnvironmentVars::default(),
            behavior: ResolvedBehavior::default(),
            hooks: ResolvedHooks::default(),
            output: ResolvedOutput::default(),
            scoping: ResolvedScoping::default(),
        }
    }

    #[test]
    fn plan_basic_ordering() {
        let graph = DependencyGraph::builder()
            .add_step("a".to_string(), vec![])
            .add_step("b".to_string(), vec!["a".to_string()])
            .build()
            .unwrap();

        let workflow_steps = vec!["a".to_string(), "b".to_string()];
        let mut resolved = HashMap::new();
        resolved.insert("a".to_string(), make_step("a", vec![]));
        resolved.insert("b".to_string(), make_step("b", vec!["a".to_string()]));

        let plan = build_execution_plan(&graph, &workflow_steps, &RunOptions::default(), &resolved)
            .unwrap();

        assert_eq!(plan.steps_to_run, vec!["a", "b"]);
        assert!(plan.flag_skipped.is_empty());
        assert!(plan.env_skipped.is_empty());
    }

    #[test]
    fn plan_skip_flag_cascades() {
        let graph = DependencyGraph::builder()
            .add_step("a".to_string(), vec![])
            .add_step("b".to_string(), vec!["a".to_string()])
            .build()
            .unwrap();

        let workflow_steps = vec!["a".to_string(), "b".to_string()];
        let mut resolved = HashMap::new();
        resolved.insert("a".to_string(), make_step("a", vec![]));
        resolved.insert("b".to_string(), make_step("b", vec!["a".to_string()]));

        let options = RunOptions {
            skip: {
                let mut s = HashSet::new();
                s.insert("a".to_string());
                s
            },
            skip_behavior: SkipBehavior::SkipWithDependents,
            ..Default::default()
        };

        let plan = build_execution_plan(&graph, &workflow_steps, &options, &resolved).unwrap();

        assert!(plan.steps_to_run.is_empty());
        assert!(plan.flag_skipped.contains("a"));
        assert!(plan.flag_skipped.contains("b"));
    }

    #[test]
    fn plan_environment_filtering() {
        let graph = DependencyGraph::builder()
            .add_step("always".to_string(), vec![])
            .add_step("ci_only".to_string(), vec![])
            .build()
            .unwrap();

        let workflow_steps = vec!["always".to_string(), "ci_only".to_string()];
        let mut resolved = HashMap::new();
        resolved.insert("always".to_string(), make_step("always", vec![]));
        let mut ci_step = make_step("ci_only", vec![]);
        ci_step.scoping.only_environments = vec!["ci".to_string()];
        resolved.insert("ci_only".to_string(), ci_step);

        let options = RunOptions {
            active_environment: Some("development".to_string()),
            ..Default::default()
        };

        let plan = build_execution_plan(&graph, &workflow_steps, &options, &resolved).unwrap();

        assert_eq!(plan.steps_to_run, vec!["always"]);
        assert_eq!(plan.env_skipped, vec!["ci_only"]);
    }

    #[test]
    fn plan_only_flag_filters() {
        let graph = DependencyGraph::builder()
            .add_step("a".to_string(), vec![])
            .add_step("b".to_string(), vec![])
            .build()
            .unwrap();

        let workflow_steps = vec!["a".to_string(), "b".to_string()];
        let mut resolved = HashMap::new();
        resolved.insert("a".to_string(), make_step("a", vec![]));
        resolved.insert("b".to_string(), make_step("b", vec![]));

        let options = RunOptions {
            only: {
                let mut s = HashSet::new();
                s.insert("a".to_string());
                s
            },
            ..Default::default()
        };

        let plan = build_execution_plan(&graph, &workflow_steps, &options, &resolved).unwrap();

        assert_eq!(plan.steps_to_run, vec!["a"]);
    }
}
