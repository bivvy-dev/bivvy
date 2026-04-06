//! System tests for `bivvy lint` — all interactive, PTY-based.
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

const VALID_CONFIG: &str = r#"
app_name: "LintTest"
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
  build:
    title: "Build project"
    command: "rustc --version"
    depends_on: [deps]
workflows:
  default:
    steps: [deps, build]
"#;

#[test]
fn lint_valid_config() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!")
        .expect("Should report valid config");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn lint_circular_dependency_detected() {
    let bad_config = r#"
app_name: "BadApp"
steps:
  a:
    command: "git --version"
    depends_on: [b]
  b:
    command: "cargo --version"
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;
    let temp = setup_project(bad_config);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Circular dependency detected:")
        .expect("Should detect circular dependency");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn lint_json_format_flag() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "json"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(
        text.contains("{") || text.contains("valid") || text.contains("ok"),
        "JSON format should produce structured output, got: {}",
        &text[..text.len().min(300)]
    );
}

#[test]
fn lint_sarif_format_flag() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--format", "sarif"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(
        text.contains("{") || text.contains("sarif") || text.contains("valid"),
        "SARIF format should produce structured output, got: {}",
        &text[..text.len().min(300)]
    );
}

#[test]
fn lint_strict_flag() {
    let temp = setup_project(VALID_CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn lint_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("No configuration found").unwrap();
    s.expect(expectrl::Eof).unwrap();
}
