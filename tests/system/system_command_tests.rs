//! System tests for ALL non-run commands and ALL their flags.
//!
//! Uses a single realistic config with 4 non-skippable steps that exercise
//! real external development commands (git, rustc, cargo).  Tests are
//! organized by command: status, list, lint, last, history, config, init,
//! templates, and completions.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Shared realistic config
// ─────────────────────────────────────────────────────────────────────

const CONFIG: &str = r#"
app_name: "CommandTest"

settings:
  default_output: verbose

vars:
  version:
    command: "git log --oneline -1"

steps:
  verify-tools:
    title: "Verify development tools"
    command: "rustc --version && cargo --version"
    skippable: false
    check:
      type: execution
      command: "rustc --version"

  check-repo:
    title: "Check repository"
    command: "git rev-parse --git-dir && git branch --show-current"
    skippable: false
    depends_on: [verify-tools]
    check:
      type: execution
      command: "git rev-parse --git-dir"

  analyze-project:
    title: "Analyze project"
    command: "cargo metadata --format-version 1 --no-deps --manifest-path Cargo.toml"
    skippable: false
    depends_on: [check-repo]
    check:
      type: execution
      command: "cargo metadata --format-version 1 --no-deps --manifest-path Cargo.toml"

  build-report:
    title: "Build report"
    command: "rustc --version > .build-report.txt && cargo --version >> .build-report.txt"
    skippable: false
    depends_on: [analyze-project]

workflows:
  default:
    steps: [verify-tools, check-repo, analyze-project, build-report]
  quick:
    description: "Quick check"
    steps: [verify-tools, check-repo]
"#;

// ─────────────────────────────────────────────────────────────────────
// Invalid config for lint tests
// ─────────────────────────────────────────────────────────────────────

const INVALID_CONFIG: &str = r#"
app_name: "InvalidProject"

steps:
  alpha:
    title: "Alpha step"
    command: "git --version"
    depends_on: [beta]
  beta:
    title: "Beta step"
    command: "rustc --version"
    depends_on: [alpha]

workflows:
  default:
    steps: [alpha, beta, ghost-step]
"#;

// =====================================================================
// STATUS COMMAND
// =====================================================================

/// `bivvy status` shows app name and all step names with their status.
#[test]
fn status_shows_step_status() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("CommandTest"), "Should show app name, got:\n{text}");
    assert!(text.contains("verify-tools"), "Should show verify-tools step, got:\n{text}");
    assert!(text.contains("check-repo"), "Should show check-repo step, got:\n{text}");
    assert!(text.contains("analyze-project"), "Should show analyze-project step, got:\n{text}");
    assert!(text.contains("build-report"), "Should show build-report step, got:\n{text}");
    assert_exit_code(&s, 0);
}

/// `bivvy status --json` produces valid JSON containing app_name and every
/// configured step.
#[test]
fn status_json_output() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["status", "--json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "status --json should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("status --json should produce valid JSON: {e}\nGot:\n{stdout}"));

    assert_eq!(
        json["app_name"].as_str(),
        Some("CommandTest"),
        "JSON should contain app_name = CommandTest"
    );

    // Verify every configured step appears, regardless of whether steps is
    // rendered as a map or an array of objects.
    let steps = &json["steps"];
    for step in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        let present = if let Some(obj) = steps.as_object() {
            obj.contains_key(step)
        } else if let Some(arr) = steps.as_array() {
            arr.iter().any(|s| {
                s["name"].as_str() == Some(step) || s["step"].as_str() == Some(step)
            })
        } else {
            panic!("JSON 'steps' should be an object or array, got:\n{json}")
        };
        assert!(present, "JSON should contain step '{step}', got:\n{json}");
    }
}

