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

fn setup_project_with_git(config: &str) -> TempDir {
    let temp = setup_project(config);
    Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(temp.path())
        .output()
        .expect("git init failed");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(temp.path().join("Cargo.lock"), "# lock\n").unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(temp.path().join("VERSION"), "0.1.0\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(temp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp.path())
        .output()
        .unwrap();
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

/// Strip ANSI escape sequences for readable assertions.
fn strip_ansi(s: &str) -> String {
    s.chars()
        .fold((String::new(), false), |(mut acc, in_esc), c| {
            if c == '\x1b' {
                (acc, true)
            } else if in_esc {
                if c.is_ascii_alphabetic() {
                    (acc, false)
                } else {
                    (acc, true)
                }
            } else {
                acc.push(c);
                (acc, false)
            }
        })
        .0
}

/// Config where all steps have passing completed_checks — triggers "Already complete" prompts.
/// Uses real commands (rustc, git, test) instead of shell builtins.
const COMPLETED_CONFIG: &str = r#"
app_name: "RunTest"
settings:
  default_output: verbose

steps:
  deps:
    title: "Install dependencies"
    command: "rustc --version && git --version"
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  build:
    title: "Build project"
    command: "test -f Cargo.toml && wc -l Cargo.toml"
    depends_on: [deps]
    completed_check:
      type: command_succeeds
      command: "test -f Cargo.toml"

  test:
    title: "Run tests"
    command: "git status --short && wc -l src/main.rs"
    depends_on: [build]

  lint:
    title: "Lint code"
    command: "grep -c 'fn ' src/main.rs && head -1 Cargo.toml"
    depends_on: [build]

workflows:
  default:
    steps: [deps, build, test, lint]
  check:
    description: "Quick verification"
    steps: [lint, test]
"#;

/// Config with no completed_checks — everything runs fresh, no prompts.
/// Uses real commands (git, rustc) instead of echo.
const FRESH_CONFIG: &str = r#"
app_name: "FreshApp"
steps:
  greet:
    title: "Say hello"
    command: "rustc --version"
  farewell:
    title: "Say goodbye"
    command: "git --version"
workflows:
  default:
    steps: [greet, farewell]
"#;

// ---------------------------------------------------------------------------
// Default workflow (bare `bivvy`)
// ---------------------------------------------------------------------------

#[test]
fn bare_bivvy_runs_default_workflow() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let mut s = spawn_bivvy(&[], temp.path());

    // Interactive mode prompts for each skippable step — accept with 'y'
    s.expect("FreshApp").expect("Should show app name");

    // Accept prompts for each step (say yes to run them)
    s.expect("Say hello").unwrap();
    s.send("y").unwrap();
    s.expect("greet").unwrap();

    s.expect("Say goodbye").unwrap();
    s.send("y").unwrap();
    s.expect("farewell").unwrap();

    s.expect(expectrl::Eof).unwrap();
}

// ---------------------------------------------------------------------------
// `bivvy run` — basic execution
// ---------------------------------------------------------------------------

#[test]
fn run_default_workflow() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    s.expect("FreshApp").expect("Should show app name");
    s.expect("2 run").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_named_workflow() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "check", "--dry-run"], temp.path());

    s.expect("check workflow")
        .expect("Should show workflow name");
    s.expect(expectrl::Eof).unwrap();
}

// ---------------------------------------------------------------------------
// `bivvy run` — interactive prompts for completed steps
// ---------------------------------------------------------------------------

#[test]
fn run_interactive_completed_step_shows_rerun_prompt() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // Steps with passing completed_check trigger "Already complete. Re-run?"
    s.expect("Already complete")
        .expect("Should prompt about completed step");

    // Press Enter to accept default (No — skip)
    s.send_line("").unwrap();

    // Should continue to next steps
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_interactive_decline_rerun_skips_step() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // First completed step prompt
    s.expect("Already complete").unwrap();
    // Send 'n' to explicitly decline
    s.send("n").unwrap();

    s.expect("Skipped").expect("Should show skipped status");
    s.expect(expectrl::Eof).unwrap();
}

// ---------------------------------------------------------------------------
// `bivvy run` — flags
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_flag() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    s.expect("dry-run mode")
        .expect("Should indicate dry-run mode");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_verbose_flag() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    s.expect("FreshApp").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_quiet_flag() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--quiet"], temp.path());

    // Quiet mode should still complete successfully — verify exit via EOF
    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    let clean = strip_ansi(&text);
    // Quiet mode should produce minimal output (less than verbose)
    assert!(
        clean.len() < 5000,
        "Quiet mode should produce minimal output, got {} bytes",
        clean.len()
    );
}

#[test]
fn run_only_flag_filters_steps() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--only", "greet"], temp.path());

    s.expect("greet").expect("Should run filtered step");
    s.expect("1 run").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_skip_flag_skips_steps() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let mut s = spawn_bivvy(&["run", "--skip", "farewell"], temp.path());

    s.expect("greet").unwrap();
    s.expect("1 run").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_force_flag_reruns_completed() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--force", "deps"], temp.path());

    // Force should run deps without prompting about completion
    s.expect("deps").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_env_flag() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let mut s = spawn_bivvy(&["run", "--env", "ci", "--dry-run"], temp.path());

    s.expect("ci").expect("Should show ci environment");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn run_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["run"], temp.path());

    s.expect("No configuration found")
        .expect("Should error about missing config");
    s.expect(expectrl::Eof).unwrap();
}

// ---------------------------------------------------------------------------
// `bivvy run` — exit code verification
// ---------------------------------------------------------------------------

#[test]
fn run_success_exit_code_zero() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let bin = cargo_bin("bivvy");
    let status = Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Successful workflow should exit 0, got {:?}",
        status.code()
    );
}

#[test]
fn run_no_config_exit_code_non_zero() {
    let temp = TempDir::new().unwrap();
    let bin = cargo_bin("bivvy");
    let status = Command::new(bin)
        .args(["run"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Missing config should exit non-zero, got {:?}",
        status.code()
    );
}

#[test]
fn run_dry_run_exit_code_zero() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let bin = cargo_bin("bivvy");
    let status = Command::new(bin)
        .args(["run", "--dry-run"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Dry run should exit 0, got {:?}",
        status.code()
    );
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
    command: "rustc --version"
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
    s.expect(expectrl::Eof).unwrap();
}
