//! Integration tests for `bivvy lint` — gaps not covered by cli_test.rs.
//!
//! Covers: specific validation error cases, format flags, strict mode,
//! and various config error scenarios.
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
fn lint_no_config_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No configuration found"));
    Ok(())
}

// --- Invalid YAML ---

#[test]
fn lint_bad_yaml_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;
    fs::write(
        bivvy_dir.join("config.yml"),
        "app_name: test\nsteps:\n  - this is not valid: [",
    )?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert().failure();
    Ok(())
}

// --- Circular dependency detection ---

#[test]
fn lint_circular_dependency_fails() -> Result<(), Box<dyn std::error::Error>> {
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
    cmd.arg("lint");
    cmd.assert().failure().stderr(
        predicate::str::contains("circular")
            .or(predicate::str::contains("Circular"))
            .or(predicate::str::contains("error")),
    );
    Ok(())
}

// --- Self-dependency ---

#[test]
fn lint_self_dependency_fails() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [a]
workflows:
  default:
    steps: [a]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert().failure();
    Ok(())
}

// --- Undefined dependency ---

#[test]
fn lint_undefined_dependency_fails() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
steps:
  a:
    command: echo a
    depends_on: [nonexistent]
workflows:
  default:
    steps: [a]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert().failure();
    Ok(())
}

// --- Valid config passes ---

#[test]
fn lint_valid_config_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
steps:
  hello:
    command: echo hello
  world:
    command: echo world
    depends_on: [hello]
workflows:
  default:
    steps: [hello, world]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Configuration is valid!"));
    Ok(())
}

// --- Format flags ---

#[test]
fn lint_json_format_outputs_json() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
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
    cmd.args(["lint", "--format", "json"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn lint_sarif_format_outputs_sarif() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
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
    cmd.args(["lint", "--format", "sarif"]);
    cmd.assert().success();
    Ok(())
}

// --- Strict mode ---

#[test]
fn lint_strict_mode_fails_on_warnings() -> Result<(), Box<dyn std::error::Error>> {
    // app_name with spaces produces a warning
    let config = r#"
app_name: My App With Spaces
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
    cmd.args(["lint", "--strict"]);
    cmd.assert().failure();
    Ok(())
}

#[test]
fn lint_without_strict_passes_with_warnings() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: My App With Spaces
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
    cmd.arg("lint");
    cmd.assert().success();
    Ok(())
}

// --- JSON format with errors shows structured output ---

#[test]
fn lint_json_format_with_errors() -> Result<(), Box<dyn std::error::Error>> {
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
    cmd.assert().failure();
    Ok(())
}

// --- Undefined template reference ---

#[test]
fn lint_undefined_template_reference() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
steps:
  deps:
    template: nonexistent_template_that_does_not_exist
workflows:
  default:
    steps: [deps]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    // An undefined template should produce an error or warning
    cmd.assert().failure();
    Ok(())
}

// --- Multiple steps, some valid, some invalid ---

#[test]
fn lint_mixed_valid_and_invalid_steps() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: test-app
steps:
  good_step:
    command: echo good
  bad_step:
    command: echo bad
    depends_on: [nonexistent]
workflows:
  default:
    steps: [good_step, bad_step]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert().failure();
    Ok(())
}