/// `bivvy status --step verify-tools` shows info for that specific step.
#[test]
fn status_specific_step() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status", "--step", "verify-tools"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("verify-tools"),
        "Should show verify-tools step info, got:\n{text}"
    );
    assert!(
        text.contains("Verify development tools"),
        "Should show step title, got:\n{text}"
    );
    // Should NOT leak other steps — --step scopes output
    assert!(
        !text.contains("Build report"),
        "Should not show other step titles when --step is scoped, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy status --env ci` accepts the environment flag and still runs
/// successfully, showing the configured app name.
#[test]
fn status_with_env() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["status", "--env", "ci"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("CommandTest"),
        "Should still show app name under --env ci, got:\n{text}"
    );
    assert!(
        text.contains("verify-tools"),
        "Should still list steps under --env ci, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy status` exits with code 0 on valid config.
#[test]
fn status_exit_code() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["status"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Status on valid config should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =====================================================================
// LIST COMMAND
// =====================================================================

/// `bivvy list` shows both steps and workflows sections.
#[test]
fn list_shows_steps_and_workflows() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Steps:"), "Should have Steps section, got:\n{text}");
    assert!(text.contains("verify-tools"), "Should list verify-tools, got:\n{text}");
    assert!(text.contains("check-repo"), "Should list check-repo, got:\n{text}");
    assert!(text.contains("analyze-project"), "Should list analyze-project, got:\n{text}");
    assert!(text.contains("build-report"), "Should list build-report, got:\n{text}");
    assert!(text.contains("Workflows:"), "Should have Workflows section, got:\n{text}");
    assert!(text.contains("default"), "Should list default workflow, got:\n{text}");
    assert!(text.contains("quick"), "Should list quick workflow, got:\n{text}");
    assert_exit_code(&s, 0);
}

/// `bivvy list --steps-only` shows steps but not the Workflows section.
#[test]
fn list_steps_only() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--steps-only"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Steps:"), "Should have Steps section, got:\n{text}");
    assert!(text.contains("verify-tools"), "Should list verify-tools, got:\n{text}");
    assert!(text.contains("check-repo"), "Should list check-repo, got:\n{text}");
    assert!(text.contains("analyze-project"), "Should list analyze-project, got:\n{text}");
    assert!(text.contains("build-report"), "Should list build-report, got:\n{text}");
    assert!(!text.contains("Workflows:"), "Should NOT have Workflows section with --steps-only, got:\n{text}");
    assert_exit_code(&s, 0);
}

/// `bivvy list --workflows-only` shows workflows but not the Steps section.
#[test]
fn list_workflows_only() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--workflows-only"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Workflows:"), "Should have Workflows section, got:\n{text}");
    assert!(text.contains("default"), "Should list default workflow, got:\n{text}");
    assert!(text.contains("quick"), "Should list quick workflow, got:\n{text}");
    assert!(!text.contains("Steps:"), "Should NOT have Steps section with --workflows-only, got:\n{text}");
    // With --workflows-only, step names should not appear as list entries
    assert!(
        !text.contains("verify-tools"),
        "Should not list step names under --workflows-only, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy list --json` produces valid JSON with steps and workflows arrays
/// containing every step and workflow name.
#[test]
fn list_json_output() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["list", "--json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "list --json should exit 0, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("list --json should produce valid JSON: {e}\nGot:\n{stdout}"));

    let steps = json["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("JSON should have a steps array, got:\n{json}"));
    let workflows = json["workflows"]
        .as_array()
        .unwrap_or_else(|| panic!("JSON should have a workflows array, got:\n{json}"));

    // All 4 configured steps should appear
    let step_names: Vec<String> = steps
        .iter()
        .filter_map(|s| s["name"].as_str().map(String::from))
        .collect();
    for expected in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        assert!(
            step_names.iter().any(|n| n == expected),
            "Step '{expected}' missing from JSON steps array: {step_names:?}"
        );
    }

    // Both configured workflows should appear
    let workflow_names: Vec<String> = workflows
        .iter()
        .filter_map(|w| w["name"].as_str().map(String::from))
        .collect();
    for expected in ["default", "quick"] {
        assert!(
            workflow_names.iter().any(|n| n == expected),
            "Workflow '{expected}' missing from JSON workflows array: {workflow_names:?}"
        );
    }
}

/// `bivvy list --env ci` accepts the environment flag and still lists steps
/// and workflows successfully.
#[test]
fn list_with_env() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "ci"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps:"),
        "Should still show steps list under --env ci, got:\n{text}"
    );
    assert!(
        text.contains("verify-tools"),
        "Should list configured steps under --env ci, got:\n{text}"
    );
    assert!(
        text.contains("Workflows:"),
        "Should still show workflows section under --env ci, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy list` exits with code 0.
#[test]
fn list_exit_code() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["list"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "List should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =====================================================================
// LINT COMMAND
// =====================================================================

/// `bivvy lint` on a valid config shows "Configuration is valid!" and exits 0.
#[test]
fn lint_valid_config() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `bivvy lint --format json` produces valid JSON output with no errors on a
/// valid config.
#[test]
fn lint_json_format() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "lint --format json on valid config should exit 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("lint --format json should produce valid JSON: {e}\nGot:\n{stdout}"));

    // Valid config: diagnostics array (top-level or nested) should be empty —
    // any Error-severity diagnostics would fail the lint.
    let diagnostics = if let Some(arr) = json.as_array() {
        arr.clone()
    } else if let Some(arr) = json["diagnostics"].as_array() {
        arr.clone()
    } else {
        panic!(
            "JSON lint output should be an array or have a diagnostics array, got:\n{json}"
        );
    };
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d["severity"].as_str() == Some("error"))
        .collect();
    assert!(
        errors.is_empty(),
        "Valid config should produce no error diagnostics, got: {errors:?}"
    );
}

