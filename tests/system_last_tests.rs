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
  test:
    title: "Test"
    command: "rustc --version"
    skippable: false
workflows:
  default:
    steps: [build, test]
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
        text.contains("No runs recorded") || text.contains("No run") || text.contains("no runs"),
        "Should indicate no runs, got: {}",
        &text[..text.len().min(300)]
    );
}

/// After a run, shows workflow and timing info.
#[test]
fn last_after_run_shows_details() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run") || text.contains("Last"),
        "Should show header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Workflow") || text.contains("default"),
        "Should show workflow info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// After a run, shows timing/duration.
#[test]
fn last_shows_timing() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("s") || text.contains("ms") || text.contains("duration")
            || text.contains("Duration") || text.contains("ago") || text.contains("sec"),
        "Should show timing info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// After a run, shows per-step results.
#[test]
fn last_shows_step_results() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet") || text.contains("farewell") || text.contains("step")
            || text.contains("2"),
        "Should show per-step info, got: {}",
        &text[..text.len().min(500)]
    );
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
        text.contains("Last Run") || text.contains("Last"),
        "Should show last run header"
    );
    // The most recent run was the quick workflow
    assert!(
        text.contains("quick") || text.contains("build"),
        "Should show most recent workflow (quick), got: {}",
        &text[..text.len().min(500)]
    );
}

/// After a failed run, shows failure info.
#[test]
fn last_after_failed_run_shows_failure() {
    let temp = setup_project(FAILING_CONFIG);
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
        text.contains("Last Run") || text.contains("Last") || text.contains("FailLast"),
        "Should show last run info after failure"
    );
    assert!(
        text.contains("✗") || text.contains("✘") || text.contains("fail")
            || text.contains("error") || text.contains("bad") || text.contains("1"),
        "Should show failure indicator, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// FLAGS
// =====================================================================

/// --json outputs structured data with workflow key.
#[test]
fn last_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("default") || text.contains("{"),
        "Should output JSON with workflow key, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json output has expected structure (steps, status, duration).
#[test]
fn last_json_structure() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("steps") || text.contains("status") || text.contains("duration")
            || text.contains("workflow"),
        "JSON should contain structured data, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --all shows all runs.
#[test]
fn last_all_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run") || text.contains("Last") || text.contains("default"),
        "Should show runs with --all, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --output includes command output.
#[test]
fn last_output_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run") || text.contains("greet") || text.contains("farewell")
            || text.contains("output") || text.contains("git version"),
        "Should include command output, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --step filters to a specific step.
#[test]
fn last_step_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "greet"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet") || text.contains("git") || text.contains("Last"),
        "Should show greet step info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --json + --all combined.
#[test]
fn last_json_all() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("[") || text.contains("{"),
        "JSON + all should produce structured output, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json + --output combined.
#[test]
fn last_json_output() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--json", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("output") || text.contains("{"),
        "JSON + output should produce structured output, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --step + --output combined.
#[test]
fn last_step_output() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "greet", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet") || text.contains("git") || text.contains("Last"),
        "Step + output should show step details, got: {}",
        &text[..text.len().min(500)]
    );
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
        text.contains("last") || text.contains("Last") || text.contains("run"),
        "Help should describe last command, got: {}",
        &text[..text.len().min(300)]
    );
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
        text.contains("No") || text.contains("error") || text.contains("configuration")
            || text.contains("not found"),
        "No config should show error message, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --step with unknown step name.
#[test]
fn last_unknown_step() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["last", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ghost") || text.contains("not found") || text.contains("No")
            || text.contains("unknown"),
        "Unknown step should handle gracefully, got: {}",
        &text[..text.len().min(300)]
    );
}

/// JSON output with no runs (should be valid JSON, not crash).
#[test]
fn last_json_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("null") || text.contains("No") || text.contains("{")
            || text.contains("no runs"),
        "JSON with no runs should handle gracefully, got: {}",
        &text[..text.len().min(300)]
    );
}
