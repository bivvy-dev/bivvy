//! Integration tests for the steps public API.

use bivvy::checks::{evaluator::CheckEvaluator, Check, CheckResult, PresenceKind, ValidationMode};
use bivvy::config::{ExecutionConfig, EnvironmentVarsConfig, InterpolationContext, StepConfig};
use bivvy::snapshots::SnapshotStore;
use bivvy::steps::{execute_step, ExecutionOptions, ResolvedStep, StepResult, StepStatus};
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

#[test]
fn public_api_accessible() {
    // Verify all public types are accessible
    let _status: StepStatus = StepStatus::Pending;
    let _check_result = CheckResult::passed("test");
    let _options = ExecutionOptions::default();
}

#[test]
fn full_step_execution_workflow() {
    let temp = TempDir::new().unwrap();

    // 1. Create step config
    let config = StepConfig {
        execution: ExecutionConfig {
            command: Some("echo 'setup complete'".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    // 2. Resolve step
    let step = ResolvedStep::from_config("setup", &config, None);
    assert_eq!(step.name, "setup");
    assert_eq!(step.execution.command, "echo 'setup complete'");

    // 3. Execute step
    let ctx = InterpolationContext::new();
    let options = ExecutionOptions {
        capture_output: true,
        ..Default::default()
    };

    let result = execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

    // 4. Check result
    assert!(result.success);
    assert_eq!(result.status(), StepStatus::Completed);
    assert!(result.output.unwrap().contains("setup complete"));
}

#[test]
fn check_evaluation_workflow() {
    let temp = TempDir::new().unwrap();

    // Create the marker file
    fs::write(temp.path().join("deps.lock"), "").unwrap();

    // Run the check using CheckEvaluator
    let check = Check::Presence {
        name: None,
        target: Some("deps.lock".to_string()),
        kind: Some(PresenceKind::File),
        command: None,
    };
    let ctx = InterpolationContext::new();
    let mut store = SnapshotStore::empty();
    let mut evaluator = CheckEvaluator::new(temp.path(), &ctx, &mut store);
    let result = evaluator.evaluate(&check);

    assert!(result.passed_check());
    assert!(result.description.contains("deps.lock"));
}

#[test]
fn check_evaluation_skips_completed_step() {
    let temp = TempDir::new().unwrap();

    // Create marker file that indicates completion
    fs::write(temp.path().join("installed.marker"), "done").unwrap();

    // Evaluate check directly — in the new architecture the orchestrator
    // does this before calling execute_step
    let check = Check::Presence {
        name: None,
        target: Some("installed.marker".to_string()),
        kind: Some(PresenceKind::File),
        command: None,
    };
    let ctx = InterpolationContext::new();
    let mut store = SnapshotStore::empty();
    let mut evaluator = CheckEvaluator::new(temp.path(), &ctx, &mut store);
    let result = evaluator.evaluate(&check);

    // Check passes — orchestrator would skip execution
    assert!(result.passed_check());
}

#[test]
fn step_result_summary_line_formatting() {
    // Test summary line for different states
    let success = StepResult::success("test", std::time::Duration::from_secs(1), Some(0), None);
    let summary = success.summary_line();
    assert!(summary.contains("✓"));
    assert!(summary.contains("test"));

    let failure = StepResult::failure(
        "broken",
        std::time::Duration::from_secs(5),
        "command not found".to_string(),
        None,
    );
    let summary = failure.summary_line();
    assert!(summary.contains("✗"));
    assert!(summary.contains("command not found"));
}

#[test]
fn dry_run_does_not_execute() {
    let temp = TempDir::new().unwrap();

    let config = StepConfig {
        execution: ExecutionConfig {
            command: Some("touch should_not_exist.txt".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let step = ResolvedStep::from_config("touch", &config, None);

    let ctx = InterpolationContext::new();
    let options = ExecutionOptions {
        dry_run: true,
        ..Default::default()
    };

    let result = execute_step(&step, temp.path(), &ctx, &HashMap::new(), &options, None).unwrap();

    assert!(result.success);
    assert!(result.output.unwrap().contains("Would run"));
    assert!(!temp.path().join("should_not_exist.txt").exists());
}

#[test]
fn environment_variable_handling() {
    let temp = TempDir::new().unwrap();

    let mut step_env = HashMap::new();
    step_env.insert("STEP_VAR".to_string(), "from_step".to_string());

    let config = StepConfig {
        execution: ExecutionConfig {
            command: Some(if cfg!(windows) {
                "echo %STEP_VAR% %GLOBAL_VAR%".to_string()
            } else {
                "echo $STEP_VAR $GLOBAL_VAR".to_string()
            }),
            ..Default::default()
        },
        env_vars: EnvironmentVarsConfig {
            env: step_env,
            ..Default::default()
        },
        ..Default::default()
    };
    let step = ResolvedStep::from_config("env_test", &config, None);

    let mut global_env = HashMap::new();
    global_env.insert("GLOBAL_VAR".to_string(), "from_global".to_string());

    let ctx = InterpolationContext::new();
    let options = ExecutionOptions {
        capture_output: true,
        ..Default::default()
    };

    let result = execute_step(&step, temp.path(), &ctx, &global_env, &options, None).unwrap();

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("from_step"));
    assert!(output.contains("from_global"));
}