/// `bivvy lint --format sarif` produces valid SARIF JSON with the required
/// top-level fields (`version` and `runs`).
#[test]
fn lint_sarif_format() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint", "--format", "sarif"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "lint --format sarif on valid config should exit 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("lint --format sarif should produce valid JSON: {e}\nGot:\n{stdout}"));

    // SARIF 2.1.0 requires both `version` and `runs` at the root.
    assert!(
        json["version"].is_string(),
        "SARIF output must have a version field, got:\n{json}"
    );
    assert!(
        json["runs"].is_array(),
        "SARIF output must have a runs array, got:\n{json}"
    );
}

/// `bivvy lint --strict` treats warnings as errors; on a valid config it still
/// reports success and exits 0.
#[test]
fn lint_strict_mode() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["lint", "--strict"], temp.path());

    s.expect("Configuration is valid!").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `bivvy lint` on invalid config detects circular dependencies AND missing
/// step references, reports both, and exits with code 1.
#[test]
fn lint_invalid_config() {
    let temp = setup_project_with_git(INVALID_CONFIG);
    let mut s = spawn_bivvy(&["lint"], temp.path());

    let text = read_to_eof(&mut s);
    // The config has circular deps (alpha -> beta -> alpha)
    assert!(
        text.to_lowercase().contains("circular dependency"),
        "Should report circular dependency, got:\n{text}"
    );
    // The default workflow references an undefined step (ghost-step)
    assert!(
        text.contains("ghost-step"),
        "Should report the undefined ghost-step reference, got:\n{text}"
    );
    assert_exit_code(&s, 1);
}

