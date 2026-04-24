//! System tests for `bivvy history` — all interactive, PTY-based.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const SIMPLE_CONFIG: &str = r#"
app_name: "HistoryTest"
steps:
  greet:
    title: "Say hello"
    command: "git --version"
    skippable: false
  farewell:
    title: "Say goodbye"
    command: "rustc --version"
    skippable: false
    depends_on: [greet]
workflows:
  default:
    steps: [greet, farewell]
"#;

const MULTI_WORKFLOW_CONFIG: &str = r#"
app_name: "MultiHistory"
steps:
  build:
    title: "Build"
    command: "rustc --version"
    skippable: false
  test:
    title: "Test"
    command: "cargo fmt --version"
    skippable: false
    depends_on: [build]
  deploy:
    title: "Deploy"
    command: "git --version"
    skippable: false
    depends_on: [build, test]
workflows:
  default:
    steps: [build, test]
  release:
    steps: [build, test, deploy]
"#;

const FAILING_CONFIG: &str = r#"
app_name: "FailHistory"
steps:
  good:
    title: "Good step"
    command: "git --version"
    skippable: false
  bad:
    title: "Bad step"
    command: "git --no-such-flag-xyz"
    skippable: false
    depends_on: [good]
workflows:
  default:
    steps: [good, bad]
"#;

// =====================================================================
// HAPPY PATH
// =====================================================================

/// No runs yet shows "No run history" message.
#[test]
fn history_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history for this project."),
        "Should show 'No run history for this project.' message, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// After a single run, history shows the run with workflow name and step count.
#[test]
fn history_after_run() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show workflow name 'default', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("2 steps"),
        "Should show '2 steps' for the two-step workflow, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("✓"),
        "Successful run should show success indicator '✓', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --limit restricts number of runs shown.
///
/// Uses `--json` to count entries unambiguously and also verifies the
/// plain-text header appears so both rendering paths are exercised.
#[test]
fn history_limit_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());

    // Plain-text view: header must render.
    let mut s = spawn_bivvy(&["history", "--limit", "1"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);

    // JSON view: exactly one entry must be returned.
    let mut s = spawn_bivvy(&["history", "--limit", "1", "--json"], temp.path());
    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let parsed: serde_json::Value =
        serde_json::from_str(&text[json_start..=json_end]).expect("valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");
    assert_eq!(
        arr.len(),
        1,
        "--limit 1 should return exactly 1 entry, got {}",
        arr.len()
    );
    assert_exit_code(&s, 0);
}

/// History is returned newest-first.
///
/// Runs the default workflow on two different configs (changing the
/// workflow name between runs) so we can distinguish the two records and
/// verify ordering.
#[test]
fn history_is_newest_first() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    // First record: default workflow.
    run_bivvy_silently(temp.path(), &["run"]);
    // Second (newer) record: release workflow.
    run_bivvy_silently(temp.path(), &["run", "--workflow", "release"]);

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());
    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let parsed: serde_json::Value =
        serde_json::from_str(&text[json_start..=json_end]).expect("valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 2, "expected 2 history entries");

    // Newest-first: release came last, so it is the first entry.
    assert_eq!(
        arr[0]["workflow"].as_str(),
        Some("release"),
        "first entry should be the most recent (release)"
    );
    assert_eq!(
        arr[1]["workflow"].as_str(),
        Some("default"),
        "second entry should be the older (default)"
    );
    assert_exit_code(&s, 0);
}

/// --detail shows step names for each run.
#[test]
fn history_detail_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps:"),
        "Detail flag should show 'Steps:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("greet"),
        "Detail should list 'greet' step, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("farewell"),
        "Detail should list 'farewell' step, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --json produces valid JSON output with expected fields.
#[test]
fn history_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("JSON output should be valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 1, "Should have exactly 1 run entry");
    let entry = &arr[0];

    // Verify workflow name.
    assert_eq!(
        entry["workflow"].as_str(),
        Some("default"),
        "JSON entry should have workflow 'default'"
    );

    // Verify steps_run is an array containing both steps from SIMPLE_CONFIG.
    let steps_run = entry["steps_run"]
        .as_array()
        .expect("steps_run should be an array");
    let step_names: Vec<&str> = steps_run.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        step_names.contains(&"greet"),
        "steps_run should contain 'greet', got {step_names:?}"
    );
    assert!(
        step_names.contains(&"farewell"),
        "steps_run should contain 'farewell', got {step_names:?}"
    );

    // Verify duration_ms is a non-negative number.
    let duration_ms = entry["duration_ms"]
        .as_u64()
        .expect("duration_ms should be a u64");
    // No upper bound assert — CI timing varies — but it must be present and numeric.
    let _ = duration_ms;

    // Verify status is present and represents a successful run.
    let status = entry["status"]
        .as_str()
        .expect("status should be a string");
    assert_eq!(
        status, "Success",
        "status should be 'Success' for a successful run, got {status:?}"
    );

    // Verify timestamp is a non-empty string (ISO-8601).
    let timestamp = entry["timestamp"]
        .as_str()
        .expect("timestamp should be a string");
    assert!(
        !timestamp.is_empty(),
        "timestamp should not be empty"
    );

    assert_exit_code(&s, 0);
}

