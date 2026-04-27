//! System tests for `bivvy snapshot` CLI.
//!
//! Tests the snapshot subcommands: capture, list, and delete.
//! Each test uses an isolated project with change checks configured.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const CHANGE_CHECK_CONFIG: &str = r#"
app_name: snapshot-test
steps:
  build:
    command: "cargo --version"
    check:
      type: change
      target: "Cargo.toml"
workflows:
  default:
    steps: [build]
"#;

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_capture_and_list() {
    let temp = setup_project_with_git(CHANGE_CHECK_CONFIG);

    // Write the target file so there's something to hash
    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    // Capture a named snapshot
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(&bin)
        .args(["snapshot", "baseline-v1", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy snapshot");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "snapshot capture should succeed, got: {combined}"
    );

    // List snapshots — should show the one we just captured
    let output = std::process::Command::new(&bin)
        .args(["snapshot", "list", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy snapshot list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("baseline-v1"),
        "snapshot list should show 'baseline-v1', got: {combined}"
    );
    assert!(output.status.success());
}

#[test]
fn snapshot_delete() {
    let temp = setup_project_with_git(CHANGE_CHECK_CONFIG);

    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let bin = assert_cmd::cargo::cargo_bin("bivvy");

    // Capture
    let output = std::process::Command::new(&bin)
        .args(["snapshot", "to-delete", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy snapshot");
    assert!(output.status.success());

    // Delete
    let output = std::process::Command::new(&bin)
        .args(["snapshot", "delete", "to-delete", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy snapshot delete");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        output.status.success(),
        "snapshot delete should succeed, got: {combined}"
    );

    // List should no longer show it
    let output = std::process::Command::new(&bin)
        .args(["snapshot", "list", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy snapshot list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("to-delete"),
        "deleted snapshot should not appear in list, got: {combined}"
    );
}

#[test]
fn snapshot_no_change_checks_shows_message() {
    // Config without any change checks
    let config = r#"
app_name: no-changes
steps:
  hello:
    command: "cargo --version"
    check:
      type: execution
      command: "cargo --version"
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project_with_git(config);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(&bin)
        .args(["snapshot", "test-snap", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy snapshot");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should indicate no change checks found
    assert!(
        combined.contains("No change checks") || combined.contains("no change"),
        "Should indicate no change checks found, got: {combined}"
    );
}