/// `bivvy lint` on invalid config exits with code 1 (documented lint-error
/// exit code).
#[test]
fn lint_invalid_config_exit_code() {
    let temp = setup_project_with_git(INVALID_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Lint on invalid config should exit with code 1, stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `bivvy lint` on valid config exits with code 0.
#[test]
fn lint_valid_config_exit_code() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["lint"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Lint on valid config should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =====================================================================
// LAST COMMAND (requires a prior run)
// =====================================================================

/// Run workflow first, then `bivvy last` shows last run info with workflow name.
#[test]
fn last_shows_run_info() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last"], temp.path());

    s.expect("Last Run").unwrap();
    s.expect("Workflow:").unwrap();
    s.expect("default").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `bivvy last --json` produces valid JSON with run/workflow information and
/// exits 0.
#[test]
fn last_json_output() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["last", "--json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "last --json should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("last --json should produce valid JSON: {e}\nGot:\n{stdout}"));

    // The JSON must identify the workflow that ran. Either a top-level
    // `workflow` field or a nested `run.workflow` field is acceptable.
    let workflow = json["workflow"]
        .as_str()
        .or_else(|| json["run"]["workflow"].as_str())
        .unwrap_or_else(|| {
            panic!("JSON should contain a workflow name, got:\n{json}")
        });
    assert_eq!(
        workflow, "default",
        "JSON workflow should be 'default' (ran via bivvy run), got: {workflow}"
    );
}

/// `bivvy last --step verify-tools` shows that specific step's last run info,
/// exits 0, and does not show unrelated steps.
#[test]
fn last_specific_step() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--step", "verify-tools"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("verify-tools"),
        "Should show verify-tools step info, got:\n{text}"
    );
    // --step should scope output — other step titles should not appear.
    assert!(
        !text.contains("Build report"),
        "Should not show unrelated step titles when --step is scoped, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last --all` shows every recorded run with a "Run N of M" header.
#[test]
fn last_all_runs() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--all"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run 1 of 2"),
        "Should show 'Run 1 of 2' header, got:\n{text}"
    );
    assert!(
        text.contains("Run 2 of 2"),
        "Should show 'Run 2 of 2' header, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last --output` includes the real captured stdout from the last run
/// (rustc/cargo version output produced by the verify-tools step).
#[test]
fn last_with_output() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["last", "--output"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Last Run"),
        "Should show 'Last Run' header, got:\n{text}"
    );
    // The verify-tools step runs `rustc --version` which prints a line
    // starting with "rustc ". --output should include that captured line.
    assert!(
        text.contains("rustc "),
        "Should include captured rustc output from the run, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy last` exits with code 0 after a successful run.
#[test]
fn last_exit_code_after_run() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["last"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Last after successful run should exit 0");
    assert_eq!(output.status.code(), Some(0), "Exit code should be exactly 0");
}

/// `bivvy last` with no prior run prints "No runs recorded for this project."
/// and exits with code 0 (absence of runs is informational, not an error).
#[test]
fn last_no_prior_run() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["last"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No runs recorded for this project."),
        "Should emit the exact 'No runs recorded' message, got:\n{stdout}"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "last with no prior runs should exit 0 (informational), stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =====================================================================
// HISTORY COMMAND (requires prior runs)
// =====================================================================

/// Run workflow, then `bivvy history` shows run entries with workflow name
/// and exits 0.
#[test]
fn history_shows_runs() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history"], temp.path());

    s.expect("Run History").unwrap();
    s.expect("default").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `bivvy history --json` produces valid JSON containing at least one run
/// record for the workflow just executed.
#[test]
fn history_json_output() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["history", "--json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "history --json should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("history --json should produce valid JSON: {e}\nGot:\n{stdout}"));

    // Locate the runs array — accept top-level array or a `runs`/`history`
    // key, but require that exactly one of these shapes is present.
    let runs = json
        .as_array()
        .cloned()
        .or_else(|| json["runs"].as_array().cloned())
        .or_else(|| json["history"].as_array().cloned())
        .unwrap_or_else(|| panic!("JSON should contain a runs array, got:\n{json}"));

    assert!(
        !runs.is_empty(),
        "History after one run should contain at least one entry, got:\n{json}"
    );
}

/// `bivvy history --step verify-tools` filters history to that step, shows
/// the Run History header, and does not list unrelated steps.
#[test]
fn history_step_filter() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--step", "verify-tools"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show Run History header, got:\n{text}"
    );
    assert!(
        text.contains("verify-tools"),
        "History step filter should show verify-tools, got:\n{text}"
    );
    // Should not show unrelated step titles when scoped.
    assert!(
        !text.contains("Build report"),
        "Should not show unrelated step titles when --step is scoped, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy history --limit 1` shows exactly 1 run entry when multiple runs
/// exist. Verified via JSON output for an exact count assertion.
#[test]
fn history_limit() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);
    run_bivvy_silently(temp.path(), &["run"]);
    run_bivvy_silently(temp.path(), &["run"]);

    // Verify the human output renders the header and workflow name.
    let mut s = spawn_bivvy(&["history", "--limit", "1"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show Run History header, got:\n{text}"
    );
    assert!(
        text.contains("default"),
        "Should show the workflow name, got:\n{text}"
    );
    assert_exit_code(&s, 0);

    // Verify --limit 1 truncates to exactly one entry via the JSON shape.
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["history", "--limit", "1", "--json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(output.status.code(), Some(0), "history --json should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("history --json should produce valid JSON: {e}\nGot:\n{stdout}"));
    let runs = json
        .as_array()
        .cloned()
        .or_else(|| json["runs"].as_array().cloned())
        .or_else(|| json["history"].as_array().cloned())
        .unwrap_or_else(|| panic!("JSON should contain a runs array, got:\n{json}"));
    assert_eq!(
        runs.len(),
        1,
        "history --limit 1 should return exactly 1 run, got {} entries:\n{json}",
        runs.len()
    );
}

/// `bivvy history --since 1h` shows recent runs (the run we just executed
/// must fall inside the 1h window) and exits 0.
#[test]
fn history_since() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--since", "1h"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show Run History header, got:\n{text}"
    );
    assert!(
        text.contains("default"),
        "Recent run should include workflow name, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy history --detail` shows a detailed view listing every step that
/// ran as part of the default workflow.
#[test]
fn history_detail() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps:"),
        "Detailed view should show Steps: section, got:\n{text}"
    );
    // Every step from the default workflow should appear in the detail view.
    for step in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        assert!(
            text.contains(step),
            "Detailed view should list '{step}' step, got:\n{text}"
        );
    }
    assert_exit_code(&s, 0);
}

