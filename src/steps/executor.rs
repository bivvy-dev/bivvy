//! Step execution engine.
//!
//! Executes resolved steps with proper environment handling,
//! interpolation, and hook execution.

use crate::config::interpolation::{resolve_string, InterpolationContext};
use crate::error::{BivvyError, Result};
use crate::shell::{execute, execute_streaming, CommandOptions, OutputCallback};
use crate::steps::completed_check::{run_check, CheckResult};
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
    /// Create a skipped result.
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

    /// Generate a summary line for display.
    pub fn summary_line(&self) -> String {
        let status = self.status();
        let duration_str = format_duration(self.duration);

        match status {
            StepStatus::Completed => {
                format!("{} {} ({})", status.display_char(), self.name, duration_str)
            }
            StepStatus::Skipped => {
                format!("{} {} (already complete)", status.display_char(), self.name)
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

/// Execute a single step.
pub fn execute_step(
    step: &ResolvedStep,
    project_root: &Path,
    context: &InterpolationContext,
    global_env: &HashMap<String, String>,
    options: &ExecutionOptions,
    output_callback: Option<OutputCallback>,
) -> Result<StepResult> {
    // Check if already complete (unless forced)
    if !options.force {
        if let Some(ref check) = step.completed_check {
            let check_result = run_check(check, project_root);
            if check_result.complete {
                return Ok(StepResult::skipped(&step.name, check_result));
            }
        }
    }

    // Dry run mode
    if options.dry_run {
        let command = resolve_string(&step.command, context)?;
        return Ok(StepResult::success(
            &step.name,
            Duration::ZERO,
            None,
            Some(format!("Would run: {}", command)),
        ));
    }

    // Build environment
    let mut env = global_env.clone();
    env.extend(step.env.iter().map(|(k, v)| (k.clone(), v.clone())));

    // Resolve command with interpolation
    let command = resolve_string(&step.command, context)?;

    // Guard against empty commands
    if command.trim().is_empty() {
        return Err(BivvyError::StepExecutionError {
            step: step.name.clone(),
            message: "step has no command to execute (command is empty)".to_string(),
        });
    }

    // Execute before hooks
    for hook in &step.before {
        let hook_cmd = resolve_string(hook, context)?;
        execute_hook(&hook_cmd, project_root, &env)?;
    }

    // Execute main command
    let cmd_options = CommandOptions {
        cwd: Some(project_root.to_path_buf()),
        env: env.clone(),
        capture_stdout: options.capture_output || output_callback.is_none(),
        capture_stderr: options.capture_output || output_callback.is_none(),
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
    for hook in &step.after {
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
    use crate::config::CompletedCheck;
    use std::fs;
    use tempfile::TempDir;

    fn make_step(command: &str) -> ResolvedStep {
        ResolvedStep {
            name: "test".to_string(),
            title: "Test Step".to_string(),
            description: None,
            command: command.to_string(),
            depends_on: vec![],
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
    fn execute_step_runs_command() {
        let temp = TempDir::new().unwrap();
        let step = make_step("echo hello");
        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result =
            execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

        assert!(result.success);
        assert!(!result.skipped);
        assert!(result.output.unwrap().contains("hello"));
    }

    #[test]
    fn execute_step_skips_when_complete() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("done.txt"), "").unwrap();

        let mut step = make_step("echo should not run");
        step.completed_check = Some(CompletedCheck::FileExists {
            path: "done.txt".to_string(),
        });

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result =
            execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

        assert!(result.success);
        assert!(result.skipped);
    }

    #[test]
    fn execute_step_runs_when_forced() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("done.txt"), "").unwrap();

        let mut step = make_step("echo forced");
        step.completed_check = Some(CompletedCheck::FileExists {
            path: "done.txt".to_string(),
        });

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            force: true,
            capture_output: true,
            ..Default::default()
        };

        let result =
            execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

        assert!(result.success);
        assert!(!result.skipped);
        assert!(result.output.unwrap().contains("forced"));
    }

    #[test]
    fn execute_step_dry_run() {
        let temp = TempDir::new().unwrap();
        let step = make_step("rm -rf /");

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            dry_run: true,
            ..Default::default()
        };

        let result =
            execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

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
        step.env
            .insert("STEP_VAR".to_string(), "step_value".to_string());

        let mut global_env = HashMap::new();
        global_env.insert("GLOBAL_VAR".to_string(), "global_value".to_string());

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result = execute_step(&step, temp.path(), &ctx, &global_env, &options, None).unwrap();

        assert!(result.success);
        assert!(result.output.unwrap().contains("step_value"));
    }

    #[test]
    fn execute_step_runs_hooks() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step("echo main");
        step.before = vec!["echo before".to_string()];
        step.after = vec!["echo after".to_string()];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions {
            capture_output: true,
            ..Default::default()
        };

        let result =
            execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn execute_step_fails_on_hook_failure() {
        let temp = TempDir::new().unwrap();

        let mut step = make_step("echo main");
        step.before = vec!["exit 1".to_string()];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None);
        assert!(result.is_err());
    }

    #[test]
    fn hooks_execute_in_order() {
        let temp = TempDir::new().unwrap();
        let order_file = temp.path().join("order.txt");

        let mut step = make_step(&format!("echo main >> {}", order_file.display()));
        step.before = vec![
            format!("echo before1 >> {}", order_file.display()),
            format!("echo before2 >> {}", order_file.display()),
        ];
        step.after = vec![format!("echo after1 >> {}", order_file.display())];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

        let content = fs::read_to_string(&order_file).unwrap();
        let lines: Vec<_> = content.lines().map(|l| l.trim()).collect();

        assert_eq!(lines, vec!["before1", "before2", "main", "after1"]);
    }

    #[test]
    fn before_hook_failure_stops_execution() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("ran.txt");

        let mut step = make_step(&format!("touch {}", marker.display()));
        step.before = vec!["exit 1".to_string()];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None);

        assert!(result.is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn after_hooks_skipped_on_command_failure() {
        let temp = TempDir::new().unwrap();
        let marker = temp.path().join("after-ran.txt");

        let mut step = make_step("exit 1");
        step.after = vec![format!("touch {}", marker.display())];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result =
            execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

        assert!(!result.success);
        assert!(!marker.exists());
    }

    #[test]
    #[cfg(unix)]
    fn hooks_receive_step_env() {
        let temp = TempDir::new().unwrap();
        let output_file = temp.path().join("env.txt");

        let mut step = make_step("echo done");
        step.env
            .insert("HOOK_VAR".to_string(), "hook_value".to_string());
        step.before = vec![format!("echo $HOOK_VAR >> {}", output_file.display())];

        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

        let content = fs::read_to_string(&output_file).unwrap();
        assert!(content.contains("hook_value"));
    }

    #[test]
    fn execute_step_rejects_empty_command() {
        let temp = TempDir::new().unwrap();
        let step = make_step("");
        let ctx = InterpolationContext::new();
        let options = ExecutionOptions::default();

        let result = execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None);

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

        let result = execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None);

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

        let check_result = crate::steps::CheckResult::complete("already done");
        let result = StepResult::skipped("test", check_result);
        assert_eq!(result.status(), StepStatus::Skipped);
    }

    #[test]
    fn step_result_summary_line_includes_status() {
        let result = StepResult::success("test", Duration::from_secs(1), Some(0), None);
        let line = result.summary_line();
        assert!(line.contains('✓'));
        assert!(line.contains("test"));
    }

    #[test]
    fn step_result_recovery_detail_defaults_none() {
        let success = StepResult::success("test", Duration::from_secs(1), Some(0), None);
        assert!(success.recovery_detail.is_none());

        let failure =
            StepResult::failure("test", Duration::from_secs(1), "error".to_string(), None);
        assert!(failure.recovery_detail.is_none());

        let check_result = crate::steps::CheckResult::complete("done");
        let skipped = StepResult::skipped("test", check_result);
        assert!(skipped.recovery_detail.is_none());
    }

    #[test]
    fn format_duration_formats_correctly() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.0s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 5s");
    }
}
