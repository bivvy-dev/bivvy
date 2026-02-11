//! Workflow execution orchestration.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::config::BivvyConfig;
use crate::error::{BivvyError, Result};
use crate::steps::{
    execute_step, run_check, ExecutionOptions, ResolvedStep, StepResult, StepStatus,
};
use crate::ui::{format_duration, Prompt, PromptType, UserInterface};

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

            let step_start = Instant::now();
            let result =
                match execute_step(step, project_root, context, global_env, &exec_options, None) {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("Step '{}' errored: {}", step_name, e);
                        StepResult::failure(step_name, step_start.elapsed(), e.to_string(), None)
                    }
                };

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

    /// Run the specified workflow with direct UI interaction.
    ///
    /// Unlike `run_with_progress`, this method takes a `UserInterface` directly
    /// and handles interactive prompts for completed steps and sensitive steps.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_ui(
        &self,
        options: &RunOptions,
        context: &InterpolationContext,
        global_env: &HashMap<String, String>,
        project_root: &Path,
        workflow_non_interactive: bool,
        step_overrides: &HashMap<String, StepOverride>,
        ui: &mut dyn UserInterface,
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
            ui.warning(&format!("  {} skipped", skip_name));
        }

        let interactive = ui.is_interactive() && !workflow_non_interactive;

        let mut results = Vec::new();
        let mut all_success = true;

        for (index, step_name) in steps_to_run.iter().enumerate() {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            // Resolve effective prompt_if_complete (step-level, possibly overridden)
            let effective_prompt_if_complete = step_overrides
                .get(step_name)
                .and_then(|o| o.prompt_if_complete)
                .unwrap_or(step.prompt_if_complete);

            let mut needs_force = options.force.contains(step_name);

            // Check if already complete (unless forced)
            if !needs_force && !options.dry_run {
                if let Some(ref check) = step.completed_check {
                    let check_result = run_check(check, project_root);
                    if check_result.complete {
                        if interactive && effective_prompt_if_complete {
                            if step.skippable {
                                // Ask if they want to re-run
                                let prompt = Prompt {
                                    key: format!("rerun_{}", step_name),
                                    question: format!(
                                        "'{}' is already complete. Re-run?",
                                        step.title
                                    ),
                                    prompt_type: PromptType::Confirm,
                                    default: Some("false".to_string()),
                                };

                                let answer = ui.prompt(&prompt)?;
                                if answer.as_bool() != Some(true) {
                                    // User declined; skip
                                    ui.show_progress(index + 1, total);
                                    ui.warning(&format!(
                                        "  {} skipped (already complete)",
                                        step_name
                                    ));
                                    results.push(StepResult::skipped(&step.name, check_result));
                                    continue;
                                }
                                // User wants to re-run, force past the check in execute_step
                                needs_force = true;
                            } else {
                                // Not skippable, inform and re-run
                                ui.message(&format!(
                                    "  '{}' is already complete, re-running (not skippable)",
                                    step.title
                                ));
                                needs_force = true;
                            }
                        } else {
                            // Not interactive or prompt_if_complete is false: silently skip
                            ui.show_progress(index + 1, total);
                            ui.warning(&format!("  {} skipped (already complete)", step_name));
                            results.push(StepResult::skipped(&step.name, check_result));
                            continue;
                        }
                    }
                }
            }

            // Sensitive confirmation
            if step.sensitive && interactive {
                let prompt = Prompt {
                    key: format!("sensitive_{}", step_name),
                    question: format!("'{}' handles sensitive data. Continue?", step.title),
                    prompt_type: PromptType::Confirm,
                    default: Some("true".to_string()),
                };

                let answer = ui.prompt(&prompt)?;
                if answer.as_bool() != Some(true) {
                    if step.skippable {
                        ui.show_progress(index + 1, total);
                        ui.warning(&format!(
                            "  {} skipped (declined sensitive step)",
                            step_name
                        ));
                        results.push(StepResult::skipped(
                            &step.name,
                            crate::steps::CheckResult::complete("User declined sensitive step"),
                        ));
                        continue;
                    } else {
                        return Err(BivvyError::StepExecutionError {
                            step: step_name.clone(),
                            message: format!(
                                "Step '{}' is sensitive and not skippable, but user declined",
                                step.title
                            ),
                        });
                    }
                }
            }

            ui.show_progress(index + 1, total);
            ui.message(&format!("  Running {}...", step_name));

            let exec_options = ExecutionOptions {
                force: needs_force,
                dry_run: options.dry_run,
                capture_output: true,
                ..Default::default()
            };

            let step_start = Instant::now();
            let result =
                match execute_step(step, project_root, context, global_env, &exec_options, None) {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("Step '{}' errored: {}", step_name, e);
                        StepResult::failure(step_name, step_start.elapsed(), e.to_string(), None)
                    }
                };

            let duration = format_duration(result.duration);
            match result.status() {
                StepStatus::Completed => {
                    ui.success(&format!("  {} ({})", step_name, duration));
                }
                StepStatus::Failed => {
                    ui.error(&format!("  {} failed ({})", step_name, duration));
                    if let Some(ref err) = result.error {
                        ui.error(&format!("    {}", err));
                    }
                    if let Some(ref output) = result.output {
                        let trimmed = output.trim();
                        if !trimmed.is_empty() {
                            for line in trimmed.lines() {
                                ui.message(&format!("    {}", line));
                            }
                        }
                    }
                }
                StepStatus::Skipped => {
                    ui.warning(&format!("  {} skipped", step_name));
                }
                _ => {}
            }

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
    use crate::config::schema::StepOverride;
    use crate::ui::MockUI;
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

    fn make_step_with_check(
        name: &str,
        command: &str,
        check: Option<crate::config::CompletedCheck>,
    ) -> ResolvedStep {
        ResolvedStep {
            name: name.to_string(),
            title: name.to_string(),
            description: None,
            command: command.to_string(),
            depends_on: vec![],
            completed_check: check,
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
    fn run_with_ui_executes_simple_step() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("ran.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "hello".to_string(),
            make_step("hello", &format!("touch {}", marker.display()), vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert_eq!(result.steps.len(), 1);
        assert!(!result.steps[0].skipped);
        // Verify the command actually ran
        assert!(
            marker.exists(),
            "step command should have created marker file"
        );
    }

    #[test]
    fn run_with_ui_interactive_no_check_does_not_force() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "hello".to_string(),
            make_step("hello", "echo hello", vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // No prompts should appear for a step without completed_check
        assert!(ui.prompts_shown().is_empty());
    }

    #[test]
    fn run_with_ui_incomplete_check_runs_without_force() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("ran.txt");
        // Don't create marker.txt — so the check will NOT pass

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "install".to_string(),
            make_step_with_check(
                "install",
                &format!("touch {}", marker.display()),
                Some(crate::config::CompletedCheck::FileExists {
                    path: "marker.txt".to_string(),
                }),
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // No prompt — check didn't pass, so step runs normally
        assert!(ui.prompts_shown().is_empty());
        // Verify command actually ran
        assert!(
            marker.exists(),
            "step should run when completed_check does not pass"
        );
    }

    #[test]
    fn run_with_ui_prompts_when_complete_and_interactive() {
        let temp = TempDir::new().unwrap();
        // Create the file so the check passes
        fs::write(temp.path().join("marker.txt"), "done").unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "install".to_string(),
            make_step_with_check(
                "install",
                "echo installed",
                Some(crate::config::CompletedCheck::FileExists {
                    path: "marker.txt".to_string(),
                }),
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // User declines re-run
        ui.set_prompt_response("rerun_install", "false");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // Should have prompted
        assert!(ui.prompts_shown().contains(&"rerun_install".to_string()));
        // Step should be skipped since user declined
        assert_eq!(result.steps.len(), 1);
        assert!(result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_reruns_when_user_confirms() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("marker.txt"), "done").unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "install".to_string(),
            make_step_with_check(
                "install",
                "echo reinstalled",
                Some(crate::config::CompletedCheck::FileExists {
                    path: "marker.txt".to_string(),
                }),
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // User confirms re-run
        ui.set_prompt_response("rerun_install", "true");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(ui.prompts_shown().contains(&"rerun_install".to_string()));
        // Step should have run (not skipped)
        assert_eq!(result.steps.len(), 1);
        assert!(!result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_silent_skip_when_not_interactive() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("marker.txt"), "done").unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "install".to_string(),
            make_step_with_check(
                "install",
                "echo installed",
                Some(crate::config::CompletedCheck::FileExists {
                    path: "marker.txt".to_string(),
                }),
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        // Not interactive — should silently skip

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // No prompts should have been shown
        assert!(ui.prompts_shown().is_empty());
        // Step should be skipped
        assert_eq!(result.steps.len(), 1);
        assert!(result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_silent_skip_when_prompt_if_complete_false() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("marker.txt"), "done").unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut step = make_step_with_check(
            "install",
            "echo installed",
            Some(crate::config::CompletedCheck::FileExists {
                path: "marker.txt".to_string(),
            }),
        );
        step.prompt_if_complete = false;

        let mut steps = HashMap::new();
        steps.insert("install".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // Should NOT have prompted even though interactive
        assert!(ui.prompts_shown().is_empty());
        assert!(result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_sensitive_step_prompts() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [deploy]
        "#,
        )
        .unwrap();

        let mut step = make_step("deploy", "echo deployed", vec![]);
        step.sensitive = true;

        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // User confirms
        ui.set_prompt_response("sensitive_deploy", "true");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(ui.prompts_shown().contains(&"sensitive_deploy".to_string()));
        assert!(!result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_sensitive_not_skippable_declined_errors() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [deploy]
        "#,
        )
        .unwrap();

        let mut step = make_step("deploy", "echo deployed", vec![]);
        step.sensitive = true;
        step.skippable = false;

        let mut steps = HashMap::new();
        steps.insert("deploy".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // User declines
        ui.set_prompt_response("sensitive_deploy", "false");

        let result = runner.run_with_ui(
            &options,
            &ctx,
            &HashMap::new(),
            temp.path(),
            false,
            &HashMap::new(),
            &mut ui,
        );

        assert!(result.is_err());
    }

    #[test]
    fn run_with_ui_workflow_non_interactive_suppresses_prompts() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("marker.txt"), "done").unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "install".to_string(),
            make_step_with_check(
                "install",
                "echo installed",
                Some(crate::config::CompletedCheck::FileExists {
                    path: "marker.txt".to_string(),
                }),
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);

        // Even though UI is interactive, workflow_non_interactive should suppress prompts
        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                true, // workflow_non_interactive
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(ui.prompts_shown().is_empty());
        assert!(result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_step_override_disables_prompt_if_complete() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("marker.txt"), "done").unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [install]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "install".to_string(),
            make_step_with_check(
                "install",
                "echo installed",
                Some(crate::config::CompletedCheck::FileExists {
                    path: "marker.txt".to_string(),
                }),
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);

        // Override prompt_if_complete to false for this step
        let mut overrides = HashMap::new();
        overrides.insert(
            "install".to_string(),
            StepOverride {
                prompt_if_complete: Some(false),
                ..Default::default()
            },
        );

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &overrides,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // Should NOT have prompted because override disables it
        assert!(ui.prompts_shown().is_empty());
        assert!(result.steps[0].skipped);
    }

    #[test]
    fn failed_step_stops_dependent_step() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("after.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [failing, after_fail]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "failing".to_string(),
            make_step("failing", "exit 1", vec![]),
        );
        steps.insert(
            "after_fail".to_string(),
            make_step(
                "after_fail",
                &format!("touch {}", marker.display()),
                vec!["failing".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        // Should return Ok (not Err), but marked as failed
        assert!(!result.success);
        // Only the failing step ran; after_fail was blocked by the break
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].name, "failing");
        assert!(!result.steps[0].success);
        assert!(!marker.exists());
    }

    #[test]
    fn allow_failure_continues_to_next_step() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("after.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [failing, after_fail]
        "#,
        )
        .unwrap();

        let mut failing_step = make_step("failing", "exit 1", vec![]);
        failing_step.allow_failure = true;

        let mut steps = HashMap::new();
        steps.insert("failing".to_string(), failing_step);
        steps.insert(
            "after_fail".to_string(),
            make_step(
                "after_fail",
                &format!("touch {}", marker.display()),
                vec!["failing".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        // Workflow reports failure (a step failed)
        assert!(!result.success);
        // Both steps ran
        assert_eq!(result.steps.len(), 2);

        let failing = result.steps.iter().find(|s| s.name == "failing").unwrap();
        let after = result
            .steps
            .iter()
            .find(|s| s.name == "after_fail")
            .unwrap();
        assert!(!failing.success);
        assert!(after.success);
        assert!(marker.exists());
    }

    #[test]
    fn step_execution_error_does_not_abort_workflow() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("second.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [broken, healthy]
        "#,
        )
        .unwrap();

        // A step with a before-hook that fails causes execute_step to return Err.
        // With allow_failure, the workflow should continue to the next step.
        let mut broken_step = make_step("broken", "echo main", vec![]);
        broken_step.before = vec!["exit 1".to_string()];
        broken_step.allow_failure = true;

        let mut steps = HashMap::new();
        steps.insert("broken".to_string(), broken_step);
        steps.insert(
            "healthy".to_string(),
            make_step(
                "healthy",
                &format!("touch {}", marker.display()),
                vec!["broken".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        // Workflow completed (not Err), but not fully successful
        assert!(!result.success);
        // Both steps were processed
        assert_eq!(result.steps.len(), 2);

        let broken = result.steps.iter().find(|s| s.name == "broken").unwrap();
        let healthy = result.steps.iter().find(|s| s.name == "healthy").unwrap();
        assert!(!broken.success);
        assert!(healthy.success);
        assert!(marker.exists());
    }

    #[test]
    fn step_execution_error_stops_when_not_allow_failure() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("second.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [broken, healthy]
        "#,
        )
        .unwrap();

        // Before-hook failure with allow_failure = false (default).
        // Previously this returned Err and aborted the entire run.
        // Now it should produce a WorkflowResult with success=false.
        let mut broken_step = make_step("broken", "echo main", vec![]);
        broken_step.before = vec!["exit 1".to_string()];

        let mut steps = HashMap::new();
        steps.insert("broken".to_string(), broken_step);
        steps.insert(
            "healthy".to_string(),
            make_step(
                "healthy",
                &format!("touch {}", marker.display()),
                vec!["broken".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        // Returns Ok (not Err), marked as failed
        assert!(!result.success);
        // Only broken step ran; healthy was blocked
        assert_eq!(result.steps.len(), 1);
        assert!(!result.steps[0].success);
        assert!(!marker.exists());
    }

    #[test]
    fn run_with_ui_step_error_continues_with_allow_failure() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("second.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [broken, healthy]
        "#,
        )
        .unwrap();

        let mut broken_step = make_step("broken", "echo main", vec![]);
        broken_step.before = vec!["exit 1".to_string()];
        broken_step.allow_failure = true;

        let mut steps = HashMap::new();
        steps.insert("broken".to_string(), broken_step);
        steps.insert(
            "healthy".to_string(),
            make_step(
                "healthy",
                &format!("touch {}", marker.display()),
                vec!["broken".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.steps.len(), 2);

        let broken = result.steps.iter().find(|s| s.name == "broken").unwrap();
        let healthy = result.steps.iter().find(|s| s.name == "healthy").unwrap();
        assert!(!broken.success);
        assert!(healthy.success);
        assert!(marker.exists());
    }

    #[test]
    fn run_with_ui_shows_error_output_on_failure() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [broken]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "broken".to_string(),
            make_step("broken", "echo 'something went wrong' >&2; exit 1", vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                &mut ui,
            )
            .unwrap();

        assert!(!result.success);
        // Error line should show the exit code
        assert!(ui.has_error("Command failed with exit code"));
        // Captured stderr should be surfaced as messages
        assert!(ui.has_message("something went wrong"));
    }

    #[test]
    fn allow_failure_lets_all_dependent_steps_run() {
        let temp = TempDir::new().unwrap();
        let marker_b = temp.path().join("b.txt");
        let marker_c = temp.path().join("c.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [a, b, c]
        "#,
        )
        .unwrap();

        // Step a fails but has allow_failure; b and c form a chain
        let mut step_a = make_step("a", "exit 1", vec![]);
        step_a.allow_failure = true;

        let mut steps = HashMap::new();
        steps.insert("a".to_string(), step_a);
        steps.insert(
            "b".to_string(),
            make_step(
                "b",
                &format!("touch {}", marker_b.display()),
                vec!["a".to_string()],
            ),
        );
        steps.insert(
            "c".to_string(),
            make_step(
                "c",
                &format!("touch {}", marker_c.display()),
                vec!["b".to_string()],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        assert!(!result.success);
        // All 3 steps ran despite a's failure
        assert_eq!(result.steps.len(), 3);

        let step_a_result = result.steps.iter().find(|s| s.name == "a").unwrap();
        let step_b_result = result.steps.iter().find(|s| s.name == "b").unwrap();
        let step_c_result = result.steps.iter().find(|s| s.name == "c").unwrap();
        assert!(!step_a_result.success);
        assert!(step_b_result.success);
        assert!(step_c_result.success);
        assert!(marker_b.exists());
        assert!(marker_c.exists());
    }
}