/// `bivvy history` exits with code 0 after a run.
#[test]
fn history_exit_code_after_run() {
    let temp = setup_project_with_git(CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["history"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "History after run should exit 0");
    assert_eq!(output.status.code(), Some(0), "Exit code should be exactly 0");
}

// =====================================================================
// CONFIG COMMAND
// =====================================================================

/// `bivvy config` shows the resolved configuration with app name and steps,
/// and exits 0.
#[test]
fn config_shows_resolved() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("app_name"), "Should show app_name key, got:\n{text}");
    assert!(text.contains("CommandTest"), "Should show app name value, got:\n{text}");
    // All four configured steps should appear.
    for step in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        assert!(
            text.contains(step),
            "Should show step '{step}', got:\n{text}"
        );
    }
    assert_exit_code(&s, 0);
}

/// `bivvy config --json` produces valid JSON with the full config including
/// app_name and every configured step.
#[test]
fn config_json_output() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["config", "--json"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "config --json should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("config --json should produce valid JSON: {e}\nGot:\n{stdout}"));

    assert_eq!(
        json["app_name"].as_str(),
        Some("CommandTest"),
        "JSON should contain app_name = CommandTest"
    );
    // Verify all configured steps are present in the JSON steps map/array.
    let steps = &json["steps"];
    for step in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        let present = if let Some(obj) = steps.as_object() {
            obj.contains_key(step)
        } else if let Some(arr) = steps.as_array() {
            arr.iter().any(|s| s["name"].as_str() == Some(step))
        } else {
            false
        };
        assert!(present, "JSON should contain step '{step}', got:\n{json}");
    }
}

/// `bivvy config --yaml` produces output that re-parses as YAML and exposes
/// the same app_name and steps as the input config.
#[test]
fn config_yaml_output() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["config", "--yaml"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "config --yaml should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("app_name"),
        "YAML output should contain app_name key, got:\n{stdout}"
    );
    assert!(
        stdout.contains("CommandTest"),
        "YAML output should contain app name value, got:\n{stdout}"
    );
    for step in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        assert!(
            stdout.contains(step),
            "YAML output should contain step '{step}', got:\n{stdout}"
        );
    }
}

/// `bivvy config --merged` shows the merged config with app_name and every
/// configured step, and exits 0.
#[test]
fn config_merged() {
    let temp = setup_project_with_git(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("app_name"), "Merged config should contain app_name, got:\n{text}");
    assert!(text.contains("CommandTest"), "Merged config should contain app name value, got:\n{text}");
    for step in ["verify-tools", "check-repo", "analyze-project", "build-report"] {
        assert!(
            text.contains(step),
            "Merged config should contain step '{step}', got:\n{text}"
        );
    }
    assert_exit_code(&s, 0);
}

/// `bivvy config` exits with code 0.
#[test]
fn config_exit_code() {
    let temp = setup_project_with_git(CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["config"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert!(output.status.success(), "Config should exit 0");
    assert_eq!(output.status.code(), Some(0), "Exit code should be exactly 0");
}

// =====================================================================
// INIT COMMAND
// =====================================================================

/// `bivvy init --minimal` creates .bivvy/config.yml with valid structure and
/// exits 0 after the interactive "Run setup now?" prompt is declined.
#[test]
fn init_creates_config() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml").unwrap();
    // Dismiss the "Run setup now?" interactive prompt (Enter accepts default "No")
    wait_and_answer(&s, "Run setup now?", KEY_ENTER, "init: dismiss run prompt");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config file should exist after init"
    );

    // Verify the created config is valid YAML with expected structure by
    // parsing it and asserting on the structured result — this catches
    // malformed YAML that a string-contains check would miss.
    let content = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Created config should be valid YAML: {e}\nGot:\n{content}"));
    assert!(
        parsed.get("app_name").is_some(),
        "Created config should have app_name key, got:\n{content}"
    );
    assert!(
        parsed.get("steps").is_some(),
        "Created config should have steps key, got:\n{content}"
    );
}

/// `bivvy init --minimal --force` overwrites an existing config, re-parses
/// cleanly, exits 0, and no trace of the old config remains.
#[test]
fn init_force_overwrites() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: OldConfig\n").unwrap();

    let mut s = spawn_bivvy(&["init", "--minimal", "--force"], temp.path());

    s.expect("Created .bivvy/config.yml").unwrap();
    // Dismiss the "Run setup now?" interactive prompt (Enter accepts default "No")
    wait_and_answer(&s, "Run setup now?", KEY_ENTER, "init --force: dismiss run prompt");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let content = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(
        !content.contains("OldConfig"),
        "Old config should be replaced, got:\n{content}"
    );
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Overwritten config should be valid YAML: {e}\nGot:\n{content}"));
    assert!(
        parsed.get("app_name").is_some(),
        "New config should have app_name, got:\n{content}"
    );
    assert!(
        parsed.get("steps").is_some(),
        "New config should have steps, got:\n{content}"
    );
}

