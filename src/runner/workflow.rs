//! Workflow execution orchestration.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::config::interpolation::InterpolationContext;
use crate::config::BivvyConfig;
use crate::error::{BivvyError, Result};
use crate::steps::{execute_step, ExecutionOptions, ResolvedStep, StepResult, StepStatus};

use super::dependency::{DependencyGraph, SkipBehavior};

/// Progress events emitted during workflow execution.
#[derive(Debug)]
pub enum RunProgress<'a> {
    /// A step is about to start.
    StepStarting {
        name: &'a str,
        index: usize,
        total: usize,
    },
    /// A step finished.
    StepFinished {
        name: &'a str,
        result: &'a StepResult,
    },
    /// A step was skipped.
    StepSkipped { name: &'a str },
}

/// Orchestrates the execution of a workflow.
pub struct WorkflowRunner<'a> {
    config: &'a BivvyConfig,
    steps: HashMap<String, ResolvedStep>,
}

/// Result of running a workflow.
#[derive(Debug)]
pub struct WorkflowResult {
    /// Workflow name.
    pub workflow: String,
    /// Results from each executed step.
    pub steps: Vec<StepResult>,
    /// Names of skipped steps.
    pub skipped: Vec<String>,
    /// Total duration.
    pub duration: Duration,
    /// Whether all steps succeeded.
    pub success: bool,
}

/// Options for running a workflow.
#[derive(Debug, Default)]
pub struct RunOptions {
    /// Workflow name (defaults to "default").
    pub workflow: Option<String>,
    /// Only run these steps.
    pub only: HashSet<String>,
    /// Skip these steps.
    pub skip: HashSet<String>,
    /// How to handle skipped step dependencies.
    pub skip_behavior: SkipBehavior,
    /// Force re-run these steps even if complete.
    pub force: HashSet<String>,
    /// Dry run mode.
    pub dry_run: bool,
}

impl<'a> WorkflowRunner<'a> {
    /// Create a new workflow runner.
    pub fn new(config: &'a BivvyConfig, steps: HashMap<String, ResolvedStep>) -> Self {
        Self { config, steps }
    }

    /// Run the specified workflow.
    pub fn run(
        &self,
        options: &RunOptions,
        context: &InterpolationContext,
        global_env: &HashMap<String, String>,
        project_root: &Path,
    ) -> Result<WorkflowResult> {
        self.run_with_progress(options, context, global_env, project_root, |_| {})
    }

