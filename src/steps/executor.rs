//! Step execution engine.
//!
//! Executes resolved steps with proper environment handling,
//! interpolation, and hook execution.

use crate::checks::CheckResult;
use crate::config::interpolation::{resolve_string, InterpolationContext};
use crate::error::{BivvyError, Result};
use crate::shell::{execute, execute_streaming, CommandOptions, OutputCallback};
use crate::steps::resolved::ResolvedStep;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Status of a step in the workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// Step is waiting to run.
    Pending,

    /// Step is currently executing.
    Running,

    /// Step completed successfully.
    Completed,

    /// Step failed.
    Failed,

    /// Step was skipped (already complete or dependency failed).
    Skipped,
}

impl StepStatus {
    /// Check if this is a terminal state (no more changes expected).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped
        )
    }

    /// Get a display character for this status.
    pub fn display_char(&self) -> char {
        match self {
            StepStatus::Pending => '○',
            StepStatus::Running => '◉',
            StepStatus::Completed => '✓',
            StepStatus::Failed => '✗',
            StepStatus::Skipped => '⊘',
        }
    }

    /// Title-case label shown in the run-path step result line.
    ///
    /// This is the single source of truth for what appears next to
    /// `display_char()` on the post-spinner result row, e.g. the
    /// `Completed` in `      ✓ Completed (43ms)`.
    pub fn label(&self) -> &'static str {
        match self {
            StepStatus::Pending => "Pending",
            StepStatus::Running => "Running",
            StepStatus::Completed => "Completed",
            StepStatus::Failed => "Failed",
            StepStatus::Skipped => "Skipped",
        }
    }
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            StepStatus::Pending => "pending",
            StepStatus::Running => "running",
            StepStatus::Completed => "completed",
            StepStatus::Failed => "failed",
            StepStatus::Skipped => "skipped",
        };
        write!(f, "{}", s)
    }
}

/// Result of executing a step.
#[derive(Debug)]
pub struct StepResult {
    /// Step name.
    pub name: String,

    /// Whether the step succeeded.
    pub success: bool,

    /// Execution duration.
    pub duration: Duration,

    /// Exit code (if command was run).
    pub exit_code: Option<i32>,

    /// Whether step was skipped (already complete).
    pub skipped: bool,

    /// Completion check result (if run).
    pub check_result: Option<CheckResult>,

    /// Error message (if failed).
    pub error: Option<String>,

    /// Captured output (if available).
    pub output: Option<String>,

    /// Recovery detail (e.g., "succeeded on retry (attempt 2)", "skipped by user").
    pub recovery_detail: Option<String>,
}

impl StepResult {
    /// Create a skipped result (user actively declined to run this step).
    pub fn skipped(name: &str, check_result: CheckResult) -> Self {
        Self {
            name: name.to_string(),
            success: true,
            duration: Duration::ZERO,
            exit_code: None,
            skipped: true,
            check_result: Some(check_result),
            error: None,
            output: None,
            recovery_detail: None,
        }
    }

    /// Create a check-passed result (check passed, step didn't need to run).
    ///
    /// Unlike `skipped`, this records as a successful completion — dependents
    /// should proceed, and the summary shows ✓ instead of ○.
    pub fn check_passed(name: &str, check_result: CheckResult) -> Self {
        Self {
            name: name.to_string(),
            success: true,
            duration: Duration::ZERO,
            exit_code: None,
            skipped: false,
            check_result: Some(check_result),
            error: None,
            output: None,
            recovery_detail: None,
        }
    }

