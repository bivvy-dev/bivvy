//! Comprehensive system tests for `bivvy last`.
//!
//! Tests display of last-run information including timing, workflow
//! details, step results, JSON structure, failed runs, and all flag
//! combinations.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const SIMPLE_CONFIG: &str = r#"
app_name: "LastTest"
steps:
  greet:
    title: "Check git"
    command: "git --version"
    skippable: false
  farewell:
    title: "Check rustc"
    command: "rustc --version"
    skippable: false
workflows:
  default:
    steps: [greet, farewell]
"#;

const MULTI_WORKFLOW_CONFIG: &str = r#"
app_name: "MultiLast"
steps:
  build:
    title: "Build"
    command: "cargo --version"
    skippable: false
  verify:
    title: "Verify"
    command: "rustc --version"
    skippable: false
workflows:
  default:
    steps: [build, verify]
  quick:
    steps: [build]
"#;

const FAILING_CONFIG: &str = r#"
app_name: "FailLast"
steps:
  good:
    title: "Good step"
    command: "git --version"
    skippable: false
  bad:
    title: "Bad step"
    command: "git --no-such-flag-xyz"
    skippable: false
    depends_on: [good]
workflows:
  default:
    steps: [good, bad]
"#;

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Before any run, shows "no runs" message.
#[test]
fn last_no_runs_shows_message() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No runs recorded for this project."),
        "Should indicate no runs recorded, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// After a run, shows workflow and timing info.
#[test]
fn last_after_run_shows_details() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Workflow:"),
        "Should show 'Workflow:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("default"),
        "Should show workflow name 'default', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After a run, shows timing/duration.
#[test]
fn last_shows_timing() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("When:"),
        "Should show 'When:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Duration:"),
        "Should show 'Duration:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After a run, shows per-step results.
#[test]
fn last_shows_step_results() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps:"),
        "Should show 'Steps:' section header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("greet"),
        "Should show 'greet' step name, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("farewell"),
        "Should show 'farewell' step name, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After a run, shows success status.
#[test]
fn last_shows_success_status() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Status:"),
        "Should show 'Status:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Success"),
        "Should show 'Success' status, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After multiple runs, shows the most recent (quick workflow).
#[test]
fn last_shows_most_recent_run() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);

    // Run default workflow
    run_workflow_silently(temp.path());

    // Run quick workflow (most recent)
    run_bivvy_silently(temp.path(), &["run", "--workflow", "quick"]);

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header, got: {}",
        &text[..text.len().min(300)]
    );
    // The most recent run was the quick workflow
    assert!(
        text.contains("quick"),
        "Should show most recent workflow name 'quick', got: {}",
        &text[..text.len().min(500)]
    );
    // Quick workflow only has the build step
    assert!(
        text.contains("build"),
        "Should show 'build' step from quick workflow, got: {}",
        &text[..text.len().min(500)]
    );
    // The prior default-workflow "verify" step is not part of the most
    // recent run and should not appear in the steps listing.
    assert!(
        !text.contains("verify"),
        "Should NOT show 'verify' step from prior run, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After a failed run, shows failure info.
#[test]
fn last_after_failed_run_shows_failure() {
    let temp = setup_project(FAILING_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let run_status = std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    // `bivvy run` on a failing workflow must exit non-zero (documented
    // exit code 1 for workflow/step failure).
    assert_eq!(
        run_status.code(),
        Some(1),
        "bivvy run with failing step should exit with code 1, got {:?}",
        run_status.code()
    );

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header after failure, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Status:"),
        "Should show 'Status:' label after failure, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Failed"),
        "Should show 'Failed' status, got: {}",
        &text[..text.len().min(500)]
    );
    // The failing step 'bad' depends on 'good' — both steps should be
    // recorded in the run and listed in the steps section.
    assert!(
        text.contains("Steps:"),
        "Should show 'Steps:' section, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("good"),
        "Should list 'good' step (the prerequisite that ran), got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("bad"),
        "Should list 'bad' step (the failing step), got: {}",
        &text[..text.len().min(500)]
    );
    // `bivvy last` itself succeeds with exit code 0 even when reporting
    // on a failed run.
    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS
// =====================================================================

/// --json outputs valid JSON with expected fields.
#[test]
fn last_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    // Extract JSON from PTY output (may have leading/trailing whitespace)
    let trimmed = text.trim();
    let json: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|e| panic!("Should output valid JSON, error: {e}, got: {trimmed}"));
    assert_eq!(
        json["workflow"], "default",
        "JSON 'workflow' field should be 'default'"
    );
    assert_eq!(
        json["status"], "Success",
        "JSON 'status' field should be 'Success'"
    );
    assert_exit_code(&s, 0);
}

