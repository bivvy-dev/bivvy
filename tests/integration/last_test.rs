//! Integration tests for the `bivvy last` command.
// The cargo_bin function is marked deprecated in favor of cargo_bin! macro,
// but both work correctly. Suppressing until assert_cmd stabilizes the new API.
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

const SIMPLE_CONFIG: &str = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#;

const MULTI_STEP_CONFIG: &str = r#"
app_name: MultiStep
steps:
  step_one:
    command: echo one
  step_two:
    command: echo two
  step_three:
    command: echo three
workflows:
  default:
    steps: [step_one, step_two, step_three]
"#;

// --- No previous run ---

#[test]
fn last_no_previous_run_shows_message() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No runs recorded"));
    Ok(())
}

// --- Show last run information after a successful run ---

#[test]
fn last_after_run_shows_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    // First, do a run
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    // Then check last
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Last Run"))
        .stdout(predicate::str::contains("Workflow:"))
        .stdout(predicate::str::contains("default"));
    Ok(())
}

#[test]
fn last_after_run_shows_status_success() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Status:"))
        .stdout(predicate::str::contains("Success"));
    Ok(())
}

#[test]
fn last_after_run_shows_when_and_duration() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("When:"))
        .stdout(predicate::str::contains("Duration:"));
    Ok(())
}

#[test]
fn last_after_run_shows_steps() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"))
        .stdout(predicate::str::contains("step_one"))
        .stdout(predicate::str::contains("step_two"))
        .stdout(predicate::str::contains("step_three"));
    Ok(())
}

// --- Failed run ---

#[test]
fn last_after_failed_run_shows_failure() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: FailTest
steps:
  bad_step:
    command: "false"
workflows:
  default:
    steps: [bad_step]
"#;
    let temp = setup_project(config);

    // Run should fail
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .failure();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Failed"));
    Ok(())
}

// --- Last run reflects most recent run ---

#[test]
fn last_reflects_most_recent_run() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: TwoWorkflows
steps:
  hello:
    command: echo hello
  world:
    command: echo world
workflows:
  default:
    steps: [hello]
  custom:
    steps: [world]
"#;
    let temp = setup_project(config);

    // First run with default workflow
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    // Second run with custom workflow
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .args(["run", "--workflow", "custom"])
        .assert()
        .success();

    // Last should show the custom workflow (most recent)
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("custom"));
    Ok(())
}

// --- No config at all ---

#[test]
fn last_no_config_shows_no_runs() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("last");
    // Should succeed but show no runs (no state for this project)
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No runs recorded"));
    Ok(())
}

// --- Help flag ---

#[test]
fn last_help_shows_usage() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["last", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("last run"));
    Ok(())
}

// --- Flags are accepted ---

#[test]
fn last_accepts_json_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["last", "--json"]);
    // Should not error on the flag even if no runs exist
    cmd.assert().success();
    Ok(())
}

#[test]
fn last_accepts_step_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["last", "--step", "hello"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn last_accepts_all_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["last", "--all"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn last_accepts_output_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["last", "--output"]);
    cmd.assert().success();
    Ok(())
}

// --- Global flags work with last ---

#[test]
fn last_with_debug_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--debug", "last"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn last_with_quiet_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--quiet", "last"]);
    cmd.assert().success();
    Ok(())
}