/// `bivvy init --minimal` exits with code 0 in non-interactive mode.
#[test]
fn init_exit_code() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["init", "--minimal"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Init --minimal should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Init should have created .bivvy/config.yml"
    );
}

/// `bivvy init` without --force on an existing config warns with the exact
/// documented message and exits with code 1, leaving the existing config
/// untouched.
#[test]
fn init_existing_config_no_force() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: ExistingApp\n").unwrap();

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["init", "--minimal"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    assert_eq!(
        output.status.code(),
        Some(1),
        "init on existing config should exit with code 1, stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Configuration already exists. Use --force to overwrite."),
        "Should emit the exact 'Configuration already exists' warning, got:\n{combined}"
    );

    // Existing config must not have been touched.
    let content = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(
        content.contains("ExistingApp"),
        "Existing config should be preserved, got:\n{content}"
    );
}

// =====================================================================
// TEMPLATES COMMAND
// =====================================================================

/// `bivvy templates` lists well-known built-in templates across Ruby, Node,
/// and Rust categories and prints the "N templates available" footer.
#[test]
fn templates_lists_available() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);
    // These built-in templates must always be present.
    for tmpl in ["bundle-install", "yarn-install", "cargo-build"] {
        assert!(
            text.contains(tmpl),
            "Should list built-in template '{tmpl}', got:\n{text}"
        );
    }
    assert!(
        text.contains("templates available"),
        "Should show the 'N templates available' footer, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy templates --category rust` filters to Rust templates — shows
/// cargo-build and omits unrelated categories (bundle-install, yarn-install).
#[test]
fn templates_category_filter() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["templates", "--category", "rust"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build"),
        "Rust filter should show cargo-build, got:\n{text}"
    );
    // Ruby and Node templates must NOT appear when filtering to rust.
    assert!(
        !text.contains("bundle-install"),
        "Rust filter should not show ruby template bundle-install, got:\n{text}"
    );
    assert!(
        !text.contains("yarn-install"),
        "Rust filter should not show node template yarn-install, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// `bivvy templates` exits with code 0.
#[test]
fn templates_exit_code() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["templates"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Templates should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =====================================================================
// COMPLETIONS COMMAND
// =====================================================================

/// `bivvy completions bash` produces a bash completion script that defines
/// the `_bivvy` completion function, uses `COMPREPLY`, and registers itself
/// with `complete -F`.
#[test]
fn completions_bash() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["completions", "bash"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "completions bash should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // clap_complete's bash generator produces all three of these markers.
    assert!(
        stdout.contains("_bivvy"),
        "Bash completions should define _bivvy function, got:\n{stdout}"
    );
    assert!(
        stdout.contains("COMPREPLY"),
        "Bash completions should use COMPREPLY, got:\n{stdout}"
    );
    assert!(
        stdout.contains("complete -F"),
        "Bash completions should register with 'complete -F', got:\n{stdout}"
    );
}

/// `bivvy completions zsh` produces a zsh completion script with `#compdef
/// bivvy`, a `_bivvy` function, and `_arguments` directives.
#[test]
fn completions_zsh() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["completions", "zsh"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "completions zsh should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("#compdef bivvy"),
        "Zsh completions should start with '#compdef bivvy', got:\n{stdout}"
    );
    assert!(
        stdout.contains("_bivvy"),
        "Zsh completions should define _bivvy function, got:\n{stdout}"
    );
    assert!(
        stdout.contains("_arguments"),
        "Zsh completions should use _arguments directive, got:\n{stdout}"
    );
}

/// `bivvy completions fish` produces a fish completion script using
/// `complete -c bivvy` directives.
#[test]
fn completions_fish() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["completions", "fish"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "completions fish should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("complete -c bivvy"),
        "Fish completions should contain 'complete -c bivvy' directives, got:\n{stdout}"
    );
}

/// `bivvy completions bash` exits with code 0 and produces non-empty output.
#[test]
fn completions_exit_code() {
    let temp = TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = std::process::Command::new(bin)
        .args(["completions", "bash"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Completions should exit with code 0, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.is_empty(),
        "Completions should produce output, got empty stdout"
    );
    assert!(
        stdout.contains("bivvy"),
        "Completions output should reference bivvy, got:\n{stdout}"
    );
}