/// --json output has expected structure (steps_run, status, duration_ms, workflow).
#[test]
fn last_json_structure() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let trimmed = text.trim();
    let json: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|e| panic!("Should output valid JSON, error: {e}, got: {trimmed}"));

    assert!(
        json.get("workflow").is_some(),
        "JSON should have 'workflow' field"
    );
    assert!(
        json.get("status").is_some(),
        "JSON should have 'status' field"
    );
    assert!(
        json.get("duration_ms").is_some(),
        "JSON should have 'duration_ms' field"
    );
    assert!(
        json.get("steps_run").is_some(),
        "JSON should have 'steps_run' field"
    );
    // Verify steps_run contains our step names
    let steps_run = json["steps_run"].as_array().expect("steps_run should be array");
    assert!(
        steps_run.iter().any(|s| s == "greet"),
        "steps_run should contain 'greet'"
    );
    assert!(
        steps_run.iter().any(|s| s == "farewell"),
        "steps_run should contain 'farewell'"
    );
    assert_exit_code(&s, 0);
}

/// --all shows all runs.
#[test]
fn last_all_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run 1 of 1"),
        "Should show 'Run 1 of 1' header with --all, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show workflow name 'default', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --output includes output note for each step.
#[test]
fn last_output_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("(no captured output available in run history)"),
        "Should include captured output note, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --step filters to a specific step.
#[test]
fn last_step_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "greet"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("greet"),
        "Should show 'greet' step info, got: {}",
        &text[..text.len().min(300)]
    );
    // farewell should not appear in step listing when --step=greet is used
    assert!(
        !text.contains("farewell"),
        "Should NOT show 'farewell' step when filtered to 'greet', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --json + --all combined outputs JSON array.
#[test]
fn last_json_all() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    let trimmed = text.trim();
    let json: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|e| panic!("Should output valid JSON array, error: {e}, got: {trimmed}"));
    assert!(json.is_array(), "JSON --all output should be an array");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "Should have exactly 1 run");
    assert_eq!(arr[0]["workflow"], "default");
    assert_exit_code(&s, 0);
}

/// --json + --output combined outputs valid JSON.
#[test]
fn last_json_output() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    let trimmed = text.trim();
    let json: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|e| panic!("Should output valid JSON, error: {e}, got: {trimmed}"));
    assert_eq!(
        json["workflow"], "default",
        "JSON should contain workflow field"
    );
    assert_eq!(
        json["status"], "Success",
        "JSON should contain status field"
    );
    assert_exit_code(&s, 0);
}

/// --step + --output combined shows step details with output note.
#[test]
fn last_step_output() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "greet", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet"),
        "Should show 'greet' step, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("(no captured output available in run history)"),
        "Should show output note for step, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn last_help() {
    let mut s = spawn_bivvy_global(&["last", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Show last run information"),
        "Help should show 'Show last run information' description, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Output as JSON"),
        "Help should document --json flag with 'Output as JSON', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Show all runs"),
        "Help should document --all flag with 'Show all runs', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Show details for specific step"),
        "Help should document --step flag with 'Show details for specific step', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Include command output"),
        "Help should document --output flag with 'Include command output', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file — should still report no runs (not crash).
#[test]
fn last_no_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No runs recorded for this project."),
        "No config should show 'No runs recorded for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --step with unknown step name returns error.
#[test]
fn last_unknown_step() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Step 'ghost' was not part of the last run."),
        "Unknown step should show \"Step 'ghost' was not part of the last run.\", got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 1);
}

/// JSON output with no runs (should handle gracefully, not crash).
#[test]
fn last_json_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No runs recorded for this project."),
        "JSON with no runs should show 'No runs recorded for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --all with multiple runs shows each run with numbered headers.
#[test]
fn last_all_multiple_runs() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);

    // Run default workflow
    run_workflow_silently(temp.path());

    // Run quick workflow
    run_bivvy_silently(temp.path(), &["run", "--workflow", "quick"]);

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run 1 of 2"),
        "Should show 'Run 1 of 2' header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Run 2 of 2"),
        "Should show 'Run 2 of 2' header, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --json + --all with multiple runs returns JSON array with correct count.
#[test]
fn last_json_all_multiple_runs() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);

    run_workflow_silently(temp.path());
    run_bivvy_silently(temp.path(), &["run", "--workflow", "quick"]);

    let mut s = spawn_bivvy(&["last", "--json", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    let trimmed = text.trim();
    let json: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|e| panic!("Should output valid JSON array, error: {e}, got: {trimmed}"));
    let arr = json.as_array().expect("Should be a JSON array");
    assert_eq!(arr.len(), 2, "Should have 2 runs in history");
    assert_eq!(arr[1]["workflow"], "quick", "Most recent run should be 'quick'");
    assert_exit_code(&s, 0);
}
