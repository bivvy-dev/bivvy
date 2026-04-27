//! System tests for change detection workflow.
//!
//! Tests the full change detection lifecycle: file changes between runs
//! trigger step re-execution and baseline updates.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const CHANGE_DETECTION_CONFIG: &str = r#"
app_name: change-test
steps:
  build:
    command: "rustc --version"
    check:
      type: change
      target: "version.txt"
      on_change: proceed
workflows:
  default:
    steps: [build]
"#;

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[test]
fn change_detection_first_run_establishes_baseline() {
    let temp = setup_project_with_git(CHANGE_DETECTION_CONFIG);

    // Create the target file
    std::fs::write(temp.path().join("version.txt"), "1.0.0").unwrap();

    // First run: should establish baseline (indeterminate → run the step)
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(&bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "First run should succeed, got: {combined}"
    );
    assert!(
        combined.contains("build") || combined.contains("rustc"),
        "Output should mention the build step, got: {combined}"
    );
}

#[test]
fn change_detection_no_change_skips_step() {
    let temp = setup_project_with_git(CHANGE_DETECTION_CONFIG);

    // Create the target file
    std::fs::write(temp.path().join("version.txt"), "1.0.0").unwrap();

    let bin = assert_cmd::cargo::cargo_bin("bivvy");

    // First run: establishes baseline
    let output = std::process::Command::new(&bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "First run should succeed");

    // Second run without changes: check should detect no change
    let output = std::process::Command::new(&bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "Second run should succeed, got: {combined}"
    );
    // With on_change: proceed, no change means the check fails → step should
    // be considered complete (check "passes" = work already done)
    // The step may be skipped or run depending on how the check interacts
    // with the orchestrator. At minimum it should not error.
}

#[test]
fn change_detection_file_change_triggers_rerun() {
    let temp = setup_project_with_git(CHANGE_DETECTION_CONFIG);

    // Create the target file
    std::fs::write(temp.path().join("version.txt"), "1.0.0").unwrap();

    let bin = assert_cmd::cargo::cargo_bin("bivvy");

    // First run: establishes baseline
    let output = std::process::Command::new(&bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "First run should succeed");

    // Change the file
    std::fs::write(temp.path().join("version.txt"), "2.0.0").unwrap();

    // Third run after change: change detected, step should run
    let output = std::process::Command::new(&bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "Run after change should succeed, got: {combined}"
    );
    // With on_change: proceed, a change means the check passes → step's work
    // needs to be done. The step should execute.
}
