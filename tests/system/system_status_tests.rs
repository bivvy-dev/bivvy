//! Comprehensive system tests for `bivvy status`.
//!
//! Tests pre-flight status display including step status indicators,
//! environment info, last-run data, requirements, recommendations,
//! JSON output structure, and error conditions.
#![cfg(unix)]

mod system;

use expectrl::WaitStatus;
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
    check:
      type: execution
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

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Parse JSON out of a PTY-captured output. PTY translates `\n` to `\r\n`
/// and may include trailing whitespace, so we strip `\r` and trim before
/// parsing.
fn parse_pty_json(text: &str) -> serde_json::Value {
    let cleaned: String = text.chars().filter(|c| *c != '\r').collect();
    let trimmed = cleaned.trim();
    serde_json::from_str(trimmed).unwrap_or_else(|e| {
        panic!("Should output valid JSON. Error: {e}\nGot:\n---\n{trimmed}\n---")
    })
}

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Shows app name, step list, Steps: label, and Environment: label.
#[test]
fn status_shows_app_name_and_steps() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("StatusTest"),
        "Should show app name, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Steps:"),
        "Should show Steps: label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Environment:"),
        "Should show Environment: label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("deps"),
        "Should show deps step, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("build"),
        "Should show build step, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("lint"),
        "Should show lint step, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// After a run, status reflects completed steps with success icon and