    /// Create a success result.
    pub fn success(
        name: &str,
        duration: Duration,
        exit_code: Option<i32>,
        output: Option<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            success: true,
            duration,
            exit_code,
            skipped: false,
            check_result: None,
            error: None,
            output,
            recovery_detail: None,
        }
    }

    /// Create a failure result.
    pub fn failure(name: &str, duration: Duration, error: String, output: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            success: false,
            duration,
            exit_code: None,
            skipped: false,
            check_result: None,
            error: Some(error),
            output,
            recovery_detail: None,
        }
    }

    /// Get the status of this result.
    pub fn status(&self) -> StepStatus {
        if self.skipped {
            StepStatus::Skipped
        } else if self.success {
            StepStatus::Completed
        } else {
            StepStatus::Failed
        }
    }

    /// Whether this result represents a step whose check passed without
    /// actually executing the command (the work was already done).
    pub fn was_check_passed(&self) -> bool {
        self.success && !self.skipped && self.check_result.is_some() && self.duration.is_zero()
    }

    /// Whether the step's command actually executed (succeeded or failed),
    /// as opposed to being skipped or short-circuited by a passing check.
    pub fn actually_executed(&self) -> bool {
        !self.skipped && !self.was_check_passed()
    }

    /// Generate a summary line for display.
    pub fn summary_line(&self) -> String {
        let status = self.status();
        let duration_str = format_duration(self.duration);

        match status {
            StepStatus::Completed => {
                format!("{} {} ({})", status.display_char(), self.name, duration_str)
            }
            StepStatus::Skipped => {
                let reason = self
                    .check_result
                    .as_ref()
                    .map(|c| c.description.as_str())
                    .unwrap_or("already complete");
                format!("{} {} ({})", status.display_char(), self.name, reason)
            }
            StepStatus::Failed => {
                let error = self.error.as_deref().unwrap_or("unknown error");
                format!("{} {} - {}", status.display_char(), self.name, error)
            }
            _ => format!("{} {}", status.display_char(), self.name),
        }
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if secs == 0 {
        format!("{}ms", millis)
    } else if secs < 60 {
        format!("{}.{}s", secs, millis / 100)
    } else {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {}s", mins, secs)
    }
}

/// Options for step execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionOptions {
    /// Force run even if completed.
    pub force: bool,

    /// Dry run mode (don't actually execute).
    pub dry_run: bool,

    /// Capture output instead of streaming.
    pub capture_output: bool,

    /// Additional secret patterns to mask.
    pub secret_patterns: Vec<String>,
}

/// Build the merged environment a step will run with.
///
/// Layers are applied lowest → highest:
///
/// 1. `base_env` — pre-merged YAML layers (settings.env_vars + workflow.env)
/// 2. step `env_file` — loaded from disk if `step.env_vars.env_file` is set
/// 3. step `env` — inline `env:` map on the step
/// 4. `process_env` — the parent process environment
///
/// The parent process env wins last so that shell-exported variables
/// (`DATABASE_URL=... bivvy run`) override values declared in YAML.
///
/// # Errors
///
/// Returns an error if `step.env_vars.env_file` is set, the file is missing,
/// and `env_file_optional` is false.
pub fn build_step_env(
    step: &ResolvedStep,
    project_root: &Path,
    base_env: &HashMap<String, String>,
    process_env: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let mut env = base_env.clone();

    if let Some(ref env_file_path) = step.env_vars.env_file {
        let resolved_path = project_root.join(env_file_path);
        if step.env_vars.env_file_optional {
            let file_env = crate::config::load_env_file_optional(&resolved_path);
            env.extend(file_env);
        } else {
            let file_env = crate::config::load_env_file(&resolved_path)?;
            env.extend(file_env);
        }
    }

    env.extend(
        step.env_vars
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );

    env.extend(process_env.iter().map(|(k, v)| (k.clone(), v.clone())));

    Ok(env)
}

/// Execute a single step.
///
/// See [`build_step_env`] for the env-layering precedence.
pub fn execute_step(
    step: &ResolvedStep,
    project_root: &Path,
    context: &InterpolationContext,
    base_env: &HashMap<String, String>,
    process_env: &HashMap<String, String>,
    options: &ExecutionOptions,
    output_callback: Option<OutputCallback>,
) -> Result<StepResult> {
    // Create step-scoped context with template inputs
    let context = &context.with_step_inputs(&step.inputs);

    // Note: Check evaluation and precondition evaluation
    // are handled by the orchestrator BEFORE calling execute_step().
    // The executor only executes — it does not make run/skip decisions.

    let env = build_step_env(step, project_root, base_env, process_env)?;

    // Make the resolved env available for ${VAR} interpolation in
    // command/hook strings.
    let mut ctx_with_env = context.clone();
    ctx_with_env.env = env.clone();
    let context = &ctx_with_env;

    // Dry run mode
    if options.dry_run {
        let display = if step.behavior.sensitive {
            "Would run: [SENSITIVE]".to_string()
        } else {
            let command = resolve_string(&step.execution.command, context)?;
            format!("Would run: {}", command)
        };
        return Ok(StepResult::success(
            &step.name,
            Duration::ZERO,
            None,
            Some(display),
        ));
    }

    // Resolve command with interpolation
    let command = resolve_string(&step.execution.command, context)?;

    // Guard against empty commands
    if command.trim().is_empty() {
        return Err(BivvyError::StepExecutionError {
            step: step.name.clone(),
            message: "step has no command to execute (command is empty)".to_string(),
        });
    }

    // Execute before hooks
    for hook in &step.hooks.before {
        let hook_cmd = resolve_string(hook, context)?;
        execute_hook(&hook_cmd, project_root, &env)?;
    }

    // Execute main command
    let cmd_options = CommandOptions {
        cwd: Some(project_root.to_path_buf()),
        env: env.clone(),
        capture_stdout: options.capture_output || output_callback.is_none(),
        capture_stderr: options.capture_output || output_callback.is_none(),
        stdin_null: true,
        ..Default::default()
    };

    let result = if let Some(callback) = output_callback {
        execute_streaming(&command, &cmd_options, callback)?
    } else {
        execute(&command, &cmd_options)?
    };

    if !result.success {
        return Ok(StepResult::failure(
            &step.name,
            result.duration,
            format!("Command failed with exit code {:?}", result.exit_code),
            Some(result.stderr),
        ));
    }

    // Execute after hooks
    for hook in &step.hooks.after {
        let hook_cmd = resolve_string(hook, context)?;
        execute_hook(&hook_cmd, project_root, &env)?;
    }

    Ok(StepResult::success(
        &step.name,
        result.duration,
        result.exit_code,
        if options.capture_output {
            Some(result.stdout)
        } else {
            None
        },
    ))
}

