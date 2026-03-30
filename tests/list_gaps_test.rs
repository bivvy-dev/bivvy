//! Integration tests for `bivvy list` — gaps not covered by cli_test.rs.
//!
//! Covers: --steps-only, --workflows-only, workflow listing details,
//! dependency display, template references, environment filtering, and descriptions.
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
fn list_no_config_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No configuration found"));
    Ok(())
}

// --- --steps-only ---

#[test]
fn list_steps_only_hides_workflows() -> Result<(), Box<dyn std::error::Error>> {
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
    cmd.args(["list", "--steps-only"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Steps:"), "Should show steps section");
    assert!(
        !stdout.contains("Workflows:"),
        "Should not show workflows section with --steps-only"
    );
    Ok(())
}

// --- --workflows-only ---

#[test]
fn list_workflows_only_hides_steps() -> Result<(), Box<dyn std::error::Error>> {
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
    cmd.args(["list", "--workflows-only"]);
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Steps:"),
        "Should not show steps section with --workflows-only"
    );
    assert!(
        stdout.contains("Workflows:"),
        "Should show workflows section"
    );
    Ok(())
}

// --- Workflow display with arrows ---

#[test]
fn list_workflow_shows_step_order_with_arrows() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm run build
  deploy:
    command: bin/deploy
workflows:
  default:
    steps: [install, build, deploy]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert().success();
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Workflows should show step order with arrow separators
    assert!(
        stdout.contains("\u{2192}"), // →
        "Workflow should show arrows between steps"
    );
    Ok(())
}

// --- Multiple workflows ---

#[test]
fn list_multiple_workflows() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  install:
    command: npm install
  build:
    command: npm run build
  test:
    command: npm test
workflows:
  default:
    steps: [install, build]
  ci:
    steps: [install, test]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("default"))
        .stdout(predicate::str::contains("ci"));
    Ok(())
}

// --- Dependency tree display ---

#[test]
fn list_shows_dependency_info() -> Result<(), Box<dyn std::error::Error>> {
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
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("depends on:"))
        .stdout(predicate::str::contains("install"));
    Ok(())
}

// --- Template references ---

#[test]
fn list_shows_template_reference() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  deps:
    template: yarn
workflows:
  default:
    steps: [deps]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("template: yarn"));
    Ok(())
}

// --- Command display ---

#[test]
fn list_shows_step_command() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello world
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("echo hello world"));
    Ok(())
}

// --- Description display ---

#[test]
fn list_shows_step_description() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
    description: "Greets the world"
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Greets the world"));
    Ok(())
}

#[test]
fn list_shows_step_title_when_no_description() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
    title: "Hello Step"
workflows:
  default:
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Hello Step"));
    Ok(())
}

#[test]
fn list_shows_workflow_description() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  hello:
    command: echo hello
workflows:
  default:
    description: "Full development setup"
    steps: [hello]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Full development setup"));
    Ok(())
}

// --- Environment filtering ---

#[test]
fn list_with_env_shows_environment() -> Result<(), Box<dyn std::error::Error>> {
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
    cmd.args(["list", "--env", "staging"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Environment:"))
        .stdout(predicate::str::contains("staging"));
    Ok(())
}

#[test]
fn list_env_filtering_skips_non_matching_steps() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  dev_only:
    command: echo dev
    only_environments: [development]
  always:
    command: echo always
workflows:
  default:
    steps: [dev_only, always]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["list", "--env", "ci"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skipped"));
    Ok(())
}

// --- Environment info is always shown ---

#[test]
fn list_default_shows_environment_info() -> Result<(), Box<dyn std::error::Error>> {
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
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Environment:"));
    Ok(())
}

// --- Many steps ---

#[test]
fn list_many_steps_all_shown() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Test
steps:
  step_a:
    command: echo a
  step_b:
    command: echo b
  step_c:
    command: echo c
workflows:
  default:
    steps: [step_a, step_b, step_c]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("step_a"))
        .stdout(predicate::str::contains("step_b"))
        .stdout(predicate::str::contains("step_c"));
    Ok(())
}