/// shows a "Last run:" label per docs.
#[test]
fn status_after_run_shows_completed() {
    let temp = setup_project(CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["status"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(text.contains("StatusTest"), "Should show app name");
    // After a successful run, should show success icon ✓ for completed steps
    assert!(
        text.contains("✓"),
        "Should show ✓ completion indicator after run, got: {}",
        &text[..text.len().min(500)]
    );
    // Docs specify the "Last run:" label should appear after a run
    assert!(
        text.contains("Last run:"),
        "Should show 'Last run:' label after a run, got: {}",
        &text[..text.len().min(500)]
    );
    // After a successful run, the "all steps pending" hint should NOT appear.
    assert!(
        !text.contains("Run `bivvy run` to start setup."),
        "Should not show all-pending hint after successful run, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// Before any run, steps show pending icon ◌.
#[test]
fn status_shows_pending_indicator() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    // Before any run, steps should show ◌ pending icon
    assert!(
        text.contains("◌"),
        "Should show ◌ pending indicator for unrun steps, got: {}",
        &text[..text.len().min(500)]
    );
    // Before any run, the "Last run:" label should not appear.
    assert!(
        !text.contains("Last run:"),
        "Should not show 'Last run:' before any run, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// When all steps are pending, shows the exact "Run `bivvy run` to start setup."
/// hint from `hints::all_steps_pending()`.
#[test]
fn status_shows_recommendations_all_pending() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run `bivvy run` to start setup."),
        "Should show exact all-pending hint, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// Shows environment label and source when no --env flag is given
/// (should show "(default)" source per docs).
#[test]
fn status_shows_default_environment_source() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment:"),
        "Should show Environment: label, got: {}",
        &text[..text.len().min(500)]
    );
    // Docs show: "Environment: development (default)"
    assert!(
        text.contains("(default)"),
        "Should show environment source '(default)' when no --env given, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// Shows environment label when --env is used.
#[test]
fn status_shows_environment_label() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment:"),
        "Should show Environment: label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("staging"),
        "Should show staging environment name, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// Shows Requirements: section when steps have requires.
#[test]
fn status_shows_requirements_section() {
    let temp = setup_project(REQUIRES_CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Requirements:"),
        "Should show Requirements: label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("ruby"),
        "Should show ruby requirement, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// After a failed step, status shows failure indicator ✗.
#[test]
fn status_after_failed_step() {
    let temp = setup_project(FAILED_STEP_CONFIG);
    // Phase 1: run bivvy and verify it actually failed (exit code != 0).
    // Don't use run_workflow_silently, which asserts success.
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let run_status = std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to spawn bivvy run");
    assert!(
        !run_status.success(),
        "Phase 1: bivvy run on FAILED_STEP_CONFIG should fail so status has a failure to show"
    );
    assert_eq!(
        run_status.code(),
        Some(1),
        "Phase 1: failed bivvy run should exit with code 1"
    );

    // Phase 2: bivvy status reflects the failure.
    let mut s = spawn_bivvy(&["status"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("FailTest"),
        "Should show app name after failure, got: {}",
        &text[..text.len().min(500)]
    );
    // Should show ✗ failure indicator for the bad step
    assert!(
        text.contains("✗"),
        "Should show ✗ failure indicator, got: {}",
        &text[..text.len().min(500)]
    );
    // Should show the after_failed_run hint which references the failed step.
    assert!(
        text.contains("bivvy run --only=bad"),
        "Should show hint 'bivvy run --only=bad' to re-run failed step, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

// =====================================================================
// FLAGS
// =====================================================================

/// --json outputs structured JSON with documented top-level schema:
/// `app_name`, `environment` (with `name` and `source`), and `steps` array.
#[test]
fn status_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json should exit 0"
    );

    let json = parse_pty_json(&text);

    assert_eq!(
        json["app_name"], "StatusTest",
        "JSON app_name should equal config app_name"
    );
    assert!(
        json.get("environment").is_some(),
        "JSON should have an 'environment' object"
    );
    assert!(
        json["environment"].get("name").is_some(),
        "JSON environment should have a 'name' field"
    );
    assert!(
        json["environment"].get("source").is_some(),
        "JSON environment should have a 'source' field"
    );
    let steps = json["steps"]
        .as_array()
        .expect("JSON 'steps' should be an array");
    assert_eq!(
        steps.len(),
        3,
        "Should have 3 steps in JSON steps array (deps, build, lint)"
    );
}

/// --json output includes step status as "pending" for never-run steps and
/// uses the documented step schema (name, status).
#[test]
fn status_json_structure() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json should exit 0"
    );

    let json = parse_pty_json(&text);
    let steps = json["steps"]
        .as_array()
        .expect("JSON 'steps' should be an array");

    // Every step should be pending before any run.
    for step in steps {
        assert_eq!(
            step["status"], "pending",
            "Every step should have status=pending before any run, got: {step}"
        );
        assert!(
            step["name"].is_string(),
            "Every step should have a string name, got: {step}"
        );
    }

    // Specifically, the "deps" step should be in the output.
    let names: Vec<&str> = steps
        .iter()
        .map(|s| s["name"].as_str().unwrap_or(""))
        .collect();
    assert!(
        names.contains(&"deps"),
        "Steps should include 'deps', got names: {names:?}"
    );
}

/// After a run, --json reflects success statuses and durations per docs.
#[test]
fn status_json_after_run_shows_success() {
    let temp = setup_project(CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["status", "--json"], temp.path());
    let text = read_to_eof(&mut s);
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json should exit 0"
    );

    let json = parse_pty_json(&text);
    let steps = json["steps"]
        .as_array()
        .expect("JSON 'steps' should be an array");

    // After a run, every step should be "success".
    for step in steps {
        assert_eq!(
            step["status"], "success",
            "After a successful run, every step should be success, got: {step}"
        );
    }

    // Docs specify `last_run` key on the output after a run.
    assert!(
        json.get("last_run").is_some(),
        "JSON should include 'last_run' after a run, got: {json}"
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
    // Should NOT show other steps when filtering to a single step
    assert!(
        !text.contains("lint"),
        "Should not show lint when --step deps, got: {}",
        &text[..text.len().min(300)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --step should exit 0"
    );
}

/// --env sets the target environment context.
#[test]
fn status_env_flag() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment:"),
        "Should show Environment: label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("development"),
        "Should show development environment, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --env should exit 0"
    );
}

/// --verbose shows all steps with detail.
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
    assert!(
        text.contains("Steps:"),
        "Verbose should show Steps: label, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --verbose should exit 0"
    );
}

/// --quiet produces output (status always shows).
#[test]
fn status_quiet_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--quiet"], temp.path());

    let text = read_to_eof(&mut s);
    // Even in quiet mode, status should show the app name
    assert!(
        text.contains("StatusTest"),
        "Quiet mode should still show app name, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --quiet should exit 0"
    );
}

/// --json + --env combined shows environment in JSON.
#[test]
fn status_json_with_env() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--json", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json --env should exit 0"
    );

    let json = parse_pty_json(&text);
    assert_eq!(
        json["environment"]["name"], "staging",
        "JSON --env staging should put 'staging' in environment.name"
    );
    assert_eq!(
        json["app_name"], "EnvTest",
        "JSON should still have app_name"
    );
}

/// --json + --step combined shows only the filtered step.
#[test]
fn status_json_with_step() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json", "--step", "deps"], temp.path());

    let text = read_to_eof(&mut s);
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json --step should exit 0"
    );

    let json = parse_pty_json(&text);
    let steps = json["steps"]
        .as_array()
        .expect("JSON 'steps' should be an array");
    assert_eq!(
        steps.len(),
        1,
        "JSON --step deps should have exactly one step"
    );
    assert_eq!(
        steps[0]["name"], "deps",
        "The single step should be 'deps'"
    );
}

