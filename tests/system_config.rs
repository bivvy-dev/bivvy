//! System tests for `bivvy config` — all interactive, PTY-based.
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
app_name: "ConfigTest"
settings:
  default_output: verbose
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
workflows:
  default:
    steps: [deps]
"#;

#[test]
fn config_shows_yaml() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    s.expect("app_name").expect("Should show app_name");
    s.expect("ConfigTest").expect("Should show app name value");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn config_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    s.expect("ConfigTest").expect("Should show config in JSON");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn config_yaml_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--yaml"], temp.path());

    s.expect("app_name").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn config_merged_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    s.expect("app_name").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn config_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["config"], temp.path());

    s.expect("No configuration found").unwrap();
    s.expect(expectrl::Eof).unwrap();
}
