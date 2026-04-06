//! System tests for `bivvy last` — all interactive, PTY-based.
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
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
}

const SIMPLE_CONFIG: &str = r#"
app_name: "LastTest"
steps:
  greet:
    title: "Check git"
    command: "git --version"
workflows:
  default:
    steps: [greet]
"#;

/// Run a workflow first so `last` has data to show.
fn run_workflow_first(dir: &std::path::Path) {
    let bin = cargo_bin("bivvy");
    let status = Command::new(bin)
        .args(["run"])
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(status.success(), "bivvy run should succeed");
}

#[test]
fn last_no_runs_shows_message() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["last"], temp.path());

    s.expect("No runs recorded")
        .expect("Should indicate no runs");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn last_after_run_shows_details() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["last"], temp.path());

    s.expect("Last Run").expect("Should show last run header");
    s.expect("Workflow:").expect("Should show workflow");
    s.expect("default").expect("Should show workflow name");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn last_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["last", "--json"], temp.path());

    s.expect("workflow").expect("Should output JSON");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn last_all_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    s.expect("Last Run").expect("Should show last run header");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn last_output_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["last", "--output"], temp.path());

    s.expect("Last Run").expect("Should show last run header");
    s.expect(expectrl::Eof).unwrap();
}
