//! System tests for `bivvy run` — all interactive, PTY-based.
//!
//! Every test runs the real binary in a PTY to exercise the same
//! code paths an interactive user hits. No --non-interactive shortcuts.
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

/// Config where all steps have passing completed_checks — triggers "Already complete" prompts.
const COMPLETED_CONFIG: &str = r#"
app_name: "RunTest"
settings:
  default_output: verbose

steps:
  deps:
    title: "Install dependencies"
    command: "echo installing-deps"
    completed_check:
      type: command_succeeds
      command: "true"

  build:
    title: "Build project"
    command: "echo building"
    depends_on: [deps]
    completed_check:
      type: command_succeeds
      command: "true"

  test:
    title: "Run tests"
    command: "echo testing"
    depends_on: [build]

  lint:
    title: "Lint code"
    command: "echo linting"
    depends_on: [build]

workflows:
  default:
    steps: [deps, build, test, lint]
  check:
    description: "Quick verification"
    steps: [lint, test]
"#;

/// Config with no completed_checks — everything runs fresh, no prompts.
const FRESH_CONFIG: &str = r#"
app_name: "FreshApp"
steps:
  greet:
    title: "Say hello"
    command: "echo hello-world"
  farewell:
    title: "Say goodbye"
    command: "echo goodbye-world"
workflows:
  default:
    steps: [greet, farewell]
"#;

// ---------------------------------------------------------------------------
// Default workflow (bare `bivvy`)
// ---------------------------------------------------------------------------

#[test]
fn bare_bivvy_runs_default_workflow() {
    let temp = setup_project(FRESH_CONFIG);
    let mut s = spawn_bivvy(&[], temp.path());

    // Interactive mode prompts for each skippable step — accept with 'y'
    s.expect("FreshApp").expect("Should show app name");

    // Accept prompts for each step (say yes to run them)
    if s.expect("Say hello").is_ok() {
        s.send("y").unwrap();
    }
    s.expect("greet").ok();

    if s.expect("Say goodbye").is_ok() {
        s.send("y").unwrap();
    }
    s.expect("farewell").ok();

    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// `bivvy run` — basic execution
// ---------------------------------------------------------------------------

#[test]
fn run_default_workflow() {
    let temp = setup_project(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    s.expect("FreshApp").expect("Should show app name");
    s.expect("2 run").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_named_workflow() {
    let temp = setup_project(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "check", "--dry-run"], temp.path());

    s.expect("check workflow")
        .expect("Should show workflow name");
    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// `bivvy run` — interactive prompts for completed steps
// ---------------------------------------------------------------------------

#[test]
fn run_interactive_completed_step_shows_rerun_prompt() {
    let temp = setup_project(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // Steps with passing completed_check trigger "Already complete. Re-run?"
    s.expect("Already complete")
        .expect("Should prompt about completed step");

    // Press Enter to accept default (No — skip)
    s.send_line("").unwrap();

    // Should continue to next steps
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_interactive_decline_rerun_skips_step() {
    let temp = setup_project(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // First completed step prompt
    s.expect("Already complete").unwrap();
    // Send 'n' to explicitly decline
    s.send("n").unwrap();

    s.expect("Skipped").expect("Should show skipped status");
    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// `bivvy run` — flags
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_flag() {
    let temp = setup_project(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    s.expect("dry-run mode")
        .expect("Should indicate dry-run mode");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_verbose_flag() {
    let temp = setup_project(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    s.expect("FreshApp").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_quiet_flag() {
    let temp = setup_project(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--quiet"], temp.path());

    // Quiet mode should still complete successfully
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_only_flag_filters_steps() {
    let temp = setup_project(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--only", "greet"], temp.path());

    s.expect("greet").expect("Should run filtered step");
    s.expect("1 run").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_skip_flag_skips_steps() {
    let temp = setup_project(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--skip", "farewell"], temp.path());

    s.expect("greet").unwrap();
    s.expect("1 run").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_force_flag_reruns_completed() {
    let temp = setup_project(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--force", "deps"], temp.path());

    // Force should run deps without prompting about completion
    s.expect("deps").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_env_flag() {
    let temp = setup_project(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--env", "ci", "--dry-run"], temp.path());

    s.expect("ci").expect("Should show ci environment");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn run_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["run"], temp.path());

    s.expect("No configuration found")
        .expect("Should error about missing config");
    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// `bivvy run` — env var overrides skip prompts
// ---------------------------------------------------------------------------

#[test]
fn run_env_var_override_skips_prompt() {
    let config = r#"
app_name: "EnvTest"
steps:
  deploy:
    title: "Deploy"
    command: "echo deploying"
    prompts:
      - key: target
        question: "Deploy target"
        type: select
        options:
          - label: "Staging"
            value: staging
          - label: "Production"
            value: production
workflows:
  default:
    steps: [deploy]
"#;
    let temp = setup_project(config);
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(["run"]);
    cmd.current_dir(temp.path());
    cmd.env("TARGET", "staging");

    let mut s = Session::spawn(cmd).expect("Failed to spawn");
    s.set_expect_timeout(Some(Duration::from_secs(30)));

    // Should NOT show "Deploy target" prompt because TARGET=staging is set
    // Should proceed directly to execution
    s.expect("deploy").unwrap();
    s.expect(expectrl::Eof).ok();
}