/// Shows skipped steps (with ⊘ icon) in wrong environment.
#[test]
fn status_shows_skipped_in_wrong_env() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "production"], temp.path());

    let text = read_to_eof(&mut s);
    // setup is only for development/staging, so should be skipped in production
    assert!(
        text.contains("skipped in production"),
        "Should show 'skipped in production' for setup step, got: {}",
        &text[..text.len().min(500)]
    );
    // deploy IS available in production
    assert!(
        text.contains("deploy"),
        "Should show deploy step, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// No skipping when env matches — both steps visible in staging.
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
    assert!(
        !text.contains("skipped"),
        "No steps should be skipped in staging, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status should exit 0");
}

/// --json shows skipped status for env-filtered steps.
#[test]
fn status_json_shows_skipped_in_wrong_env() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(
        &["status", "--json", "--env", "production"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json should exit 0"
    );

    let json = parse_pty_json(&text);
    let steps = json["steps"]
        .as_array()
        .expect("JSON 'steps' should be an array");

    // Find the 'setup' step — it should be skipped in production.
    let setup_step = steps
        .iter()
        .find(|st| st["name"] == "setup")
        .expect("Should find 'setup' step in JSON output");
    assert_eq!(
        setup_step["status"], "skipped",
        "setup should be skipped in production, got: {setup_step}"
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows status command description and all documented flags.
#[test]
fn status_help() {
    let mut s = spawn_bivvy_global(&["status", "--help"]);
    let text = read_to_eof(&mut s);

    // Command description from the clap derive.
    assert!(
        text.contains("Show current setup status"),
        "Help should describe status command, got: {}",
        &text[..text.len().min(500)]
    );
    // All documented flags should appear in --help.
    assert!(
        text.contains("--json"),
        "Help should document --json flag, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("--step"),
        "Help should document --step flag, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("--env"),
        "Help should document --env flag, got: {}",
        &text[..text.len().min(500)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --help should exit 0"
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file produces error with exit code 2.
#[test]
fn status_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found"),
        "Should report 'No configuration found', got: {}",
        &text[..text.len().min(300)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 2),
        "status with no config should exit 2"
    );
}

/// --step with an unknown step name fails with exit code 1.
#[test]
fn status_unknown_step_fails() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Unknown step: ghost"),
        "Unknown step should show 'Unknown step: ghost', got: {}",
        &text[..text.len().min(300)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "status --step ghost should exit 1"
    );
}

/// --json --step with unknown step fails with exit code 1 and JSON error.
#[test]
fn status_json_unknown_step_fails() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(
        &["status", "--json", "--step", "ghost"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Unknown step: ghost"),
        "JSON unknown step should show error, got: {}",
        &text[..text.len().min(300)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "status --json --step ghost should exit 1"
    );
}

/// Empty config file still shows status with default app name.
#[test]
fn status_empty_config() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    // Empty config is still valid YAML (null), bivvy uses default app name
    assert!(
        text.contains("Bivvy Setup"),
        "Should show default app name 'Bivvy Setup' for empty config, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Steps:"),
        "Should show Steps: label even for empty config, got: {}",
        &text[..text.len().min(300)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "empty config status should exit 0"
    );
}

/// Malformed YAML config produces a parse error with exit code 1.
#[test]
fn status_malformed_yaml() {
    let temp = setup_project("{{invalid yaml::");
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Failed to parse config"),
        "Malformed YAML should show 'Failed to parse config', got: {}",
        &text[..text.len().min(300)]
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "malformed YAML should exit 1"
    );
}
