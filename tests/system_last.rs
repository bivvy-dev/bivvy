//! System tests for `bivvy last` — all interactive, PTY-based.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const MULTI_STEP_CONFIG: &str = r#"
app_name: "LastTest"
steps:
  check_git:
    title: "Check git"
    command: "git --version"
    skippable: false
  check_rustc:
    title: "Check rustc"
    command: "rustc --version"
    skippable: false
    depends_on: [check_git]
workflows:
  default:
    steps: [check_git, check_rustc]
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

/// Before any run, `bivvy last` shows "No runs recorded for this project."
#[test]
fn last_no_runs_shows_message() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No runs recorded for this project."),
        "Should show full no-runs message, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After a successful run, `bivvy last` shows the header, workflow name,
/// and per-step results.
#[test]
fn last_after_run_shows_details() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Workflow:") && text.contains("default"),
        "Should show 'Workflow: default', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Duration:"),
        "Should show 'Duration:' field, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Status:") && text.contains("Success"),
        "Should show 'Status: ... Success', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Steps:"),
        "Should show 'Steps:' section, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("check_git") && text.contains("check_rustc"),
        "Should show both step names, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS
// =====================================================================

/// `bivvy last --json` outputs valid JSON with expected keys.
#[test]
fn last_json_outputs_valid_json_with_expected_keys() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);

    // Extract JSON from the PTY output (may have surrounding whitespace/noise)
    let json_start = text.find('{').expect("JSON output should contain '{'");
    let json_end = text.rfind('}').expect("JSON output should contain '}'");
    let json_str = &text[json_start..=json_end];

    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("Should output valid JSON");
    assert_eq!(
        parsed["workflow"], "default",
        "JSON 'workflow' should be 'default'"
    );
    assert_eq!(
        parsed["status"], "Success",
        "JSON 'status' should be 'Success'"
    );
    assert!(
        parsed["duration_ms"].is_number(),
        "JSON should contain numeric 'duration_ms'"
    );
    assert!(
        parsed["steps_run"].is_array(),
        "JSON should contain 'steps_run' array"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last --all` shows "Run N of M" headers when there is at least one run.
#[test]
fn last_all_shows_run_headers() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run 1 of 1"),
        "Should show 'Run 1 of 1' header with --all, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Workflow:") && text.contains("default"),
        "Should show workflow info with --all, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last --output` includes the output note for each step.
#[test]
fn last_output_shows_capture_note() {
    let temp = setup_project(MULTI_STEP_CONFIG);
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
        "Should show full output capture note with --output, got: {}",
        &text[..text.len().min(500)]
    );
    // The note should appear for every step in the run
    let note_count = text
        .matches("(no captured output available in run history)")
        .count();
    assert_eq!(
        note_count, 2,
        "Should show capture note once per step (2 steps), got {note_count} in: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last --step check_git` filters to only that step.
#[test]
fn last_step_filter_shows_matching_step() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "check_git"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("check_git"),
        "Should show filtered step 'check_git', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Steps:"),
        "Should show Steps section, got: {}",
        &text[..text.len().min(500)]
    );
    // The filtered view should not list check_rustc in the Steps section
    let after_steps = text.split("Steps:").nth(1).unwrap_or("");
    assert!(
        !after_steps.contains("check_rustc"),
        "Filtered --step check_git should not show check_rustc in Steps section, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last --json --all` returns a JSON array of all runs.
#[test]
fn last_json_all_returns_array() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json", "--all"], temp.path());

    let text = read_to_eof(&mut s);

    let json_start = text.find('[').expect("JSON --all output should contain '['");
    let json_end = text.rfind(']').expect("JSON --all output should contain ']'");
    let json_str = &text[json_start..=json_end];

    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("Should output valid JSON array");
    assert!(parsed.is_array(), "JSON --all should be an array");
    assert_eq!(
        parsed.as_array().unwrap().len(),
        1,
        "Should have exactly 1 run"
    );
    assert_eq!(parsed[0]["workflow"], "default");
    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// After a failed run, `bivvy last` shows failure status and error info.
#[test]
fn last_after_failed_run_shows_failure_status() {
    let temp = setup_project(FAILING_CONFIG);
    // Run will fail because of the bad step; don't assert success
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header after failed run, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Status:") && text.contains("Failed"),
        "Should show 'Status: ... Failed', got: {}",
        &text[..text.len().min(500)]
    );
    // Per docs: a failed run displays the Error line with the recorded message.
    assert!(
        text.contains("Error: One or more steps failed"),
        "Should show the recorded failure error message, got: {}",
        &text[..text.len().min(500)]
    );
    // Both steps should still be listed, with the failing one last.
    assert!(
        text.contains("good") && text.contains("bad"),
        "Should list both 'good' and 'bad' steps, got: {}",
        &text[..text.len().min(500)]
    );
    // `bivvy last` itself succeeds (exit 0) even when reporting a failed run.
    assert_exit_code(&s, 0);
}

/// `bivvy last --step ghost` for a step not in the last run shows an error.
#[test]
fn last_unknown_step_shows_error() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Step 'ghost' was not part of the last run."),
        "Should show full error for unknown step 'ghost', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 1);
}

/// `bivvy last --json` with no prior runs outputs "No runs recorded for this project."
/// (not a crash or invalid JSON).
#[test]
fn last_json_no_runs_shows_message() {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No runs recorded for this project."),
        "JSON with no runs should show full no-runs message, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}
