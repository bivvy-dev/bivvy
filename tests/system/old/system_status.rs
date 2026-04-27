//! System tests for `bivvy status` — all interactive, PTY-based.
//!
//! These tests verify documented behavior of the `status` command, including
//! its flags (`--json`, `--step`, `--env`), status indicators, environment
//! handling, and exit codes. See `docs/commands/status.md` for the behavior
//! under test.
#![cfg(unix)]

mod system;

use expectrl::WaitStatus;
use system::helpers::*;

/// Realistic multi-step config exercising dependencies and a check.
/// Uses real tools (`cargo`, `rustc`) rather than shell builtins so the test
/// reflects how Bivvy is actually used.
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
    command: "cargo fmt --version"
    depends_on: [build]
workflows:
  default:
    steps: [deps, build, lint]
"#;

/// Multi-environment config for verifying `--env` and skipped-step display.
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

/// Helper: extract the JSON object from PTY output (strips ANSI, trims whitespace).
fn extract_json(text: &str) -> serde_json::Value {
    let clean = text.trim();
    let start = clean.find('{').expect("JSON output should contain '{'");
    let end = clean.rfind('}').expect("JSON output should contain '}'");
    let json_str = &clean[start..=end];
    serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse JSON from status output: {e}\n---\n{json_str}\n---"))
}

// =====================================================================
// HAPPY PATH
// =====================================================================

/// `bivvy status` prints the app name, steps, and environment label,
/// and exits 0.
#[test]
fn status_shows_app_name_and_steps() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("StatusTest"),
        "status should print app name 'StatusTest', got: {text}"
    );
    assert!(
        text.contains("Steps:"),
        "status should print 'Steps:' label, got: {text}"
    );
    assert!(
        text.contains("Environment:"),
        "status should print 'Environment:' label, got: {text}"
    );
    for name in ["deps", "build", "lint"] {
        assert!(
            text.contains(name),
            "status should show step '{name}', got: {text}"
        );
    }

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status should exit 0 with a valid config"
    );
}

/// Before any run, every step shows the `◌` pending indicator (documented in
/// docs/commands/status.md).
#[test]
fn status_shows_pending_indicator_before_run() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains('◌'),
        "status should show '◌' pending indicator for never-run steps, got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0));
}

// =====================================================================
// FLAGS
// =====================================================================

/// `--env development` resolves and displays the `development` environment.
#[test]
fn status_env_flag_shows_resolved_environment() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment:"),
        "status --env should print 'Environment:' label, got: {text}"
    );
    assert!(
        text.contains("development"),
        "status --env development should show 'development' environment name, got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0), "status --env should exit 0");
}

/// `--json` emits structured JSON with the documented schema (app_name,
/// environment, steps). The JSON is parsed and the structure verified.
#[test]
fn status_json_flag_emits_documented_schema() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json = extract_json(&text);

    assert_eq!(
        json["app_name"], "StatusTest",
        "JSON app_name should be 'StatusTest', got: {json}"
    );
    assert!(
        json["environment"].is_object(),
        "JSON should have an 'environment' object, got: {json}"
    );
    assert!(
        json["environment"]["name"].is_string(),
        "JSON environment.name should be a string, got: {json}"
    );

    let steps = json["steps"]
        .as_array()
        .expect("JSON steps should be an array");
    assert_eq!(steps.len(), 3, "should list all 3 steps, got: {json}");

    let step_names: Vec<&str> = steps
        .iter()
        .map(|s| s["name"].as_str().expect("step name should be a string"))
        .collect();
    assert!(step_names.contains(&"deps"));
    assert!(step_names.contains(&"build"));
    assert!(step_names.contains(&"lint"));

    // Before any run, all steps are "pending" (documented statuses:
    // success | failed | pending | skipped).
    for step in steps {
        assert_eq!(
            step["status"], "pending",
            "all steps should be 'pending' before any run, got: {step}"
        );
    }

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --json should exit 0"
    );
}

/// `--verbose` shows all steps and the Steps: label, exiting 0.
#[test]
fn status_verbose_flag_shows_all_steps() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--verbose"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps:"),
        "status --verbose should print 'Steps:' label, got: {text}"
    );
    for name in ["deps", "build", "lint"] {
        assert!(
            text.contains(name),
            "status --verbose should show step '{name}', got: {text}"
        );
    }

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --verbose should exit 0"
    );
}

/// `--quiet` still prints the app name (status always displays its summary)
/// and exits 0.
#[test]
fn status_quiet_flag_still_shows_app_name() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--quiet"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("StatusTest"),
        "status --quiet should still show app name, got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --quiet should exit 0"
    );
}

/// `--step <name>` filters output to a single step.
#[test]
fn status_step_flag_filters_to_one_step() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "deps"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("deps"),
        "status --step deps should show 'deps', got: {text}"
    );
    assert!(
        !text.contains("lint"),
        "status --step deps should NOT include 'lint', got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "status --step should exit 0"
    );
}

/// Steps restricted by `only_environments` show as "skipped in <env>" when
/// the current env doesn't match (documented `⊘` indicator).
#[test]
fn status_shows_skipped_in_wrong_environment() {
    let temp = setup_project(MULTI_ENV_CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "production"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("skipped in production"),
        "setup should be 'skipped in production', got: {text}"
    );
    assert!(
        text.contains("deploy"),
        "deploy (allowed in production) should still be listed, got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(status, WaitStatus::Exited(pid, 0));
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file: prints "No configuration found" and exits with code 2.
#[test]
fn status_no_config_fails_with_exit_2() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found"),
        "status without config should show 'No configuration found', got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 2),
        "status without config should exit 2 (config error)"
    );
}

/// `--step` with an unknown name: prints "Unknown step: <name>" and exits 1.
#[test]
fn status_unknown_step_fails_with_exit_1() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Unknown step: ghost"),
        "status --step ghost should report 'Unknown step: ghost', got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "status --step with unknown name should exit 1"
    );
}

/// `--json --step` with an unknown name: emits a JSON error and exits 1.
#[test]
fn status_json_unknown_step_fails_with_exit_1() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["status", "--json", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Unknown step: ghost"),
        "status --json --step ghost should report 'Unknown step: ghost', got: {text}"
    );

    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "status --json --step with unknown name should exit 1"
    );
}
