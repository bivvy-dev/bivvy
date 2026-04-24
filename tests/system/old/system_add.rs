//! System tests for `bivvy add` — all interactive, PTY-based.
//!
//! These tests exercise the `bivvy add` command end-to-end through a PTY,
//! verifying both the user-facing output (success messages, errors, exit
//! codes) and the side effects on `.bivvy/config.yml`.  Structured output
//! (the full resulting YAML) is verified via `insta` snapshots so
//! formatting regressions are caught, not just the presence of a
//! substring.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;

const BASE_CONFIG: &str = r#"
app_name: "AddTest"
steps:
  existing:
    command: "rustc --version"
workflows:
  default:
    steps: [existing]
"#;

// ---------------------------------------------------------------------------
// Basic add
// ---------------------------------------------------------------------------

#[test]
fn add_template_to_config() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_template_to_config_yaml", config);
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

#[test]
fn add_with_custom_name() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "cargo-build", "--as", "my_build"], temp.path());

    s.expect("Added 'my_build' step using template 'cargo-build'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_with_custom_name_yaml", config);
}

#[test]
fn add_to_named_workflow() {
    let config = r#"
app_name: "AddTest"
steps:
  existing:
    command: "rustc --version"
workflows:
  default:
    steps: [existing]
  ci:
    steps: [existing]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--workflow", "ci"],
        temp.path(),
    );

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'ci' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Snapshot the full resulting config so we can verify:
    //   - the step definition was added
    //   - the ci workflow contains the new step
    //   - the default workflow was NOT modified
    let config_content = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_to_named_workflow_yaml", config_content);
}

#[test]
fn add_after_step() {
    let config = r#"
app_name: "AddTest"
steps:
  first:
    command: "git --version"
  second:
    command: "rustc --version"
workflows:
  default:
    steps: [first, second]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "first"],
        temp.path(),
    );

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config_content = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_after_step_yaml", config_content);
}

#[test]
fn add_after_nonexistent_step_appends_to_end() {
    // Documented behaviour: if `--after <step>` refers to a step that
    // isn't in the workflow, the new step is appended to the end
    // rather than failing.  (See `AddCommand::insert_step_name` —
    // the target is silently ignored when missing.)
    let config = r#"
app_name: "AddTest"
steps:
  first:
    command: "git --version"
  second:
    command: "rustc --version"
workflows:
  default:
    steps: [first, second]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "does-not-exist"],
        temp.path(),
    );

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config_content = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_after_nonexistent_step_yaml", config_content);
}

#[test]
fn add_no_workflow_flag() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install", "--no-workflow"], temp.path());

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_no_workflow_flag_yaml", config);
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn add_unknown_template_fails() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "nonexistent-template-xyz"], temp.path());

    s.expect("Unknown template: nonexistent-template-xyz")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    // Errors bubbled from the dispatcher exit with code 1.
    assert_exit_code(&s, 1);

    // Config should be unchanged — snapshot proves nothing was
    // written for the nonexistent template.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_unknown_template_config_unchanged", config);
}

#[test]
fn add_duplicate_fails() {
    let temp = setup_project(BASE_CONFIG);

    // First add — prime the state non-interactively so the test
    // focuses on the duplicate case.
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Second add should fail with exit code 1.
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());
    s.expect("Step 'bundle-install' already exists in configuration. Use a different name with --as.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 1);

    // Config after the failed second add should be identical to the
    // state left by the first (successful) add.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    insta::assert_snapshot!("add_duplicate_fails_config_state", config);
}

#[test]
fn add_without_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("No configuration found. Run 'bivvy init' first.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    // `AddCommand::execute` returns `CommandResult::failure(2)` when
    // no config is present — the documented "missing config" exit code.
    assert_exit_code(&s, 2);

    // No config file should be created
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Should not create config file on failure"
    );
}

// ---------------------------------------------------------------------------
// --help
// ---------------------------------------------------------------------------

/// `bivvy add --help` shows the expected description, arguments, and
/// options.  Verified via snapshot so flag renames or description
/// changes are caught as regressions.
#[test]
fn add_help() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "--help"], temp.path());
    let text = read_to_eof(&mut s);

    insta::assert_snapshot!("add_help", text);
    assert_exit_code(&s, 0);
}
