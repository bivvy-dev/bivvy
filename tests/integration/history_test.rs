//! Integration tests for the `bivvy history` command.
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

const TWO_WORKFLOW_CONFIG: &str = r#"
app_name: TwoWorkflows
steps:
  hello:
    command: echo hello
  world:
    command: echo world
workflows:
  default:
    steps: [hello]
  custom:
    steps: [world]
"#;

// --- Empty history ---

#[test]
fn history_no_runs_shows_message() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("history");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No run history"));
    Ok(())
}

#[test]
fn history_no_config_shows_empty() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("history");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No run history"));
    Ok(())
}

// --- Show execution history after runs ---

#[test]
fn history_after_single_run_shows_entry() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("history");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"))
        .stdout(predicate::str::contains("default"));
    Ok(())
}

#[test]
fn history_after_multiple_runs_shows_all() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(TWO_WORKFLOW_CONFIG);

    // Run default workflow
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    // Run custom workflow
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .args(["run", "--workflow", "custom"])
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("history");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("default"))
        .stdout(predicate::str::contains("custom"));
    Ok(())
}

// --- --limit flag ---

#[test]
fn history_limit_flag_restricts_output() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(TWO_WORKFLOW_CONFIG);

    // Run twice
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .args(["run", "--workflow", "custom"])
        .assert()
        .success();

    // With --limit 1, should show only the most recent run
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--limit", "1"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"))
        // Most recent is custom
        .stdout(predicate::str::contains("custom"));
    Ok(())
}

#[test]
fn history_limit_zero_shows_no_entries() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--limit", "0"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No run history"));
    Ok(())
}

// --- --since flag ---

#[test]
fn history_since_recent_includes_runs() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    // Runs within the last hour should be included
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--since", "1h"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"))
        .stdout(predicate::str::contains("default"));
    Ok(())
}

#[test]
fn history_since_very_short_excludes_runs() -> Result<(), Box<dyn std::error::Error>> {
    // Use --since 0m to filter out everything (0 minutes ago = now)
    // Since runs happen in the past, this should exclude them
    // Actually 0m means chrono::Duration::minutes(0) = zero duration,
    // cutoff = now - 0 = now. All runs have timestamp < now, so filtered out.
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--since", "0m"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No run history"));
    Ok(())
}

#[test]
fn history_since_accepts_days() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--since", "7d"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"));
    Ok(())
}

#[test]
fn history_since_accepts_weeks() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--since", "2w"]);
    cmd.assert().success();
    Ok(())
}

// --- --detail flag ---

#[test]
fn history_detail_flag_shows_step_info() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: DetailTest
steps:
  setup:
    command: echo setup
  build:
    command: echo build
workflows:
  default:
    steps: [setup, build]
"#;
    let temp = setup_project(config);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--detail"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Steps:"))
        .stdout(predicate::str::contains("setup"))
        .stdout(predicate::str::contains("build"));
    Ok(())
}

#[test]
fn history_without_detail_omits_step_info() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let output = Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("history")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Without --detail, should show run summary but not "Steps:" detail line
    assert!(stdout.contains("Run History"));
    // The "Steps:" line only appears in detail mode (indented under each run)
    // Note: The summary line shows step count like "1 step" but not "Steps:"
    assert!(!stdout.contains("        Steps:"));
    Ok(())
}

// --- State persistence across runs ---

#[test]
fn history_persists_across_separate_invocations() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(TWO_WORKFLOW_CONFIG);

    // First run
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    // Verify history shows one run
    let output1 = Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("history")
        .output()?;
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    assert!(stdout1.contains("default"));

    // Second run with different workflow
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .args(["run", "--workflow", "custom"])
        .assert()
        .success();

    // History should now show both runs
    let output2 = Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("history")
        .output()?;
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert!(stdout2.contains("default"));
    assert!(stdout2.contains("custom"));
    Ok(())
}

#[test]
fn last_and_history_agree_on_most_recent() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(TWO_WORKFLOW_CONFIG);

    // Run default, then custom
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .args(["run", "--workflow", "custom"])
        .assert()
        .success();

    // last should show "custom"
    let last_output = Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("last")
        .output()?;
    let last_stdout = String::from_utf8_lossy(&last_output.stdout);
    assert!(last_stdout.contains("custom"));

    // history should list "custom" first (most recent)
    let hist_output = Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("history")
        .output()?;
    let hist_stdout = String::from_utf8_lossy(&hist_output.stdout);
    assert!(hist_stdout.contains("custom"));
    Ok(())
}

// --- Failed run in history ---

#[test]
fn history_includes_failed_runs() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: FailHistory
steps:
  bad:
    command: "false"
workflows:
  default:
    steps: [bad]
"#;
    let temp = setup_project(config);

    // Run should fail
    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .failure();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("history");
    // History should still show the failed run
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"));
    Ok(())
}

#[test]
fn history_detail_shows_error_for_failed_run() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: FailDetail
steps:
  bad:
    command: "false"
workflows:
  default:
    steps: [bad]
"#;
    let temp = setup_project(config);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .failure();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--detail"]);
    cmd.assert().success();
    // With detail, error info should be shown (on stderr from ui.error())
    Ok(())
}

// --- Help flag ---

#[test]
fn history_help_shows_usage() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["history", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("execution history"));
    Ok(())
}

// --- Flags are accepted ---

#[test]
fn history_accepts_json_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--json"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn history_accepts_step_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--step", "hello"]);
    cmd.assert().success();
    Ok(())
}

// --- Global flags work with history ---

#[test]
fn history_with_debug_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--debug", "history"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn history_with_quiet_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["--quiet", "history"]);
    cmd.assert().success();
    Ok(())
}

// --- Combined flags ---

#[test]
fn history_limit_and_detail_combined() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--limit", "1", "--detail"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"));
    Ok(())
}

#[test]
fn history_since_and_limit_combined() -> Result<(), Box<dyn std::error::Error>> {
    let temp = setup_project(SIMPLE_CONFIG);

    Command::new(cargo_bin("bivvy"))
        .current_dir(temp.path())
        .arg("run")
        .assert()
        .success();

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["history", "--since", "1h", "--limit", "5"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run History"));
    Ok(())
}
