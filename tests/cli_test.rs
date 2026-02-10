//! Integration tests for CLI argument parsing.
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
  custom:
    steps: [hello]
"#;

#[test]
fn cli_no_args_runs_default() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Setup complete!"));
    Ok(())
}

#[test]
fn cli_shows_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.arg("--help");
    cmd.assert().success().stdout(predicate::str::contains(
        "Interactive development environment",
    ));
    Ok(())
}

#[test]
fn cli_shows_version() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
    Ok(())
}

#[test]
fn cli_run_with_dry_run() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--dry-run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry-run mode"));
    Ok(())
}

#[test]
fn cli_run_accepts_workflow_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--workflow", "custom", "--dry-run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Setup complete!"));
    Ok(())
}

#[test]
fn cli_run_no_config_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No configuration found"));
    Ok(())
}

#[test]
fn cli_init_creates_config() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created .bivvy/config.yml"));
    assert!(temp.path().join(".bivvy/config.yml").exists());
    Ok(())
}

#[test]
fn cli_init_fails_if_config_exists() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("init");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Configuration already exists"));
    Ok(())
}

#[test]
fn cli_status_shows_steps() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"));
    Ok(())
}

#[test]
fn cli_list_shows_steps_and_workflows() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"))
        .stdout(predicate::str::contains("Workflows:"));
    Ok(())
}

#[test]
fn cli_lint_validates_config() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Configuration is valid!"));
    Ok(())
}

#[test]
fn cli_debug_flag_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--debug", "status"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn cli_invalid_command_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.arg("invalid-command");
    cmd.assert().failure();
    Ok(())
}

#[test]
fn cli_debug_enables_logging() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["--debug", "--help"]);
    cmd.assert().success();
    Ok(())
}
