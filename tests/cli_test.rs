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

// --- Environment integration tests ---

const ENV_CONFIG: &str = r#"
app_name: EnvTest
settings:
  environments:
    staging:
      detect:
        - env: BIVVY_TEST_STAGING
          value: "1"
      default_workflow: staging
      provided_requirements:
        - postgres-server
steps:
  always:
    command: echo always
  ci_only:
    command: echo ci-only
    only_environments: [ci]
  dev_only:
    command: echo dev-only
    only_environments: [development]
  with_env_override:
    command: echo base
    environments:
      staging:
        command: echo staging-override
workflows:
  default:
    steps: [always, ci_only, dev_only, with_env_override]
  staging:
    steps: [always, with_env_override]
"#;

#[test]
fn cli_run_env_flag_filters_only_environments() -> Result<(), Box<dyn std::error::Error>> {
    // With --env development, ci_only should be skipped and dev_only should run
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--env", "development"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("always"))
        .stdout(predicate::str::contains("dev_only"));
    Ok(())
}

#[test]
fn cli_run_env_ci_skips_dev_only_steps() -> Result<(), Box<dyn std::error::Error>> {
    // With --env ci, dev_only should be skipped and ci_only should run
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--env", "ci"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("always"))
        .stdout(predicate::str::contains("ci_only"));
    Ok(())
}

#[test]
fn cli_run_env_staging_uses_default_workflow() -> Result<(), Box<dyn std::error::Error>> {
    // With --env staging, the staging default_workflow should be used
    // (staging workflow has: [always, with_env_override])
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--env", "staging"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("staging-override"));
    Ok(())
}

#[test]
fn cli_run_env_explicit_workflow_overrides_default() -> Result<(), Box<dyn std::error::Error>> {
    // --workflow default should override the staging environment's default_workflow
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--env", "staging", "--workflow", "default"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("always"));
    Ok(())
}

#[test]
fn cli_run_env_unknown_warns() -> Result<(), Box<dyn std::error::Error>> {
    // Using an unknown environment should produce a warning
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--env", "nonexistent"]);
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Unknown environment"));
    Ok(())
}

#[test]
fn cli_status_with_env_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["status", "--env", "ci"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Environment:"));
    Ok(())
}

#[test]
fn cli_list_with_env_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["list", "--env", "ci"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"));
    Ok(())
}

#[test]
fn cli_dry_run_with_env_shows_filtered_steps() -> Result<(), Box<dyn std::error::Error>> {
    // In dry-run + --env ci, dev_only should appear as skipped
    let temp = setup_project(ENV_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--dry-run", "--env", "ci"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ci_only"));
    Ok(())
}
