//! System tests for `bivvy status` — all interactive, PTY-based.
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
app_name: "StatusTest"
steps:
  deps:
    title: "Install dependencies"
    command: "echo deps"
    completed_check:
      type: command_succeeds
      command: "true"
  build:
    title: "Build project"
    command: "echo build"
    depends_on: [deps]
  lint:
    title: "Lint code"
    command: "echo lint"
    depends_on: [build]
workflows:
  default:
    steps: [deps, build, lint]
"#;

#[test]
fn status_shows_app_name_and_steps() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(text.contains("StatusTest"), "Should show app name");
    assert!(text.contains("Steps:"), "Should list steps");
    assert!(text.contains("deps"), "Should show deps step");
    assert!(text.contains("build"), "Should show build step");
    assert!(text.contains("lint"), "Should show lint step");
}

#[test]
fn status_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["status"], temp.path());

    s.expect("No configuration found").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn status_env_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "development"], temp.path());

    s.expect("Environment:").expect("Should show environment");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn status_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    s.expect("app_name").expect("Should output JSON");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn status_verbose_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--verbose"], temp.path());

    s.expect("Steps:").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn status_quiet_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--quiet"], temp.path());

    s.expect(expectrl::Eof).ok();
}
