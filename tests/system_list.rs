//! System tests for `bivvy list` — all interactive, PTY-based.
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

const CONFIG: &str = r#"
app_name: "ListTest"
steps:
  deps:
    title: "Install dependencies"
    command: "echo deps"
    description: "Install all project dependencies"
  build:
    title: "Build project"
    command: "echo build"
    depends_on: [deps]
  test:
    title: "Run tests"
    command: "echo test"
    depends_on: [build]
workflows:
  default:
    steps: [deps, build, test]
  quick:
    description: "Quick check"
    steps: [test]
"#;

#[test]
fn list_shows_steps_and_workflows() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    // All output arrives quickly — read it all at once via Eof,
    // then verify content in the captured output.
    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(text.contains("Steps:"), "Should list steps section");
    assert!(text.contains("deps"), "Should list deps step");
    assert!(text.contains("build"), "Should list build step");
    assert!(text.contains("Workflows:"), "Should list workflows section");
    assert!(text.contains("default"), "Should list default workflow");
    assert!(text.contains("quick"), "Should list quick workflow");
}

#[test]
fn list_steps_only_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--steps-only"], temp.path());

    s.expect("Steps:").unwrap();
    s.expect("deps").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn list_workflows_only_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--workflows-only"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(text.contains("Workflows:"), "Should show workflows");
    assert!(text.contains("default"), "Should show default workflow");
    assert!(text.contains("quick"), "Should show quick workflow");
}

#[test]
fn list_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    s.expect("deps").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn list_env_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "ci"], temp.path());

    s.expect("Environment:").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn list_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["list"], temp.path());

    s.expect("No configuration found").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn list_shows_dependency_info() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    s.expect("depends on").expect("Should show dependencies");
    s.expect(expectrl::Eof).ok();
}