fn execute_hook(command: &str, cwd: &Path, env: &HashMap<String, String>) -> Result<()> {
    let options = CommandOptions {
        cwd: Some(cwd.to_path_buf()),
        env: env.clone(),
        capture_stdout: true,
        capture_stderr: true,
        ..Default::default()
    };

    let result = execute(command, &options)?;

    if !result.success {
        return Err(BivvyError::StepExecutionError {
            step: "hook".to_string(),
            message: format!("Hook '{}' failed: {}", command, result.stderr),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steps::resolved::{
        ResolvedBehavior, ResolvedEnvironmentVars, ResolvedExecution, ResolvedHooks,
        ResolvedOutput, ResolvedScoping,
    };
    use std::fs;
    use tempfile::TempDir;

    fn make_step(command: &str) -> ResolvedStep {
        ResolvedStep {
            name: "test".to_string(),
            title: "Test Step".to_string(),
            description: None,
            depends_on: vec![],
            requires: vec![],
            inputs: HashMap::new(),
            satisfied_when: vec![],
            execution: ResolvedExecution {
                command: command.to_string(),
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
    fn execute_step_runs_command() {
        let temp = TempDir::new().unwrap();
        let step = make_step("echo hello");
        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(!result.skipped);
        assert!(result.output.unwrap().contains("hello"));
    }

    // Note: Tests for check-based skipping and force behavior
    // have been removed from the executor because check evaluation is now
    // handled by the orchestrator (Phase 5 of the system redesign).
    // See orchestrate.rs and workflow.rs tests for check evaluation coverage.

    #[test]
    fn execute_step_dry_run() {
        let temp = TempDir::new().unwrap();
        let step = make_step("rm -rf /");

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            dry_run: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("Would run"));
    }

    #[test]
    fn execute_step_merges_env() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step(if cfg!(windows) {
            "echo %STEP_VAR%"
        } else {
            "echo $STEP_VAR"
        });
        step.env_vars
            .env
            .insert("STEP_VAR".to_string(), "step_value".to_string());

        let mut process_env = HashMap::new();
        process_env.insert("UNRELATED_VAR".to_string(), "unrelated".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &process_env,
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("step_value"));
    }

    /// Bug fix: shell-exported env vars (`FOO=bar bivvy run`) must override
    /// values declared in `step.env`, matching how Make/npm/Docker handle
    /// command-line env overrides.
    #[test]
    #[cfg(unix)]
    fn process_env_overrides_step_env() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step("echo $DATABASE_URL");
        step.env_vars
            .env
            .insert("DATABASE_URL".to_string(), "from_step".to_string());

        let mut process_env = HashMap::new();
        process_env.insert("DATABASE_URL".to_string(), "from_shell".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &process_env,
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        let output = result.output.unwrap();
        assert!(
            output.contains("from_shell"),
            "expected shell value to win, got: {}",
            output
        );
        assert!(
            !output.contains("from_step"),
            "step value should have been shadowed, got: {}",
            output
        );
    }

    /// Process env should also win over the YAML base env (which represents
    /// global `settings.env_vars` and the active workflow's `env`).
    #[test]
    #[cfg(unix)]
    fn process_env_overrides_base_env() {
        let temp = TempDir::new().unwrap();
        let step = make_step("echo $API_HOST");

        let mut base_env = HashMap::new();
        base_env.insert("API_HOST".to_string(), "from_yaml".to_string());

        let mut process_env = HashMap::new();
        process_env.insert("API_HOST".to_string(), "from_shell".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &base_env,
            &process_env,
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("from_shell"));
    }

    /// When the shell does NOT set a variable, step env should still win
    /// over the YAML base env — step is more specific than global/workflow.
    #[test]
    #[cfg(unix)]
    fn step_env_overrides_base_env_when_shell_silent() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step("echo $LOG_LEVEL");
        step.env_vars
            .env
            .insert("LOG_LEVEL".to_string(), "debug".to_string());

        let mut base_env = HashMap::new();
        base_env.insert("LOG_LEVEL".to_string(), "info".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &base_env,
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("debug"));
    }

    /// Verify the InterpolationContext.env is populated from the merged
    /// runtime env so `${VAR}` in command strings resolves against shell +
    /// step + base env, not an empty map.
    #[test]
    #[cfg(unix)]
    fn interpolation_resolves_against_merged_env() {
        let temp = TempDir::new().unwrap();
        let step = make_step("echo resolved-${SHELL_PROVIDED}");

        let mut process_env = HashMap::new();
        process_env.insert("SHELL_PROVIDED".to_string(), "yes".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &process_env,
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("resolved-yes"));
    }

    #[test]
    fn execute_step_runs_hooks() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step("echo main");
        step.hooks.before = vec!["echo before".to_string()];
        step.hooks.after = vec!["echo after".to_string()];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();
        assert!(result.success);
    }

    #[test]
    fn execute_step_fails_on_hook_failure() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step("echo main");
        step.hooks.before = vec!["exit 1".to_string()];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn hooks_execute_in_order() {
        let temp = TempDir::new().unwrap();
        let order_file = temp.path().join("order.txt");

        let mut step = make_step(&format!("echo main >> {}", order_file.display()));
        step.hooks.before = vec![
            format!("echo before1 >> {}", order_file.display()),
            format!("echo before2 >> {}", order_file.display()),
        ];
        step.hooks.after = vec![format!("echo after1 >> {}", order_file.display())];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        let content = fs::read_to_string(&order_file).unwrap();
        let lines: Vec<_> = content.lines().map(|l| l.trim()).collect();

        assert_eq!(lines, vec!["before1", "before2", "main", "after1"]);
    }

    #[test]
    fn before_hook_failure_stops_execution() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("ran.txt");

        let mut step = make_step(&format!("touch {}", marker.display()));
        step.hooks.before = vec!["exit 1".to_string()];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        );

        assert!(result.is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn after_hooks_skipped_on_command_failure() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("after-ran.txt");

        let mut step = make_step("exit 1");
        step.hooks.after = vec![format!("touch {}", marker.display())];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        assert!(!result.success);
        assert!(!marker.exists());
    }

    #[test]
    #[cfg(unix)]
    fn hooks_receive_step_env() {
        let temp = TempDir::new().unwrap();
        let output_file = temp.path().join("env.txt");

        let mut step = make_step("echo done");
        step.env_vars
            .env
            .insert("HOOK_VAR".to_string(), "hook_value".to_string());
        step.hooks.before = vec![format!("echo $HOOK_VAR >> {}", output_file.display())];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        let content = fs::read_to_string(&output_file).unwrap();
        assert!(content.contains("hook_value"));
    }

    #[test]
    fn execute_step_rejects_empty_command() {
        let temp = TempDir::new().unwrap();
        let step = make_step("");
        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("empty"), "error was: {}", err);
    }

    #[test]
    fn execute_step_rejects_whitespace_only_command() {
        let temp = TempDir::new().unwrap();
        let step = make_step("   ");
        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("empty"), "error was: {}", err);
    }

    #[test]
    fn step_status_is_terminal() {
        assert!(!StepStatus::Pending.is_terminal());
        assert!(!StepStatus::Running.is_terminal());
        assert!(StepStatus::Completed.is_terminal());
        assert!(StepStatus::Failed.is_terminal());
        assert!(StepStatus::Skipped.is_terminal());
    }

    #[test]
    fn step_status_display_char() {
        assert_eq!(StepStatus::Completed.display_char(), '✓');
        assert_eq!(StepStatus::Failed.display_char(), '✗');
        assert_eq!(StepStatus::Skipped.display_char(), '⊘');
    }

    #[test]
    fn step_status_display() {
        assert_eq!(format!("{}", StepStatus::Pending), "pending");
        assert_eq!(format!("{}", StepStatus::Completed), "completed");
    }

    #[test]
    fn step_result_status() {
        let result = StepResult::success("test", Duration::from_secs(1), Some(0), None);
        assert_eq!(result.status(), StepStatus::Completed);

        let result = StepResult::failure("test", Duration::from_secs(1), "error".to_string(), None);
        assert_eq!(result.status(), StepStatus::Failed);

        let check_result = crate::checks::CheckResult::passed("already done");
        let result = StepResult::skipped("test", check_result);
        assert_eq!(result.status(), StepStatus::Skipped);
    }

    #[test]
    fn was_check_passed_distinguishes_check_only_results() {
        let check_result = crate::checks::CheckResult::passed("done");
        assert!(StepResult::check_passed("a", check_result.clone()).was_check_passed());

        // Actually-ran results are not "check passed"
        assert!(
            !StepResult::success("a", Duration::from_secs(1), Some(0), None).was_check_passed()
        );
        assert!(
            !StepResult::failure("a", Duration::from_secs(1), "boom".into(), None)
                .was_check_passed()
        );
        // User-skipped results are not "check passed"
        assert!(!StepResult::skipped("a", check_result).was_check_passed());
    }

    #[test]
    fn actually_executed_only_for_real_runs() {
        let check_result = crate::checks::CheckResult::passed("done");

        // Real executions count
        assert!(
            StepResult::success("a", Duration::from_secs(1), Some(0), None).actually_executed()
        );
        assert!(
            StepResult::failure("a", Duration::from_secs(1), "boom".into(), None)
                .actually_executed()
        );

        // Non-runs do not
        assert!(!StepResult::check_passed("a", check_result.clone()).actually_executed());
        assert!(!StepResult::skipped("a", check_result).actually_executed());
    }

    #[test]
    fn step_result_summary_line_includes_status() {
        let result = StepResult::success("test", Duration::from_secs(1), Some(0), None);
        let line = result.summary_line();
        assert!(line.contains('✓'));
        assert!(line.contains("test"));
    }

    #[test]
    fn step_result_summary_line_skipped_shows_check_description() {
        let check_result = crate::checks::CheckResult::passed("rustc --version succeeded");
        let result = StepResult::skipped("rust", check_result);
        let line = result.summary_line();
        assert!(line.contains("rustc --version"), "got: {}", line);
        assert!(!line.contains("already complete"), "got: {}", line);
    }

    #[test]
    fn step_result_summary_line_skipped_without_check_result() {
        let mut result =
            StepResult::skipped("test", crate::checks::CheckResult::passed("User declined"));
        result.check_result = None;
        let line = result.summary_line();
        assert!(line.contains("already complete"), "got: {}", line);
    }

    #[test]
    fn step_result_recovery_detail_defaults_none() {
        let success = StepResult::success("test", Duration::from_secs(1), Some(0), None);
        assert!(success.recovery_detail.is_none());

        let failure =
            StepResult::failure("test", Duration::from_secs(1), "error".to_string(), None);
        assert!(failure.recovery_detail.is_none());

        let check_result = crate::checks::CheckResult::passed("done");
        let skipped = StepResult::skipped("test", check_result);
        assert!(skipped.recovery_detail.is_none());
    }

    // Note: Precondition tests have been removed from the executor because
    // precondition evaluation is now handled by the orchestrator (Phase 5).
    // See orchestrate.rs and workflow.rs tests for precondition coverage.

    #[test]
    fn format_duration_formats_correctly() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.0s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 5s");
    }

    #[test]
    fn execute_step_resolves_template_inputs() {
        let temp = TempDir::new().unwrap();
        let mut step = make_step("echo ${bump}");
        step.inputs.insert("bump".to_string(), "minor".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("minor"));
    }

    #[test]
    fn execute_step_inputs_available_in_dry_run() {
        let temp = TempDir::new().unwrap();
        let mut step = make_step("cargo set-version --bump ${bump}");
        step.inputs.insert("bump".to_string(), "patch".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            dry_run: true,
            ..Default::default()
        };

        let result = execute_step(
            &step,
            temp.path(),
            &ctx,
            &HashMap::new(),
            &HashMap::new(),
            &options,
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result
            .output
            .unwrap()
            .contains("cargo set-version --bump patch"),);
    }
}
