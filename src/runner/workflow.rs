//! Workflow execution orchestration.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::config::interpolation::InterpolationContext;
use crate::config::schema::StepOverride;
use crate::config::BivvyConfig;
use crate::error::{BivvyError, Result};
use crate::requirements::checker::GapChecker;
use crate::requirements::installer;
use crate::steps::{
    execute_step, run_check, ExecutionOptions, ResolvedStep, StepResult, StepStatus,
};
use crate::ui::spinner::live_output_callback;
use crate::ui::theme::BivvyTheme;
use crate::ui::{
    format_duration, OutputMode, Prompt, PromptOption, PromptType, StatusKind, UserInterface,
};

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
    /// Requirements that are provided by the environment and should skip gap checks.
    pub provided_requirements: HashSet<String>,
    /// Active environment name for only_environments filtering.
    pub active_environment: Option<String>,
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
        self.run_with_progress(options, context, global_env, project_root, None, |_| {})
    }

    /// Run the specified workflow with a progress callback.
    pub fn run_with_progress(
        &self,
        options: &RunOptions,
        context: &InterpolationContext,
        global_env: &HashMap<String, String>,
        project_root: &Path,
        mut gap_checker: Option<&mut GapChecker<'_>>,
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

        // Filter by only_environments and --only/--skip
        let mut env_skipped: Vec<String> = Vec::new();
        let steps_to_run: Vec<_> = order
            .iter()
            .filter(|s| !skipped.contains(*s))
            .filter(|s| options.only.is_empty() || options.only.contains(*s))
            .filter(|s| {
                if let Some(step) = self.steps.get(*s) {
                    if !step.only_environments.is_empty() {
                        if let Some(ref active_env) = options.active_environment {
                            if !step.only_environments.iter().any(|e| e == active_env) {
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

        let total = steps_to_run.len();

        // Report skipped steps
        for skip_name in &skipped {
            on_progress(RunProgress::StepSkipped { name: skip_name });
        }
        for skip_name in &env_skipped {
            on_progress(RunProgress::StepSkipped { name: skip_name });
        }

        let mut results = Vec::new();
        let mut all_success = true;
        let mut failed_steps: HashSet<String> = HashSet::new();

        for (index, step_name) in steps_to_run.iter().enumerate() {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            // Check if any dependency failed
            if step.depends_on.iter().any(|dep| failed_steps.contains(dep)) {
                on_progress(RunProgress::StepSkipped { name: step_name });
                all_success = false;
                failed_steps.insert(step_name.clone());
                continue;
            }

            // Check requirement gaps (non-UI: any blocking gap is an error)
            if let Some(ref mut checker) = gap_checker {
                let provided = if options.provided_requirements.is_empty() {
                    None
                } else {
                    Some(&options.provided_requirements)
                };
                let gaps = checker.check_step(step, provided);
                let blocking: Vec<_> = gaps
                    .iter()
                    .filter(|g| !g.status.is_satisfied() && !g.status.can_proceed())
                    .collect();
                if !blocking.is_empty() {
                    let names: Vec<_> = blocking.iter().map(|g| g.requirement.as_str()).collect();
                    return Err(BivvyError::RequirementMissing {
                        requirement: names.join(", "),
                        message: format!(
                            "Step '{}' requires: {}. Run 'bivvy lint' for details.",
                            step_name,
                            names.join(", ")
                        ),
                    });
                }
            }

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

            results.push(result);

            if status == StepStatus::Failed {
                all_success = false;
                if !step.allow_failure {
                    failed_steps.insert(step_name.clone());
                }
            }
        }

        let mut all_skipped: Vec<String> = skipped.into_iter().collect();
        all_skipped.extend(env_skipped);

        Ok(WorkflowResult {
            workflow: workflow_name.to_string(),
            steps: results,
            skipped: all_skipped,
            duration: start.elapsed(),
            success: all_success,
        })
    }

    /// Run the specified workflow with direct UI interaction.
    ///
    /// Unlike `run_with_progress`, this method takes a `UserInterface` directly
    /// and handles interactive prompts for completed steps and sensitive steps.
    /// An optional `GapChecker` enables requirement gap detection before step execution.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_ui(
        &self,
        options: &RunOptions,
        context: &InterpolationContext,
        global_env: &HashMap<String, String>,
        project_root: &Path,
        workflow_non_interactive: bool,
        step_overrides: &HashMap<String, StepOverride>,
        mut gap_checker: Option<&mut GapChecker<'_>>,
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

        // Filter by only_environments
        let mut env_skipped: Vec<String> = Vec::new();
        let steps_to_run: Vec<_> = order
            .iter()
            .filter(|s| !skipped.contains(*s))
            .filter(|s| options.only.is_empty() || options.only.contains(*s))
            .filter(|s| {
                if let Some(step) = self.steps.get(*s) {
                    if !step.only_environments.is_empty() {
                        if let Some(ref active_env) = options.active_environment {
                            if !step.only_environments.iter().any(|e| e == active_env) {
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

        let total = steps_to_run.len();
        let theme = BivvyTheme::new();

        // Report skipped steps (from --skip flag)
        for skip_name in &skipped {
            ui.message(&format!(
                "    {}",
                theme.format_skipped(&format!("{} skipped", skip_name))
            ));
        }
        for skip_name in &env_skipped {
            ui.message(&format!(
                "    {}",
                theme.format_skipped(&format!(
                    "{} skipped (not in {} environment)",
                    skip_name,
                    options.active_environment.as_deref().unwrap_or("unknown")
                ))
            ));
        }

        let interactive = ui.is_interactive() && !workflow_non_interactive;
        let installer_ctx = installer::default_context();

        let mut results = Vec::new();
        let mut all_success = true;
        let mut failed_steps: HashSet<String> = HashSet::new();

        for (index, step_name) in steps_to_run.iter().enumerate() {
            let step =
                self.steps
                    .get(step_name)
                    .ok_or_else(|| BivvyError::ConfigValidationError {
                        message: format!("Step '{}' not found in resolved steps", step_name),
                    })?;

            // Blank line between steps
            if index > 0 {
                ui.message("");
            }

            // Format step display with numbering: "[1/7] name — title" or "[1/7] name"
            let step_number = format!("[{}/{}]", index + 1, total);
            let step_display = if *step_name == step.title {
                format!(
                    "{} {}",
                    theme.step_number.apply_to(&step_number),
                    theme.step_title.apply_to(step_name)
                )
            } else {
                format!(
                    "{} {} {} {}",
                    theme.step_number.apply_to(&step_number),
                    theme.step_title.apply_to(step_name),
                    theme.dim.apply_to("—"),
                    theme.dim.apply_to(&step.title)
                )
            };

            // Check if any dependency failed
            if step.depends_on.iter().any(|dep| failed_steps.contains(dep)) {
                ui.message(&step_display);
                ui.message(&format!(
                    "    {}",
                    StatusKind::Blocked.format(&theme, "Blocked (dependency failed)")
                ));
                ui.show_workflow_progress(index + 1, total, start.elapsed());
                all_success = false;
                failed_steps.insert(step_name.clone());
                continue;
            }

            // Check requirement gaps before proceeding
            if let Some(ref mut checker) = gap_checker {
                let provided = if options.provided_requirements.is_empty() {
                    None
                } else {
                    Some(&options.provided_requirements)
                };
                let gaps = checker.check_step(step, provided);
                if !gaps.is_empty() {
                    let can_proceed =
                        installer::handle_gaps(&gaps, checker, ui, interactive, &installer_ctx)?;
                    if !can_proceed {
                        ui.message(&step_display);
                        ui.message(&format!(
                            "    {}",
                            StatusKind::Skipped.format(&theme, "Skipped (requirement not met)")
                        ));
                        ui.show_workflow_progress(index + 1, total, start.elapsed());
                        continue;
                    }
                }
            }

            // Resolve effective prompt_if_complete (step-level, possibly overridden)
            let effective_prompt_if_complete = step_overrides
                .get(step_name)
                .and_then(|o| o.prompt_if_complete)
                .unwrap_or(step.prompt_if_complete);

            let mut needs_force = options.force.contains(step_name);
            let mut already_prompted = false;
            let mut had_prompt = false;

            // Check if already complete (unless forced)
            if !needs_force && !options.dry_run {
                if let Some(ref check) = step.completed_check {
                    let check_result = run_check(check, project_root);
                    if check_result.complete {
                        if interactive && effective_prompt_if_complete {
                            if step.skippable {
                                // Ask if they want to re-run (prompt IS the step header)
                                let prompt = Prompt {
                                    key: format!("rerun_{}", step_name),
                                    question: format!("Already complete. Re-run {}?", step_display),
                                    prompt_type: PromptType::Select {
                                        options: vec![
                                            PromptOption {
                                                label: "No (n)".to_string(),
                                                value: "no".to_string(),
                                            },
                                            PromptOption {
                                                label: "Yes (y)".to_string(),
                                                value: "yes".to_string(),
                                            },
                                        ],
                                    },
                                    default: Some("no".to_string()),
                                };

                                let answer = ui.prompt(&prompt)?;
                                if answer.as_string() != "yes" {
                                    ui.message(&format!(
                                        "    {}",
                                        theme.format_skipped("Skipped (already complete)")
                                    ));
                                    results.push(StepResult::skipped(&step.name, check_result));
                                    continue;
                                }
                                // User wants to re-run
                                needs_force = true;
                                already_prompted = true;
                                had_prompt = true;
                            } else {
                                // Not skippable, inform and re-run
                                ui.message(&step_display);
                                ui.message("    Re-running (not skippable)");
                                needs_force = true;
                            }
                        } else {
                            // Not interactive or prompt_if_complete is false: silently skip
                            ui.message(&step_display);
                            ui.message(&format!(
                                "    {}",
                                theme.format_skipped("Skipped (already complete)")
                            ));
                            results.push(StepResult::skipped(&step.name, check_result));
                            continue;
                        }
                    }
                }
            }

            // In interactive mode, prompt before running skippable steps
            // (skip if already prompted by completed check)
            if interactive && step.skippable && !already_prompted {
                let prompt = Prompt {
                    key: format!("run_{}", step_name),
                    question: format!("Run {}?", step_display),
                    prompt_type: PromptType::Select {
                        options: vec![
                            PromptOption {
                                label: "Yes (y)".to_string(),
                                value: "yes".to_string(),
                            },
                            PromptOption {
                                label: "No (n)".to_string(),
                                value: "no".to_string(),
                            },
                        ],
                    },
                    default: Some("yes".to_string()),
                };
                let answer = ui.prompt(&prompt)?;
                if answer.as_string() != "yes" {
                    ui.message(&format!("    {}", theme.format_skipped("Skipped")));
                    results.push(StepResult::skipped(
                        &step.name,
                        crate::steps::CheckResult::complete("User declined"),
                    ));
                    continue;
                }
                had_prompt = true;
            }

            // Show step name if no prompt was shown (non-interactive or non-skippable)
            if !had_prompt && !already_prompted {
                ui.message(&step_display);
            }

            // Sensitive confirmation
            if step.sensitive && interactive {
                let prompt = Prompt {
                    key: format!("sensitive_{}", step_name),
                    question: "Handles sensitive data. Continue?".to_string(),
                    prompt_type: PromptType::Select {
                        options: vec![
                            PromptOption {
                                label: "Yes (y)".to_string(),
                                value: "yes".to_string(),
                            },
                            PromptOption {
                                label: "No (n)".to_string(),
                                value: "no".to_string(),
                            },
                        ],
                    },
                    default: Some("yes".to_string()),
                };

                let answer = ui.prompt(&prompt)?;
                if answer.as_string() != "yes" {
                    if step.skippable {
                        ui.message(&format!(
                            "    {}",
                            theme.format_skipped("Skipped (declined sensitive step)")
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
                had_prompt = true;
            }

            // Blank line before spinner when a prompt was shown (visual spacing)
            if had_prompt {
                ui.message("");
            }

            // Execute with indented spinner
            let spinner_msg = format!("Running `{}`...", step.command);
            let mut spinner = ui.start_spinner_indented(&spinner_msg, 4);

            // Create live output callback if the spinner has a progress bar
            let output_mode = ui.output_mode();
            let output_callback = spinner.progress_bar().and_then(|bar| {
                let max_lines = match output_mode {
                    OutputMode::Verbose => 3,
                    OutputMode::Normal => 2,
                    _ => return None,
                };
                Some(live_output_callback(bar, spinner_msg.clone(), 6, max_lines))
            });

            let exec_options = ExecutionOptions {
                force: needs_force,
                dry_run: options.dry_run,
                capture_output: output_callback.is_none(),
                ..Default::default()
            };

            let step_start = Instant::now();
            let result = match execute_step(
                step,
                project_root,
                context,
                global_env,
                &exec_options,
                output_callback,
            ) {
                Ok(result) => result,
                Err(e) => {
                    warn!("Step '{}' errored: {}", step_name, e);
                    StepResult::failure(step_name, step_start.elapsed(), e.to_string(), None)
                }
            };

            let duration_str = format_duration(result.duration);
            match result.status() {
                StepStatus::Completed => {
                    spinner.finish_success(&format!("{} ({})", step_name, duration_str));
                }
                StepStatus::Failed => {
                    spinner.finish_error(&format!("Failed ({})", duration_str));

                    // Show error block with command, combined output, and hint
                    let mut output_parts = Vec::new();
                    if let Some(ref err) = result.error {
                        output_parts.push(err.as_str());
                    }
                    if let Some(ref output) = result.output {
                        let trimmed = output.trim();
                        if !trimmed.is_empty() {
                            output_parts.push(trimmed);
                        }
                    }
                    let combined_output = output_parts.join("\n");
                    ui.show_error_block(&step.command, &combined_output, None);
                }
                StepStatus::Skipped => {
                    spinner.finish_skipped("Skipped");
                }
                _ => {}
            }

            // Update progress bar
            ui.show_workflow_progress(index + 1, total, start.elapsed());

            let status = result.status();

            results.push(result);

            if status == StepStatus::Failed {
                all_success = false;
                if !step.allow_failure {
                    failed_steps.insert(step_name.clone());
                }
            }
        }

        // Finish progress bar
        ui.finish_workflow_progress();

        let mut all_skipped: Vec<String> = skipped.into_iter().collect();
        all_skipped.extend(env_skipped);

        Ok(WorkflowResult {
            workflow: workflow_name.to_string(),
            steps: results,
            skipped: all_skipped,
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
            requires: vec![],
            only_environments: vec![],
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
            .run_with_progress(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                None,
                |progress| match &progress {
                    RunProgress::StepStarting { name, .. } => {
                        events.push(format!("start:{}", name));
                    }
                    RunProgress::StepFinished { name, .. } => {
                        events.push(format!("finish:{}", name));
                    }
                    RunProgress::StepSkipped { name } => {
                        events.push(format!("skip:{}", name));
                    }
                },
            )
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
            .run_with_progress(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                None,
                |progress| {
                    if let RunProgress::StepSkipped { name } = progress {
                        skipped_names.push(name.to_string());
                    }
                },
            )
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
            requires: vec![],
            only_environments: vec![],
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
                None,
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
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // Interactive mode prompts "Run 'hello'?" (default yes)
        assert!(ui.prompts_shown().contains(&"run_hello".to_string()));
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
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // Interactive mode prompts "Run 'install'?" (default yes), no completed check prompt
        assert!(ui.prompts_shown().contains(&"run_install".to_string()));
        assert!(!ui.prompts_shown().contains(&"rerun_install".to_string()));
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
        ui.set_prompt_response("rerun_install", "no");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
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
        ui.set_prompt_response("rerun_install", "yes");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
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
                None,
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
                None,
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
        // User confirms both the run prompt (default yes) and sensitive prompt
        ui.set_prompt_response("sensitive_deploy", "yes");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(ui.prompts_shown().contains(&"run_deploy".to_string()));
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
        ui.set_prompt_response("sensitive_deploy", "no");

        let result = runner.run_with_ui(
            &options,
            &ctx,
            &HashMap::new(),
            temp.path(),
            false,
            &HashMap::new(),
            None,
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
                None,
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
                None,
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
        // Only the failing step ran; after_fail was blocked by dependency failure
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
                None,
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
                None,
                &mut ui,
            )
            .unwrap();

        assert!(!result.success);
        // Error line should show the exit code
        assert!(ui.has_message("Command failed with exit code"));
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

    #[test]
    fn independent_steps_continue_after_failure() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("independent.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [failing, independent]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "failing".to_string(),
            make_step("failing", "exit 1", vec![]),
        );
        // independent has no depends_on, so it should still run
        steps.insert(
            "independent".to_string(),
            make_step(
                "independent",
                &format!("touch {}", marker.display()),
                vec![],
            ),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let result = runner
            .run(&options, &ctx, &HashMap::new(), temp.path())
            .unwrap();

        assert!(!result.success);
        // Both steps should appear in results
        assert_eq!(result.steps.len(), 2);
        let failing = result.steps.iter().find(|s| s.name == "failing").unwrap();
        let independent = result
            .steps
            .iter()
            .find(|s| s.name == "independent")
            .unwrap();
        assert!(!failing.success);
        assert!(independent.success);
        assert!(marker.exists());
    }

    #[test]
    fn transitive_dependency_blocked() {
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

        // a fails → b blocked (depends on a) → c blocked (depends on b)
        let mut steps = HashMap::new();
        steps.insert("a".to_string(), make_step("a", "exit 1", vec![]));
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
        // Only a ran; b and c were blocked
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].name, "a");
        assert!(!marker_b.exists());
        assert!(!marker_c.exists());
    }

    #[test]
    fn run_with_ui_prompts_before_each_skippable_step() {
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
            make_step("second", "echo second", vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        // Both default to "true", so both proceed

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert_eq!(result.steps.len(), 2);
        assert!(ui.prompts_shown().contains(&"run_first".to_string()));
        assert!(ui.prompts_shown().contains(&"run_second".to_string()));
    }

    #[test]
    fn run_with_ui_skip_declined_step() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("ran.txt");

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [optional]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "optional".to_string(),
            make_step("optional", &format!("touch {}", marker.display()), vec![]),
        );

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let mut ui = MockUI::new();
        ui.set_interactive(true);
        ui.set_prompt_response("run_optional", "no");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert_eq!(result.steps.len(), 1);
        assert!(result.steps[0].skipped);
        assert!(!marker.exists());
    }

    #[test]
    fn run_with_ui_no_prompt_when_not_skippable() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [required]
        "#,
        )
        .unwrap();

        let mut step = make_step("required", "echo required", vec![]);
        step.skippable = false;

        let mut steps = HashMap::new();
        steps.insert("required".to_string(), step);

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
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // No run prompt for non-skippable steps
        assert!(!ui.prompts_shown().contains(&"run_required".to_string()));
    }

    #[test]
    fn run_with_ui_no_prompt_when_non_interactive() {
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
        // Not interactive (default)

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(ui.prompts_shown().is_empty());
    }

    #[test]
    fn run_with_ui_completed_prompt_replaces_run_prompt() {
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
        // User confirms re-run
        ui.set_prompt_response("rerun_install", "yes");

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                None,
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // Should see rerun prompt but NOT the general run prompt
        assert!(ui.prompts_shown().contains(&"rerun_install".to_string()));
        assert!(!ui.prompts_shown().contains(&"run_install".to_string()));
        assert!(!result.steps[0].skipped);
    }

    #[test]
    fn run_with_ui_blocked_step_shows_warning() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [failing, dependent]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "failing".to_string(),
            make_step("failing", "exit 1", vec![]),
        );
        steps.insert(
            "dependent".to_string(),
            make_step("dependent", "echo dep", vec!["failing".to_string()]),
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
                None,
                &mut ui,
            )
            .unwrap();

        assert!(!result.success);
        // The dependent step should show a blocked message
        assert!(ui.has_message("Blocked (dependency failed)"));
    }

    // --- 1E-2: Gap checking integration tests ---

    use crate::requirements::checker::GapChecker;
    use crate::requirements::probe::EnvironmentProbe;
    use crate::requirements::registry::RequirementRegistry;
    use crate::requirements::status::RequirementStatus;

    fn make_gap_checker() -> (EnvironmentProbe, RequirementRegistry) {
        let probe = EnvironmentProbe::run_with_env(|_| Err(std::env::VarError::NotPresent));
        let registry = RequirementRegistry::new();
        (probe, registry)
    }

    #[test]
    fn run_with_ui_proceeds_when_all_satisfied() {
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

        let mut step = make_step("hello", &format!("touch {}", marker.display()), vec![]);
        step.requires = vec!["fake-tool".to_string()];

        let mut steps = HashMap::new();
        steps.insert("hello".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let (probe, registry) = make_gap_checker();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());
        // Pre-cache as satisfied
        checker
            .cache
            .insert("fake-tool".to_string(), RequirementStatus::Satisfied);

        let mut ui = MockUI::new();

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                Some(&mut checker),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(marker.exists());
    }

    #[test]
    fn run_with_ui_warns_on_system_only() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        let mut step = make_step("hello", "echo hello", vec![]);
        step.requires = vec!["system-ruby".to_string()];

        let mut steps = HashMap::new();
        steps.insert("hello".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let (probe, registry) = make_gap_checker();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());
        checker.cache.insert(
            "system-ruby".to_string(),
            RequirementStatus::SystemOnly {
                path: "/usr/bin/ruby".into(),
                install_template: None,
                warning: "System ruby detected at /usr/bin/ruby. Consider using a version manager."
                    .to_string(),
            },
        );

        let mut ui = MockUI::new();

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                Some(&mut checker),
                &mut ui,
            )
            .unwrap();

        // Should succeed — SystemOnly allows proceeding
        assert!(result.success);
        // Warning should be shown
        assert!(ui.has_warning("System ruby detected"));
    }

    #[test]
    fn run_with_ui_errors_on_unknown_requirement() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        let mut step = make_step("hello", "echo hello", vec![]);
        step.requires = vec!["nonexistent-xyz".to_string()];

        let mut steps = HashMap::new();
        steps.insert("hello".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let (probe, registry) = make_gap_checker();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());
        // Unknown is not cached — it's the default for unknown requirements
        // But we can pre-cache it for clarity
        checker
            .cache
            .insert("nonexistent-xyz".to_string(), RequirementStatus::Unknown);

        // Non-interactive (default MockUI) → should error
        let mut ui = MockUI::new();

        let result = runner.run_with_ui(
            &options,
            &ctx,
            &HashMap::new(),
            temp.path(),
            false,
            &HashMap::new(),
            Some(&mut checker),
            &mut ui,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BivvyError::RequirementMissing { .. }),
            "expected RequirementMissing, got: {}",
            err
        );
    }

    #[test]
    fn run_with_ui_non_interactive_fails_on_missing() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        let mut step = make_step("hello", "echo hello", vec![]);
        step.requires = vec!["missing-tool".to_string()];

        let mut steps = HashMap::new();
        steps.insert("hello".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let (probe, registry) = make_gap_checker();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());
        checker.cache.insert(
            "missing-tool".to_string(),
            RequirementStatus::Missing {
                install_template: None,
                install_hint: Some("brew install missing-tool".to_string()),
            },
        );

        // Non-interactive → should error
        let mut ui = MockUI::new();

        let result = runner.run_with_ui(
            &options,
            &ctx,
            &HashMap::new(),
            temp.path(),
            false,
            &HashMap::new(),
            Some(&mut checker),
            &mut ui,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BivvyError::RequirementMissing { .. }));
    }

    #[test]
    fn run_with_ui_interactive_warns_on_missing() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        let mut step = make_step("hello", "echo hello", vec![]);
        step.requires = vec!["missing-tool".to_string()];

        let mut steps = HashMap::new();
        steps.insert("hello".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let (probe, registry) = make_gap_checker();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());
        checker.cache.insert(
            "missing-tool".to_string(),
            RequirementStatus::Missing {
                install_template: None,
                install_hint: Some("brew install missing-tool".to_string()),
            },
        );

        // Interactive → should warn but proceed
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
                Some(&mut checker),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        assert!(ui.has_warning("Missing requirement"));
        assert!(ui.has_warning("brew install missing-tool"));
    }

    #[test]
    fn run_with_ui_no_gaps_when_requires_empty() {
        let temp = TempDir::new().unwrap();

        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [hello]
        "#,
        )
        .unwrap();

        // Step with empty requires — no gap checking needed
        let step = make_step("hello", "echo hello", vec![]);

        let mut steps = HashMap::new();
        steps.insert("hello".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions::default();
        let ctx = InterpolationContext::new();

        let (probe, registry) = make_gap_checker();
        let mut checker = GapChecker::new(&registry, &probe, temp.path());

        let mut ui = MockUI::new();

        let result = runner
            .run_with_ui(
                &options,
                &ctx,
                &HashMap::new(),
                temp.path(),
                false,
                &HashMap::new(),
                Some(&mut checker),
                &mut ui,
            )
            .unwrap();

        assert!(result.success);
        // No warnings should appear
        assert!(ui.warnings().is_empty());
    }

    #[test]
    fn run_filters_only_environments() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [always, ci_only]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "always".to_string(),
            make_step("always", "echo always", vec![]),
        );
        let mut ci_step = make_step("ci_only", "echo ci", vec![]);
        ci_step.only_environments = vec!["ci".to_string()];
        steps.insert("ci_only".to_string(), ci_step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions {
            active_environment: Some("development".to_string()),
            ..Default::default()
        };
        let ctx = crate::config::InterpolationContext::new();
        let global_env = HashMap::new();
        let temp = TempDir::new().unwrap();

        let result = runner
            .run(&options, &ctx, &global_env, temp.path())
            .unwrap();

        // ci_only should be skipped in development
        assert_eq!(result.steps.len(), 1);
        assert!(result.skipped.contains(&"ci_only".to_string()));
    }

    #[test]
    fn run_includes_matching_only_environments() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [always, ci_only]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "always".to_string(),
            make_step("always", "echo always", vec![]),
        );
        let mut ci_step = make_step("ci_only", "echo ci", vec![]);
        ci_step.only_environments = vec!["ci".to_string()];
        steps.insert("ci_only".to_string(), ci_step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions {
            active_environment: Some("ci".to_string()),
            ..Default::default()
        };
        let ctx = crate::config::InterpolationContext::new();
        let global_env = HashMap::new();
        let temp = TempDir::new().unwrap();

        let result = runner
            .run(&options, &ctx, &global_env, temp.path())
            .unwrap();

        // ci_only should run in ci environment
        assert_eq!(result.steps.len(), 2);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn run_empty_only_environments_runs_always() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [always]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        // Empty only_environments = run in all environments
        let step = make_step("always", "echo always", vec![]);
        assert!(step.only_environments.is_empty());
        steps.insert("always".to_string(), step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions {
            active_environment: Some("any_env".to_string()),
            ..Default::default()
        };
        let ctx = crate::config::InterpolationContext::new();
        let global_env = HashMap::new();
        let temp = TempDir::new().unwrap();

        let result = runner
            .run(&options, &ctx, &global_env, temp.path())
            .unwrap();

        assert_eq!(result.steps.len(), 1);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn run_only_environments_skipped_steps_in_result() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [a, b, c]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert("a".to_string(), make_step("a", "echo a", vec![]));

        let mut b = make_step("b", "echo b", vec![]);
        b.only_environments = vec!["ci".to_string()];
        steps.insert("b".to_string(), b);

        let mut c = make_step("c", "echo c", vec![]);
        c.only_environments = vec!["staging".to_string()];
        steps.insert("c".to_string(), c);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions {
            active_environment: Some("development".to_string()),
            ..Default::default()
        };
        let ctx = crate::config::InterpolationContext::new();
        let global_env = HashMap::new();
        let temp = TempDir::new().unwrap();

        let result = runner
            .run(&options, &ctx, &global_env, temp.path())
            .unwrap();

        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.skipped.len(), 2);
        assert!(result.skipped.contains(&"b".to_string()));
        assert!(result.skipped.contains(&"c".to_string()));
    }

    #[test]
    fn run_with_progress_respects_only_environments() {
        let config: BivvyConfig = serde_yaml::from_str(
            r#"
            workflows:
              default:
                steps: [always, ci_only]
        "#,
        )
        .unwrap();

        let mut steps = HashMap::new();
        steps.insert(
            "always".to_string(),
            make_step("always", "echo always", vec![]),
        );
        let mut ci_step = make_step("ci_only", "echo ci", vec![]);
        ci_step.only_environments = vec!["ci".to_string()];
        steps.insert("ci_only".to_string(), ci_step);

        let runner = WorkflowRunner::new(&config, steps);
        let options = RunOptions {
            active_environment: Some("development".to_string()),
            ..Default::default()
        };
        let ctx = crate::config::InterpolationContext::new();
        let global_env = HashMap::new();
        let temp = TempDir::new().unwrap();

        let mut skipped_names = Vec::new();
        let result = runner
            .run_with_progress(&options, &ctx, &global_env, temp.path(), None, |progress| {
                if let RunProgress::StepSkipped { name } = progress {
                    skipped_names.push(name.to_string());
                }
            })
            .unwrap();

        assert_eq!(result.steps.len(), 1);
        assert!(skipped_names.contains(&"ci_only".to_string()));
    }
}
