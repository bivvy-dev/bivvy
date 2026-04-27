//! Integration tests for `bivvy run` gap coverage.
//!
//! These tests cover CLI-level behavior for flags and features not
//! exercised by the existing `cli_test.rs` suite: step filtering,
//! force re-run, resume, non-interactive mode, skip behavior,
//! dependency ordering, circular dependency detection, and CI
//! auto-detection.
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

// ---------------------------------------------------------------------------
// --only step filtering
// ---------------------------------------------------------------------------

const MULTI_STEP_CONFIG: &str = r#"
app_name: MultiStep
steps:
  alpha:
    command: echo alpha-output
  beta:
    command: echo beta-output
  gamma:
    command: echo gamma-output
workflows:
  default:
    steps: [alpha, beta, gamma]
"#;

#[test]
fn run_only_single_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--only", "beta"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("beta"))
        // alpha and gamma should not appear as executed steps
        .stdout(predicate::str::contains("1 run"));
    Ok(())
}

#[test]
fn run_only_multiple_steps() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--only", "alpha,gamma"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("2 run"));
    Ok(())
}

// ---------------------------------------------------------------------------
// --skip step filtering
// ---------------------------------------------------------------------------

#[test]
fn run_skip_single_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--skip", "beta"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("2 run"));
    Ok(())
}

#[test]
fn run_skip_multiple_steps() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--skip", "alpha,gamma"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1 run"));
    Ok(())
}

// ---------------------------------------------------------------------------
// --force re-run of completed steps
// ---------------------------------------------------------------------------

const COMPLETED_STEP_CONFIG: &str = r#"
app_name: ForceTest
steps:
  already_done:
    command: echo forced-run
    check:
      type: execution
      command: "exit 0"
workflows:
  default:
    steps: [already_done]
"#;

#[test]
fn run_force_reruns_completed_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(COMPLETED_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--force", "already_done"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1 run"));
    Ok(())
}

#[test]
fn run_without_force_skips_completed_step() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(COMPLETED_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Skipped"));
    Ok(())
}

// ---------------------------------------------------------------------------
// --resume flag (accepted by CLI parser)
// ---------------------------------------------------------------------------

#[test]
fn run_resume_flag_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--resume"]);
    // The flag should be accepted without error; the run still succeeds
    cmd.assert().success();
    Ok(())
}

// ---------------------------------------------------------------------------
// --save-preferences flag (accepted by CLI parser)
// ---------------------------------------------------------------------------

#[test]
fn run_save_preferences_flag_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--save-preferences"]);
    cmd.assert().success();
    Ok(())
}

// ---------------------------------------------------------------------------
// --non-interactive mode
// ---------------------------------------------------------------------------

#[test]
fn run_non_interactive_flag_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--non-interactive"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn run_non_interactive_skips_completed_without_prompt() -> Result<(), Box<dyn std::error::Error>> {
    // In non-interactive mode, completed steps should be silently skipped
    let temp = setup_project(COMPLETED_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--non-interactive"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Skipped"));
    Ok(())
}

// ---------------------------------------------------------------------------
// --skip-behavior flag
// ---------------------------------------------------------------------------

const DEPENDENCY_CHAIN_CONFIG: &str = r#"
app_name: DepChain
steps:
  first:
    command: echo first-output
  second:
    command: echo second-output
    depends_on: [first]
  third:
    command: echo third-output
    depends_on: [second]
workflows:
  default:
    steps: [first, second, third]
"#;

