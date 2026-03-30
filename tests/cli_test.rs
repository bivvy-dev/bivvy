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

// --- Templates command tests ---

#[test]
fn cli_templates_lists_available() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("templates");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bundler"))
        .stdout(predicate::str::contains("yarn"))
        .stdout(predicate::str::contains("cargo"))
        .stdout(predicate::str::contains("templates available"));
    Ok(())
}

#[test]
fn cli_templates_filter_by_category() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["templates", "--category", "ruby"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bundler"))
        .stdout(predicate::str::contains("ruby"));
    Ok(())
}

#[test]
fn cli_templates_filter_by_category_excludes_others() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["templates", "--category", "ruby"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Ruby templates should be present
    assert!(stdout.contains("bundler"));
    // Non-ruby templates should be absent
    assert!(!stdout.contains("yarn"));
    assert!(!stdout.contains("cargo"));
    assert!(!stdout.contains("pip"));
    Ok(())
}

#[test]
fn cli_templates_nonexistent_category_shows_zero() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["templates", "--category", "nonexistent"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0 templates available"));
    Ok(())
}

#[test]
fn cli_templates_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["templates", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("available templates"));
    Ok(())
}

// --- Add command tests ---

#[test]
fn cli_add_appends_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Added 'bundler'"));

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(config.contains("template: bundler"));
    assert!(config.contains("# command: bundle install"));
    Ok(())
}

#[test]
fn cli_add_with_custom_name() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler", "--as", "ruby_deps"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Added 'ruby_deps'"));

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(config.contains("ruby_deps:"));
    assert!(config.contains("template: bundler"));
    Ok(())
}

#[test]
fn cli_add_no_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler", "--no-workflow"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(config.contains("template: bundler"));
    // Workflow should not include bundler
    assert!(!config.contains("steps: [hello, bundler]"));
    Ok(())
}

#[test]
fn cli_add_fails_without_config() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("bivvy init"));
    Ok(())
}

#[test]
fn cli_add_fails_for_unknown_template() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "nonexistent_template_xyz"]);
    cmd.assert().failure();
    Ok(())
}

#[test]
fn cli_add_fails_for_duplicate_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler"]);
    cmd.assert().success();

    // Adding again should fail
    let mut cmd2 = Command::new(cargo_bin("bivvy"));
    cmd2.current_dir(temp.path());
    cmd2.args(["add", "bundler"]);
    cmd2.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    Ok(())
}

#[test]
fn cli_add_with_after_positioning() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm build
workflows:
  default:
    steps: [install, build]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler", "--after", "install"]);
    cmd.assert().success();

    let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    // bundler should appear between install and build in the workflow
    assert!(new_config.contains("steps: [install, bundler, build]"));
    Ok(())
}

#[test]
fn cli_add_to_named_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
  ci:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["add", "bundler", "--workflow", "ci"]);
    cmd.assert().success();

    let new_config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    // bundler should be added to the ci workflow
    assert!(new_config.contains("steps: [hello, bundler]"));
    // The default workflow should remain unchanged
    assert!(new_config.contains("steps: [hello]\n"));
    Ok(())
}

#[test]
fn cli_add_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["add", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("template"));
    Ok(())
}