    /// Run the specified workflow with a progress callback.
    pub fn run_with_progress(
        &self,
        options: &RunOptions,
        context: &InterpolationContext,
        global_env: &HashMap<String, String>,
        project_root: &Path,
        mut on_progress: impl FnMut(RunProgress<'_>),
    ) -> Result<WorkflowResult> {
        let start = Instant::now();
        let workflow_name = options.workflow.as_deref().unwrap_or("default");

        // Build dependency graph
        let graph = self.build_graph(workflow_name)?;

        // Compute skips
        let skipped = graph.compute_skips(&options.skip, options.skip_behavior);

        // Get execution order
        let order = graph.topological_order()?;

        // Filter to only steps we'll run
        let steps_to_run: Vec<_> = order
            .iter()
            .filter(|s| !skipped.contains(*s))
            .filter(|s| options.only.is_empty() || options.only.contains(*s))
            .cloned()
            .collect();

        let total = steps_to_run.len();

        // Report skipped steps
        for skip_name in &skipped {
            on_progress(RunProgress::StepSkipped { name: skip_name });
        }

        let mut results = Vec::new();
        let mut all_success = true;

        for (index, step_name) in steps_to_run.iter().enumerate() {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            on_progress(RunProgress::StepStarting {
                name: step_name,
                index,
                total,
            });

            let exec_options = ExecutionOptions {
                force: options.force.contains(step_name),
                dry_run: options.dry_run,
                capture_output: true,
                ..Default::default()
            };

            let result =
                execute_step(step, project_root, context, global_env, &exec_options, None)?;

            on_progress(RunProgress::StepFinished {
                name: step_name,
                result: &result,
            });

            let status = result.status();
            let should_stop = !result.success && !step.allow_failure;

            results.push(result);

            if status == StepStatus::Failed {
                all_success = false;
                if should_stop {
                    break;
                }
            }
        }

        Ok(WorkflowResult {
            workflow: workflow_name.to_string(),
            steps: results,
            skipped: skipped.into_iter().collect(),
            duration: start.elapsed(),
            success: all_success,
        })
    }

    /// Build the dependency graph for the given workflow.
    pub fn build_graph(&self, workflow: &str) -> Result<DependencyGraph> {
        let workflow_config = self.config.workflows.get(workflow).ok_or_else(|| {
            BivvyError::ConfigValidationError {
                message: format!("Unknown workflow: {}", workflow),
            }
        })?;

        let mut builder = DependencyGraph::builder();

        for step_name in &workflow_config.steps {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!(
                            "Workflow '{}' references unknown step '{}'",
                            workflow, step_name
                        ),
                    })?;

            builder = builder.add_step(step_name.clone(), step.depends_on.clone());
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_step(name: &str, command: &str, depends_on: Vec<String>) -> ResolvedStep {
        ResolvedStep {
            name: name.to_string(),
            title: name.to_string(),
            description: None,
            command: command.to_string(),
            depends_on,
            completed_check: None,
            skippable: true,
            required: false,
            prompt_if_complete: true,
            allow_failure: false,
            retry: 0,
            env: HashMap::new(),
            watches: vec![],
            before: vec![],
            after: vec![],
            sensitive: false,
            requires_sudo: false,
        }
    }

    #[test]
    fn run_workflow_in_order() {
        let temp = TempDir::new().unwrap();
        let order_file = temp.path().join("order.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [first, second]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "first".to_string(),
            make_step(
                "first",
                &format!("echo first >> {}", order_file.display()),
                vec![],
            ),
        );
        steps.insert(
            "second".to_string(),
            make_step(
                "second",
                &format!("echo second >> {}", order_file.display()),
                vec!["first".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        assert!(result.success);

        let content = fs::read_to_string(&order_file).unwrap();
        let lines: Vec<_> = content.lines().map(|l| l.trim()).collect();
        assert_eq!(lines, vec!["first", "second"]);
    }

    #[test]
    fn run_workflow_with_skip() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [first, second]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "first".to_string(),
            make_step("first", "echo first", vec![]),
        );
        steps.insert(
            "second".to_string(),
            make_step("second", "echo second", vec!["first".to_string()]),
        );

        let runner = WorkflowRunner::new(&config, steps);

        let options = RunOptions {
            skip: {
                let mut s = HashSet::new();
                s.insert("first".to_string());
                s
            },
            skip_behavior: SkipBehavior::SkipWithDependents,
            ..Default::default()
        };

        let ctx = InterpolationContext::new();
        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        assert!(result.skipped.contains(&"first".to_string()));
        assert!(result.skipped.contains(&"second".to_string()));
    }

    #[test]
    fn dry_run_does_not_execute() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("ran.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [test]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "test".to_string(),
            make_step("test", &format!("touch {}", marker.display()), vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);

        let options = RunOptions {
            dry_run: true,
            ..Default::default()
        };

        let ctx = InterpolationContext::new();
        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        assert!(result.success);
        assert!(!marker.exists());
    }

    #[test]
    fn run_with_progress_emits_events() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [step_a]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "step_a".to_string(),
            make_step("step_a", "echo hello", vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut events = Vec::new();
        let result = runner
            .run_with_progress(&options, &ctx, &HashMap::new(), temp.path(), |progress| {
                match &progress {
                    RunProgress::StepStarting { name, .. } => {
                        events.push(format!("start:{}", name));
                    }
                    RunProgress::StepFinished { name, .. } => {
                        events.push(format!("finish:{}", name));
                    }
                    RunProgress::StepSkipped { name } => {
                        events.push(format!("skip:{}", name));
                    }
                }
            })
            .unwrap();

        assert!(result.success);
        assert_eq!(events, vec!["start:step_a", "finish:step_a"]);
    }

    #[test]
    fn run_with_progress_reports_skips() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [first, second]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "first".to_string(),
            make_step("first", "echo first", vec![]),
        );
        steps.insert(
            "second".to_string(),
            make_step("second", "echo second", vec!["first".to_string()]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions {
            skip: {
                let mut s = HashSet::new();
                s.insert("first".to_string());
                s
            },
            skip_behavior: SkipBehavior::SkipWithDependents,
            ..Default::default()
        };

        let ctx = InterpolationContext::new();
        let mut skipped_names = Vec::new();
        runner
            .run_with_progress(&options, &ctx, &HashMap::new(), temp.path(), |progress| {
                if let RunProgress::StepSkipped { name } = progress {
                    skipped_names.push(name.to_string());
                }
            })
            .unwrap();

        assert!(skipped_names.contains(&"first".to_string()));
        assert!(skipped_names.contains(&"second".to_string()));
    }

    #[test]
    fn build_graph_validates_workflow() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [missing]
        "#,
        )
        .unwrap();

        let steps = HashMap::new();
        let runner = WorkflowRunner::new(&config, steps);

        let result = runner.build_graph("default");
        assert!(result.is_err());
    }
}