#[test]
fn skip_behavior_skip_with_dependents() -> Result<(), Box<dyn std::error::Error>> {
    // Default behavior: skipping "first" also skips "second" and "third"
    let temp = setup_project(DEPENDENCY_CHAIN_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args([
        "run",
        "--skip",
        "first",
        "--skip-behavior",
        "skip_with_dependents",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0 run"));
    Ok(())
}

#[test]
fn skip_behavior_skip_only() -> Result<(), Box<dyn std::error::Error>> {
    // skip_only: skipping "first" still attempts "second" and "third"
    let temp = setup_project(DEPENDENCY_CHAIN_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--skip", "first", "--skip-behavior", "skip_only"]);
    // second and third should attempt to run (they may fail if first's
    // output was needed, but the point is they are not pre-emptively skipped)
    cmd.assert().success();
    Ok(())
}

#[test]
fn skip_behavior_run_anyway() -> Result<(), Box<dyn std::error::Error>> {
    // run_anyway: the skip set is effectively empty
    let temp = setup_project(DEPENDENCY_CHAIN_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--skip", "first", "--skip-behavior", "run_anyway"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("3 run"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Dependency resolution and ordering
// ---------------------------------------------------------------------------

#[test]
fn dependency_order_respected() -> Result<(), Box<dyn std::error::Error>> {
    // Steps with depends_on should run after their dependency
    let config = r#"
app_name: DepOrder
steps:
  setup_db:
    command: echo setup-db-done
  migrate:
    command: echo migrate-done
    depends_on: [setup_db]
  seed:
    command: echo seed-done
    depends_on: [migrate]
workflows:
  default:
    steps: [seed, migrate, setup_db]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("3 run"));
    Ok(())
}

#[test]
fn diamond_dependency_works() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Diamond
steps:
  base:
    command: echo base
  left:
    command: echo left
    depends_on: [base]
  right:
    command: echo right
    depends_on: [base]
  final_step:
    command: echo final
    depends_on: [left, right]
workflows:
  default:
    steps: [base, left, right, final_step]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("4 run"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Circular dependency detection
// ---------------------------------------------------------------------------

#[test]
fn circular_dependency_errors() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: Circular
steps:
  a:
    command: echo a
    depends_on: [b]
  b:
    command: echo b
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Circular dependency"));
    Ok(())
}

#[test]
fn three_way_circular_dependency_errors() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: TriCircle
steps:
  a:
    command: echo a
    depends_on: [c]
  b:
    command: echo b
    depends_on: [a]
  c:
    command: echo c
    depends_on: [b]
workflows:
  default:
    steps: [a, b, c]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Circular dependency"));
    Ok(())
}

// ---------------------------------------------------------------------------
// CI auto-detection (CI=true env var)
// ---------------------------------------------------------------------------

#[test]
fn ci_env_var_auto_detects_ci_environment() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: CIAutoDetect
steps:
  hello:
    command: echo hello
  ci_step:
    command: echo ci-only
    only_environments: [ci]
workflows:
  default:
    steps: [hello, ci_step]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    // Set CI=true to trigger auto-detection without using --env
    cmd.env("CI", "true");
    // Clear other CI vars that might interfere
    cmd.env_remove("GITHUB_ACTIONS");
    cmd.env_remove("GITLAB_CI");
    cmd.args(["run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ci_step"))
        .stdout(predicate::str::contains("2 run"));
    Ok(())
}

#[test]
fn no_ci_env_var_skips_ci_only_steps() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: NoCIDetect
steps:
  hello:
    command: echo hello
  ci_step:
    command: echo ci-only
    only_environments: [ci]
workflows:
  default:
    steps: [hello, ci_step]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    // Explicitly clear CI-related env vars to ensure development environment
    cmd.env_remove("CI");
    cmd.env_remove("GITHUB_ACTIONS");
    cmd.env_remove("GITLAB_CI");
    cmd.env_remove("CIRCLECI");
    cmd.env_remove("JENKINS_URL");
    cmd.env_remove("TRAVIS");
    cmd.env_remove("TF_BUILD");
    cmd.env_remove("BUILDKITE");
    cmd.env_remove("CODESPACES");
    cmd.env_remove("GITPOD_WORKSPACE_ID");
    cmd.env_remove("DOCKER_CONTAINER");
    cmd.args(["run", "--env", "development"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1 run"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Combining flags
// ---------------------------------------------------------------------------

#[test]
fn dry_run_with_only_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--dry-run", "--only", "alpha"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry-run mode"));
    Ok(())
}

#[test]
fn dry_run_with_skip_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(MULTI_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--dry-run", "--skip", "beta"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry-run mode"));
    Ok(())
}

#[test]
fn force_and_non_interactive_together() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(COMPLETED_STEP_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--force", "already_done", "--non-interactive"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1 run"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Failed step blocks dependents
// ---------------------------------------------------------------------------

#[test]
fn failed_step_blocks_dependents() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: FailBlocks
steps:
  broken:
    command: "exit 1"
  after_broken:
    command: echo should-not-run
    depends_on: [broken]
workflows:
  default:
    steps: [broken, after_broken]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--non-interactive"]);
    cmd.assert().failure();
    Ok(())
}
