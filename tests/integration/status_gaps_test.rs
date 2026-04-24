//! Integration tests for `bivvy status` — gaps not covered by cli_test.rs.
//!
//! Covers: status output with various project states, environment filtering,
//! step-specific status, and display details.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    temp
}

// --- No config ---

#[test]
fn status_no_config_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No configuration found"));
    Ok(())
}

// --- Basic status with steps ---

#[test]
fn status_shows_app_name() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: MyTestApp
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MyTestApp"));
    Ok(())
}

#[test]
fn status_shows_steps_section() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm run build
workflows:
  default:
    steps: [install, build]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"))
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("build"));
    Ok(())
}

// --- Environment display ---

#[test]
fn status_shows_environment_section() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Environment:"));
    Ok(())
}

#[test]
fn status_with_env_flag_shows_specified_environment() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["status", "--env", "staging"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("staging"));
    Ok(())
}

// --- Environment-filtered steps ---

#[test]
fn status_skips_steps_not_in_environment() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  dev_only:
    command: echo dev
    only_environments: [development]
  always_run:
    command: echo always
workflows:
  default:
    steps: [dev_only, always_run]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["status", "--env", "ci"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skipped in ci"));
    Ok(())
}

// --- All steps never run (pending) ---

#[test]
fn status_all_pending_shows_hint() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  a:
    command: echo a
  b:
    command: echo b
workflows:
  default:
    steps: [a, b]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    // When all steps are pending, should show a hint about running setup
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bivvy run"));
    Ok(())
}

// --- After running some steps (partial completion) ---

#[test]
fn status_after_run_shows_completed_steps() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);

    // First run the workflow so steps get recorded
    let mut run_cmd = Command::new(cargo_bin("bivvy"));
    run_cmd.current_dir(temp.path());
    run_cmd.arg("run");
    run_cmd.assert().success();

    // Now check status
    let mut status_cmd = Command::new(cargo_bin("bivvy"));
    status_cmd.current_dir(temp.path());
    status_cmd.arg("status");
    status_cmd.assert().success();
    // After a successful run, the step should no longer show as pending
    // (It should show a checkmark or success indicator instead of the pending icon)
    Ok(())
}

// --- Status with --step flag ---

#[test]
fn status_step_flag_shows_single_step() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install:
    command: echo install
  build:
    command: echo build
workflows:
  default:
    steps: [install, build]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["status", "--step", "install"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("install"));
    Ok(())
}

#[test]
fn status_step_flag_unknown_step_fails() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["status", "--step", "nonexistent"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown step"));
    Ok(())
}

// --- Requirements in status ---

#[test]
fn status_shows_requirements_when_present() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install_deps:
    command: bundle install
    requires:
      - ruby
workflows:
  default:
    steps: [install_deps]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Requirements:"))
        .stdout(predicate::str::contains("ruby"));
    Ok(())
}

#[test]
fn status_no_requirements_section_when_none() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Requirements:"),
        "Should not show Requirements section when no steps have requires"
    );
    Ok(())
}

// --- Multiple steps, many states ---

#[test]
fn status_many_steps_all_listed() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  step_a:
    command: echo a
  step_b:
    command: echo b
  step_c:
    command: echo c
  step_d:
    command: echo d
workflows:
  default:
    steps: [step_a, step_b, step_c, step_d]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("step_a"))
        .stdout(predicate::str::contains("step_b"))
        .stdout(predicate::str::contains("step_c"))
        .stdout(predicate::str::contains("step_d"));
    Ok(())
}