/// --since filters to recent runs within the time window.
#[test]
fn history_since_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--since", "1h"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header for recent run, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show the recent run's workflow, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// --step filters history to runs that include the named step.
#[test]
fn history_step_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "greet"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Step filter should show 'Run History' header when step was run, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Step filter should show the matching run, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// Running multiple workflows produces history entries for each workflow name.
///
/// Uses `MULTI_WORKFLOW_CONFIG` which declares two workflows (`default` and
/// `release`) so we can verify that history tracks workflow identity across runs.
#[test]
fn history_tracks_multiple_workflows() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);

    // Run the default workflow (build, test) and the release workflow
    // (build, test, deploy).  `run_bivvy_silently` asserts both succeed.
    run_bivvy_silently(temp.path(), &["run"]);
    run_bivvy_silently(temp.path(), &["run", "--workflow", "release"]);

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("JSON output should be valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");

    // Both runs recorded.
    assert_eq!(arr.len(), 2, "Should have exactly 2 run entries");

    // Collect workflow names (ignoring order — newest-first is a display concern).
    let workflows: Vec<&str> = arr
        .iter()
        .filter_map(|e| e["workflow"].as_str())
        .collect();
    assert!(
        workflows.contains(&"default"),
        "history should record the 'default' workflow, got {workflows:?}"
    );
    assert!(
        workflows.contains(&"release"),
        "history should record the 'release' workflow, got {workflows:?}"
    );

    // The release run must include 'deploy' in steps_run; default must not.
    let release_entry = arr
        .iter()
        .find(|e| e["workflow"].as_str() == Some("release"))
        .expect("release entry should exist");
    let release_steps: Vec<&str> = release_entry["steps_run"]
        .as_array()
        .expect("release steps_run should be an array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        release_steps.contains(&"deploy"),
        "release run should have executed 'deploy', got {release_steps:?}"
    );

    let default_entry = arr
        .iter()
        .find(|e| e["workflow"].as_str() == Some("default"))
        .expect("default entry should exist");
    let default_steps: Vec<&str> = default_entry["steps_run"]
        .as_array()
        .expect("default steps_run should be an array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        !default_steps.contains(&"deploy"),
        "default run should NOT have executed 'deploy', got {default_steps:?}"
    );

    assert_exit_code(&s, 0);
}

/// `--step deploy` filters to only the workflows that ran that step.
///
/// Verifies that `--step` does not just return "any run exists" — it must
/// actually filter so that a step only present in one workflow produces
/// exactly one history entry.
#[test]
fn history_step_filter_isolates_workflow() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    run_bivvy_silently(temp.path(), &["run"]);
    run_bivvy_silently(temp.path(), &["run", "--workflow", "release"]);

    let mut s = spawn_bivvy(&["history", "--step", "deploy", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("JSON output should be valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");

    assert_eq!(
        arr.len(),
        1,
        "--step deploy should match only the release run, got {} entries",
        arr.len()
    );
    assert_eq!(
        arr[0]["workflow"].as_str(),
        Some("release"),
        "the sole matching run should be the release workflow"
    );

    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file — history still works, shows "No run history for this project."
#[test]
fn history_no_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history for this project."),
        "No config should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --step with a step name that doesn't exist shows "No run history".
#[test]
fn history_unknown_step() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "nonexistent"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history for this project."),
        "Unknown step filter should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// After a failed run, history shows the failure in both plain-text and
/// JSON forms, and `--detail` surfaces the recorded error message.
#[test]
fn history_after_failed_run() {
    let temp = setup_project(FAILING_CONFIG);

    // Run and let it fail — `bivvy run` documents exit code 1 for a
    // failed workflow, so we verify that instead of discarding status.
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "FAILING_CONFIG run should NOT succeed (bad step uses an invalid git flag)"
    );

    // ── Plain-text history ───────────────────────────────────────────
    let mut s = spawn_bivvy(&["history"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "After failed run, history should show 'Run History' header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("default"),
        "Failed run entry should show 'default' workflow, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("✗"),
        "Failed run should show failure indicator '✗', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);

    // ── JSON history: status must be "Failed", error must be set ────
    let mut s = spawn_bivvy(&["history", "--json"], temp.path());
    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let parsed: serde_json::Value =
        serde_json::from_str(&text[json_start..=json_end]).expect("valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 1, "expected exactly 1 history entry");
    let entry = &arr[0];
    assert_eq!(
        entry["status"].as_str(),
        Some("Failed"),
        "status should be 'Failed' after a failed run"
    );
    assert_eq!(
        entry["workflow"].as_str(),
        Some("default"),
        "failed run should be on the 'default' workflow"
    );
    assert_eq!(
        entry["error"].as_str(),
        Some("One or more steps failed"),
        "error should match the message set by the runner"
    );
    let steps_run: Vec<&str> = entry["steps_run"]
        .as_array()
        .expect("steps_run should be an array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        steps_run.contains(&"good"),
        "good step should be recorded as run, got {steps_run:?}"
    );
    assert!(
        steps_run.contains(&"bad"),
        "bad step should also be recorded as attempted, got {steps_run:?}"
    );
    assert_exit_code(&s, 0);

    // ── --detail must surface the error line ─────────────────────────
    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Error: One or more steps failed"),
        "detail view should include the recorded error, got: {}",
        &text[..text.len().min(800)]
    );
    assert_exit_code(&s, 0);
}
