//! System tests for `bivvy add` — all interactive, PTY-based.
#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    temp
}

fn spawn_bivvy(args: &[&str], dir: &std::path::Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(Duration::from_secs(15)));
    session
}

const BASE_CONFIG: &str = r#"
app_name: "AddTest"
steps:
  existing:
    command: "echo existing"
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

    s.expect("Added").expect("Should confirm addition");
    s.expect(expectrl::Eof).ok();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("template: bundle-install"));
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

#[test]
fn add_with_custom_name() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "cargo-build", "--as", "my_build"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).ok();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("my_build:"));
    assert!(config.contains("template: cargo-build"));
}

#[test]
fn add_to_named_workflow() {
    let config = r#"
app_name: "AddTest"
steps:
  existing:
    command: "echo existing"
workflows:
  default:
    steps: [existing]
  ci:
    steps: [existing]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["add", "bundle-install", "--workflow", "ci"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn add_after_step() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "existing"],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn add_no_workflow_flag() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install", "--no-workflow"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).ok();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("template: bundle-install"));
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn add_unknown_template_fails() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "nonexistent-template-xyz"], temp.path());

    s.expect("not found").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn add_duplicate_fails() {
    let temp = setup_project(BASE_CONFIG);

    // First add
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());
    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).ok();

    // Second add should fail
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());
    s.expect("already exists").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn add_without_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("bivvy init").ok();
    s.expect(expectrl::Eof).ok();
}
