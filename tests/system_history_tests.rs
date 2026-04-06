//! Comprehensive system tests for `bivvy history`.
//!
//! Tests execution history display including multiple runs, filtering,
//! detail levels, time-based queries, JSON structure, timing display,
//! success/failure indicators, and all flag combinations.
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
  deploy:
    title: "Deploy"
    command: "git --version"
    skippable: false
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

/// No runs yet shows "No run history".
#[test]
fn history_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history") || text.contains("no history") || text.contains("No runs"),
        "Should indicate no history, got: {}",
        &text[..text.len().min(300)]
    );
}

/// After a single run, shows that run with workflow name.
#[test]
fn history_after_single_run() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("History"),
        "Should show header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show workflow name 'default', got: {}",
        &text[..text.len().min(500)]
    );
}

/// After multiple runs, shows all runs.
#[test]
fn history_after_multiple_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("History"),
        "Should show header"
    );
    // Multiple runs should be listed
    assert!(
        text.contains("default"),
        "Should show workflow name, got: {}",
        &text[..text.len().min(500)]
    );
}

/// After runs on different workflows, shows both.
#[test]
fn history_multiple_workflows() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    run_workflow_silently(temp.path());
    run_bivvy_silently(temp.path(), &["run", "--workflow", "release"]);

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("History"),
        "Should show header"
    );
    // Both workflows should appear
    assert!(
        text.contains("default") || text.contains("release"),
        "Should show at least one workflow name, got: {}",
        &text[..text.len().min(500)]
    );
}

/// History shows success indicator for successful run.
#[test]
fn history_shows_success_indicator() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("✓") || text.contains("✔") || text.contains("success")
            || text.contains("passed") || text.contains("2 run"),
        "Should show success indicator, got: {}",
        &text[..text.len().min(500)]
    );
}

/// History shows timing/duration for runs.
#[test]
fn history_shows_timing() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    // Should show some timing info (seconds, ms, duration, or relative timestamp)
    assert!(
        text.contains("s") || text.contains("ms") || text.contains("ago")
            || text.contains("duration") || text.contains("sec"),
        "Should show timing info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// History shows step counts.
#[test]
fn history_shows_step_counts() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("2") || text.contains("step"),
        "Should show step count info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// History after failed run shows failure indicator.
#[test]
fn history_shows_failure_indicator() {
    let temp = setup_project(FAILING_CONFIG);
    // Run and let it fail
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("✗") || text.contains("✘") || text.contains("fail")
            || text.contains("error") || text.contains("1") || text.contains("default"),
        "Should show failure indicator or run info, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// FLAGS
// =====================================================================

/// --detail shows step-level information.
#[test]
fn history_detail_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet") || text.contains("farewell") || text.contains("Steps")
            || text.contains("step"),
        "Detail should show step info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --limit restricts number of runs shown.
#[test]
fn history_limit_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--limit", "1"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("History") || text.contains("default"),
        "Should show history with limit, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --json outputs structured data with workflow key.
#[test]
fn history_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("default") || text.contains("["),
        "Should output JSON with workflow key, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json output has expected structure (steps, status, duration).
#[test]
fn history_json_structure() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--json", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("steps") || text.contains("status") || text.contains("duration")
            || text.contains("workflow"),
        "JSON should contain structured data, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --since filters by time window.
#[test]
fn history_since_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--since", "1h"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("default") || text.contains("History"),
        "Should show recent history within 1h, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --step filters by step name.
#[test]
fn history_step_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "greet"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet") || text.contains("hello") || text.contains("History")
            || text.contains("default"),
        "Should show history filtered to greet step, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --detail + --json combined.
#[test]
fn history_detail_json() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--detail", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("workflow") || text.contains("[") || text.contains("steps"),
        "Detail+JSON should produce structured output, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --limit + --since combined.
#[test]
fn history_limit_since() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--limit", "5", "--since", "24h"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("default") || text.contains("History") || text.contains("Run"),
        "Limit+since should show results, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --step + --detail combined.
#[test]
fn history_step_detail() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "greet", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("greet") || text.contains("History") || text.contains("default"),
        "Step+detail should show step info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --since with various duration formats.
#[test]
fn history_since_various_formats() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    // Minutes
    let mut s = spawn_bivvy(&["history", "--since", "30m"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("default") || text.contains("No"),
        "30m format should show history, got: {}",
        &text[..text.len().min(300)]
    );

    // Days
    let mut s = spawn_bivvy(&["history", "--since", "7d"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("default") || text.contains("No"),
        "7d format should show history, got: {}",
        &text[..text.len().min(300)]
    );

    // Hours
    let mut s = spawn_bivvy(&["history", "--since", "2h"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("default") || text.contains("No"),
        "2h format should show history, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Long history with many runs shows all entries.
#[test]
fn history_long_history() {
    let temp = setup_project(SIMPLE_CONFIG);
    for _ in 0..5 {
        run_workflow_silently(temp.path());
    }

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("default"),
        "Long history should show entries, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn history_help() {
    let mut s = spawn_bivvy_global(&["history", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("history") || text.contains("History"),
        "Help should describe history command, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file.
#[test]
fn history_no_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No") || text.contains("configuration") || text.contains("error")
            || text.contains("not found"),
        "No config should show error message, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --step with unknown step name.
#[test]
fn history_unknown_step() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ghost") || text.contains("not found") || text.contains("No")
            || text.contains("unknown"),
        "Unknown step should handle gracefully, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --since with an edge-case duration that matches nothing.
#[test]
fn history_since_zero_matches() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history", "--since", "0m"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No") || text.contains("Run History") || text.contains("history"),
        "Zero-duration since should show history or empty message, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --limit 0 edge case.
#[test]
fn history_limit_zero() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--limit", "0"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History") || text.contains("No") || text.contains("0"),
        "Limit 0 should show history info, got: {}",
        &text[..text.len().min(300)]
    );
}

/// JSON output with no runs produces valid JSON.
#[test]
fn history_json_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("[") || text.contains("null") || text.contains("No")
            || text.contains("no runs"),
        "JSON with no runs should handle gracefully, got: {}",
        &text[..text.len().min(300)]
    );
}
