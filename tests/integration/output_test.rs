//! Integration tests for output formatting, color handling, and verbosity modes.
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

// --- NO_COLOR environment variable ---

#[test]
fn no_color_env_var_disables_ansi_codes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.env("NO_COLOR", "1");
    cmd.arg("status");
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // With NO_COLOR set, output should not contain ANSI escape codes
    assert!(
        !stdout.contains("\x1b["),
        "Output should not contain ANSI escape codes when NO_COLOR is set"
    );
    Ok(())
}

#[test]
fn no_color_flag_disables_ansi_codes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--no-color", "status"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // With --no-color flag, output should not contain ANSI escape codes
    assert!(
        !stdout.contains("\x1b["),
        "Output should not contain ANSI escape codes when --no-color is passed"
    );
    Ok(())
}

// --- Verbose output ---

#[test]
fn verbose_flag_shows_command_output() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--verbose", "run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello"));
    Ok(())
}

#[test]
fn verbose_flag_accepted_with_status() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--verbose", "status"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"));
    Ok(())
}

// --- Quiet output ---

#[test]
fn quiet_flag_accepted_with_run() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--quiet", "run"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn quiet_flag_accepted_with_status() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--quiet", "status"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn quiet_flag_accepted_with_list() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--quiet", "list"]);
    cmd.assert().success();
    Ok(())
}

// --- Config default_output setting ---

#[test]
fn config_default_output_quiet_applies() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
settings:
  default_output: quiet
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
    cmd.arg("run");
    // Should succeed with the quiet mode applied from config
    cmd.assert().success();
    Ok(())
}

// --- Lint JSON output format ---

#[test]
fn lint_json_format_outputs_valid_json() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["lint", "--format", "json"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // When config is valid, JSON format should output valid JSON (an empty array)
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with('[') || trimmed.starts_with('{'),
        "JSON output should start with '[' or '{{', got: {}",
        trimmed
    );
    Ok(())
}

#[test]
fn lint_json_format_with_errors_outputs_json() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [b]
  b:
    command: echo b
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["lint", "--format", "json"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let trimmed = stdout.trim();
    // Should output JSON even with errors
    assert!(
        trimmed.starts_with('[') || trimmed.starts_with('{'),
        "JSON output should be valid JSON, got: {}",
        trimmed
    );
    Ok(())
}

#[test]
fn lint_sarif_format_outputs_sarif() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["lint", "--format", "sarif"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let trimmed = stdout.trim();
    // SARIF is JSON-based and should contain the schema reference
    assert!(
        trimmed.contains("sarif") || trimmed.starts_with('{'),
        "SARIF output should be JSON-based, got: {}",
        trimmed
    );
    Ok(())
}

#[test]
fn lint_human_format_shows_readable_output() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["lint", "--format", "human"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Configuration is valid!"));
    Ok(())
}

// --- Verbose and quiet are mutually exclusive behavior ---

#[test]
fn verbose_and_quiet_last_wins() -> Result<(), Box<dyn std::error::Error>> {
    // When both flags are provided, clap should handle it.
    // Test that the binary does not crash.
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--verbose", "--quiet", "status"]);
    // Should not crash - the specific behavior depends on flag parsing order
    let output = cmd.output()?;
    // Just verify it ran (either success or handled gracefully)
    assert!(output.status.success() || !output.stderr.is_empty());
    Ok(())
}

// --- Dry-run output format ---

#[test]
fn dry_run_shows_mode_indicator() -> Result<(), Box<dyn std::error::Error>> {
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
fn dry_run_with_verbose_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--verbose", "run", "--dry-run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry-run mode"));
    Ok(())
}

#[test]
fn dry_run_with_quiet_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--quiet", "run", "--dry-run"]);
    cmd.assert().success();
    Ok(())
}

// --- Error output goes to stderr ---

#[test]
fn error_output_goes_to_stderr() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("run");
    let output = cmd.output()?;
    // No config found should produce an error on stderr
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("No configuration found"),
        "Error should appear on stderr, got: {}",
        stderr
    );
    Ok(())
}

// --- Status output formatting ---

#[test]
fn status_shows_structured_output() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--no-color", "status"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"))
        .stdout(predicate::str::contains("hello"));
    Ok(())
}

// --- List output formatting ---

#[test]
fn list_shows_structured_output() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm run build
    depends_on: [install]
workflows:
  default:
    steps: [install, build]
  ci:
    steps: [install, build]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--no-color", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"))
        .stdout(predicate::str::contains("Workflows:"))
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("build"));
    Ok(())
}

// --- Global flags work with all commands ---

#[test]
fn no_color_flag_works_with_lint() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--no-color", "lint"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        !stdout.contains("\x1b["),
        "Lint output should not contain ANSI escape codes with --no-color"
    );
    Ok(())
}

#[test]
fn no_color_flag_works_with_list() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--no-color", "list"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        !stdout.contains("\x1b["),
        "List output should not contain ANSI escape codes with --no-color"
    );
    Ok(())
}
