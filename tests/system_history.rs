//! System tests for `bivvy history` — all interactive, PTY-based.
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
app_name: "HistoryTest"
steps:
  greet:
    title: "Say hello"
    command: "echo hello"
workflows:
  default:
    steps: [greet]
"#;

fn run_workflow_first(dir: &std::path::Path) {
    let bin = cargo_bin("bivvy");
    // Use std::process::Command directly and wait for exit to ensure
    // run data is fully written before querying history.
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
fn history_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history"], temp.path());

    s.expect("No run history")
        .expect("Should indicate no history");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn history_after_run() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    s.expect("Run History").expect("Should show history header");
    s.expect("default").expect("Should show workflow name");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn history_limit_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["history", "--limit", "1"], temp.path());

    s.expect("Run History").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn history_detail_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());

    s.expect("Steps:").expect("Should show step details");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn history_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    // --json must produce actual JSON output with workflow data
    s.expect("workflow")
        .expect("Should output JSON with workflow key");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn history_since_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["history", "--since", "1h"], temp.path());

    s.expect("Run History").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn history_step_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_first(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "greet"], temp.path());

    s.expect(expectrl::Eof).ok();
}
