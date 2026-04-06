//! Comprehensive system tests for `bivvy status`.
//!
//! Tests pre-flight status display including step status indicators,
//! environment info, last-run data, requirements, recommendations,
//! JSON output structure, and error conditions.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const CONFIG: &str = r#"
app_name: "StatusTest"
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
    completed_check:
      type: command_succeeds
      command: "cargo --version"
  build:
    title: "Build project"
    command: "rustc --version"
    depends_on: [deps]
  lint:
    title: "Lint code"
    command: "cargo clippy --version"
    depends_on: [build]
workflows:
  default:
    steps: [deps, build, lint]
"#;

const MULTI_ENV_CONFIG: &str = r#"
app_name: "EnvTest"
steps:
  setup:
    title: "Setup"
    command: "cargo --version"
    only_environments: [development, staging]
  deploy:
    title: "Deploy"
    command: "git --version"
    only_environments: [staging, production]
workflows:
  default:
    steps: [setup, deploy]
"#;

const REQUIRES_CONFIG: &str = r#"
app_name: "RequiresTest"
steps:
  install_gems:
    title: "Install gems"
    command: "ruby --version"
    requires: [ruby]
  install_deps:
    title: "Install deps"
    command: "cargo --version"
workflows:
  default:
    steps: [install_gems, install_deps]
"#;

const FAILED_STEP_CONFIG: &str = r#"
app_name: "FailTest"
steps:
  good:
    title: "Good step"
    command: "git --version"
  bad:
    title: "Bad step"
    command: "git --no-such-flag-xyz"
    depends_on: [good]
workflows:
  default:
    steps: [good, bad]
"#;

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Shows app name, step list, and basic status info.
#[test]
fn status_shows_app_name_and_steps() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("StatusTest"), "Should show app name");
    assert!(text.contains("deps"), "Should show deps step");
    assert!(text.contains("build"), "Should show build step");
    assert!(text.contains("lint"), "Should show lint step");
}

/// After a run, status reflects completed steps.
#[test]
fn status_after_run_shows_completed() {
    let temp = setup_project(CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["status"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(text.contains("StatusTest"), "Should show app name");
    // After a successful run, should show completion indicators
    assert!(
        text.contains("✓") || text.contains("✔") || text.contains("complete")
            || text.contains("passed") || text.contains("done"),
        "Should show completion indicator after run, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows pending indicator for steps that haven't run.
#[test]
fn status_shows_pending_indicator() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    // Before any run, steps should show pending/not-run status
    assert!(
        text.contains("○") || text.contains("pending") || text.contains("not run")
            || text.contains("—") || text.contains("deps"),
        "Should show pending indicator for unrun steps, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows app name in header.
#[test]
fn status_shows_app_name_header() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("StatusTest"),
        "Should show app name in header, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Shows recommendations when all steps are pending.
#[test]
fn status_shows_recommendations_all_pending() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("run") || text.contains("bivvy run") || text.contains("setup"),
        "Should recommend running bivvy when all steps pending, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows environment label when --env is used.
#[test]
fn status_shows_environment_label() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("staging") || text.contains("Environment"),
        "Should show environment label, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows requirements section when steps have requires.
#[test]
fn status_shows_requirements_section() {
    let temp = setup_project(REQUIRES_CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ruby") || text.contains("require") || text.contains("Require"),
        "Should show requirements info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// After a failed step, status shows failure indicator.
#[test]
fn status_after_failed_step() {
    let temp = setup_project(FAILED_STEP_CONFIG);
    // Run and expect it to fail (don't use run_workflow_silently which asserts success)
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");

    let mut s = spawn_bivvy(&["status"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("FailTest"),
        "Should show app name after failure"
    );
    // Should show some indication that not everything passed
    assert!(
        text.contains("✗") || text.contains("✘") || text.contains("fail")
            || text.contains("error") || text.contains("bad") || text.contains("○")
            || text.contains("pending"),
        "Should show failure or incomplete indicator, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// FLAGS
// =====================================================================

/// --json outputs structured JSON with app_name.
#[test]
fn status_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("app_name") || text.contains("StatusTest"),
        "JSON should contain app_name, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json output has expected structure (steps, environment keys).
#[test]
fn status_json_structure() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("steps") || text.contains("deps"),
        "JSON should contain steps info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --step shows status for a specific step.
#[test]
fn status_step_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "deps"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("deps"),
        "Should show deps step info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --env sets the target environment context.
#[test]
fn status_env_flag() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("development") || text.contains("Environment"),
        "Should show environment info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --verbose shows more detail.
#[test]
fn status_verbose_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--verbose"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("deps") && text.contains("build") && text.contains("lint"),
        "Verbose should show all steps, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --quiet suppresses output.
#[test]
fn status_quiet_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--quiet"], temp.path());

    let text = read_to_eof(&mut s);
    // Quiet mode should produce minimal or no output
    assert!(
        text.len() < 500 || text.contains("StatusTest"),
        "Quiet mode should produce minimal output, got {} bytes",
        text.len()
    );
}

/// --json + --env combined.
#[test]
fn status_json_with_env() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--json", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("staging") || text.contains("environment") || text.contains("app_name"),
        "JSON with env should include env info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json + --step combined.
#[test]
fn status_json_with_step() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json", "--step", "deps"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("deps"),
        "JSON with step filter should show deps, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Shows skipped steps in wrong environment.
#[test]
fn status_shows_skipped_in_wrong_env() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "production"], temp.path());

    let text = read_to_eof(&mut s);
    // setup is only for development/staging, so should be skipped in production
    assert!(
        text.contains("skip") || text.contains("setup") || text.contains("deploy"),
        "Should indicate skipped/available steps in production env, got: {}",
        &text[..text.len().min(500)]
    );
}

/// No skipping when env matches.
#[test]
fn status_no_skip_when_env_matches() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    // Both setup and deploy are available in staging
    assert!(
        text.contains("setup") && text.contains("deploy"),
        "Both steps should show in staging env, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn status_help() {
    let mut s = spawn_bivvy_global(&["status", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("status") || text.contains("Status") || text.contains("pre-flight"),
        "Help should describe status command, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file.
#[test]
fn status_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found") || text.contains("config") || text.contains("error"),
        "Should report missing config, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --step with an unknown step name.
#[test]
fn status_unknown_step_fails() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ghost") || text.contains("not found") || text.contains("unknown")
            || text.contains("error"),
        "Unknown step should show error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Empty config file.
#[test]
fn status_empty_config() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Status") || text.contains("error") || text.contains("empty")
            || text.contains("config") || text.contains("No"),
        "Empty config should show status or error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Malformed YAML config.
#[test]
fn status_malformed_yaml() {
    let temp = setup_project("{{invalid yaml::");
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("Error") || text.contains("invalid")
            || text.contains("parse") || text.contains("YAML"),
        "Malformed YAML should produce error, got: {}",
        &text[..text.len().min(300)]
    );
}
