//! Integration tests for the `bivvy config` CLI command.
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

// --- Basic output ---

#[test]
fn config_shows_yaml_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("app_name"))
        .stdout(predicate::str::contains("hello"));
    Ok(())
}

#[test]
fn config_shows_config_file_path() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("config.yml"));
    Ok(())
}

// --- JSON output ---

#[test]
fn config_json_outputs_valid_json() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--json"]);
    let output = cmd.output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    // Should contain JSON object markers
    assert!(stdout.contains('{'));
    assert!(stdout.contains('}'));
    // Should contain the app_name field
    assert!(stdout.contains("\"app_name\""));
    assert!(stdout.contains("\"Test\""));
    // Verify it's valid JSON by parsing
    let json_lines: String = stdout
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let _: serde_json::Value = serde_json::from_str(json_lines.trim())?;
    Ok(())
}

#[test]
fn config_json_contains_steps() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"hello\""))
        .stdout(predicate::str::contains("echo hello"));
    Ok(())
}

// --- YAML output ---

#[test]
fn config_yaml_flag_outputs_yaml() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--yaml"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("app_name"))
        .stdout(predicate::str::contains("hello"));
    Ok(())
}

// --- Merged config ---

#[test]
fn config_merged_shows_merged_result() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;

    // Project config
    fs::write(
        bivvy_dir.join("config.yml"),
        r#"
app_name: BaseApp
settings:
  default_output: verbose
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
    )?;

    // Local overrides
    fs::write(
        bivvy_dir.join("config.local.yml"),
        r#"
settings:
  default_output: quiet
"#,
    )?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--merged"]);
    // The merged output should reflect the local override
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("quiet"));
    Ok(())
}

#[test]
fn config_merged_includes_local_override_values() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;

    fs::write(
        bivvy_dir.join("config.yml"),
        r#"
app_name: OriginalName
steps:
  deps:
    command: npm install
  test:
    command: npm test
workflows:
  default:
    steps: [deps, test]
"#,
    )?;

    fs::write(
        bivvy_dir.join("config.local.yml"),
        r#"
steps:
  deps:
    command: yarn install
"#,
    )?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--merged"]);
    // The merged config should have the overridden command
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("yarn install"))
        .stdout(predicate::str::contains("npm test"))
        .stdout(predicate::str::contains("OriginalName"));
    Ok(())
}

#[test]
fn config_shows_both_config_file_paths_when_local_exists() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;
    fs::write(bivvy_dir.join("config.yml"), SIMPLE_CONFIG)?;
    fs::write(bivvy_dir.join("config.local.yml"), "app_name: Local\n")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--merged"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("config.yml"))
        .stdout(predicate::str::contains("config.local.yml"));
    Ok(())
}

// --- Error cases ---

#[test]
fn config_no_config_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No configuration found"));
    Ok(())
}

#[test]
fn config_invalid_yaml_fails() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;
    fs::write(bivvy_dir.join("config.yml"), "invalid: yaml: [unclosed")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert().failure();
    Ok(())
}

// --- Config with various field types ---

#[test]
fn config_shows_step_details() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: DetailTest
steps:
  deps:
    command: npm install
    title: Install dependencies
    depends_on: []
    watches:
      - package.json
      - package-lock.json
  test:
    command: npm test
    depends_on: [deps]
workflows:
  default:
    steps: [deps, test]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("npm install"))
        .stdout(predicate::str::contains("package.json"))
        .stdout(predicate::str::contains("npm test"));
    Ok(())
}

#[test]
fn config_json_with_complex_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Complex
steps:
  deps:
    command: npm install
    watches:
      - package.json
  test:
    command: npm test
    depends_on: [deps]
workflows:
  default:
    steps: [deps, test]
  ci:
    steps: [deps, test]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["config", "--json"]);
    let output = cmd.output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    // Extract JSON portion (skip comment lines)
    let json_lines: String = stdout
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value = serde_json::from_str(json_lines.trim())?;
    // Verify structure
    assert_eq!(parsed["app_name"], "Complex");
    assert!(parsed["steps"]["deps"].is_object());
    assert!(parsed["steps"]["test"].is_object());
    assert!(parsed["workflows"]["default"].is_object());
    assert!(parsed["workflows"]["ci"].is_object());
    Ok(())
}

// --- Settings in config output ---

#[test]
fn config_shows_settings() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: SettingsTest
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
    cmd.arg("config");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("quiet"));
    Ok(())
}

// --- Completed check types in config output ---

#[test]
fn config_shows_completed_checks() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: CheckTest
steps:
  deps:
    command: npm install
    completed_check:
      type: file_exists
      path: node_modules/.package-lock.json
  db:
    command: rails db:setup
    completed_check:
      type: command_succeeds
      command: rails db:version
workflows:
  default:
    steps: [deps, db]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("file_exists"))
        .stdout(predicate::str::contains("node_modules"))
        .stdout(predicate::str::contains("command_succeeds"))
        .stdout(predicate::str::contains("rails db:version"));
    Ok(())
}

// --- Empty config ---

#[test]
fn config_handles_minimal_config() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;
    fs::write(bivvy_dir.join("config.yml"), "app_name: Minimal\n")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("config");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Minimal"));
    Ok(())
}

#[test]
fn config_help_shows_flags() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["config", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--yaml"))
        .stdout(predicate::str::contains("--merged"));
    Ok(())
}
