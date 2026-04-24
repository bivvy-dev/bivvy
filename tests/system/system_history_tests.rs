//! Comprehensive system tests for `bivvy history`.
//!
//! Tests execution history display including multiple runs, filtering,
//! detail levels, time-based queries, JSON structure, timing display,
//! success/failure indicators, and all flag combinations.
//!
//! All tests run with `HOME` set to a temp dir so the global bivvy
//! store (`~/.bivvy/projects/`) is isolated from the user environment.
//! Tests verify process exit codes, use verbatim user-facing strings
//! in assertions, and snapshot stable structural output (`--help`,
//! JSON with redactions) via `insta`.
#![cfg(unix)]

mod system;

use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Helpers
//
// Every spawn in this file routes through the shared helpers in
// `tests/system/helpers.rs`, which pin `HOME` and all four `XDG_*`
// base-directory variables to `<project>/.test_home`.  That keeps the
// global bivvy store (`~/.bivvy/projects/`) — which is what `history`
// reads — isolated from the real user environment on macOS and Linux.
// ─────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────
// Configs — all use real developer tool commands (git, rustc, cargo)
// rather than shell builtins, per system-test quality standards.
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
    command: "cargo --version"
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

/// With no runs recorded, `bivvy history` prints the documented empty
/// message verbatim and exits 0.
#[test]
fn history_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history for this project."),
        "Should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// After a single successful run, history shows the `Run History` header,
/// the `default` workflow name, and the success icon (`✓`).
#[test]
fn history_after_single_run() {
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
        text.contains("\u{2713}"),
        "Should show success icon '✓' for successful run, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After three runs, history lists three separate `default` entries.
#[test]
fn history_after_multiple_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    // Multiple runs should each show "default" workflow
    let default_count = text.matches("default").count();
    assert!(
        default_count >= 3,
        "Should show 'default' at least 3 times for 3 runs, found {} occurrences in: {}",
        default_count,
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After runs on two workflows (`default` and `release`), history lists both.
#[test]
fn history_multiple_workflows() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    run_workflow_silently(temp.path());
    run_bivvy_silently(temp.path(), &["run", "--workflow", "release"]);

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show 'default' workflow, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("release"),
        "Should show 'release' workflow, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// A successful run renders the `✓` success icon (from
/// `StatusKind::Success::icon()`).
#[test]
fn history_shows_success_indicator() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("\u{2713}"),
        "Should show success icon '✓' (StatusKind::Success), got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// History shows timing using `format_relative_time` (`just now` for
/// fresh runs) and `format_duration` (`Xms` for sub-second steps).
#[test]
fn history_shows_timing() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    // `format_relative_time` returns "just now" when seconds < 60.
    assert!(
        text.contains("just now"),
        "Should show 'just now' relative time for fresh run, got: {}",
        &text[..text.len().min(500)]
    );
    // `format_duration` uses "ms" for sub-second durations. `git --version`
    // and `rustc --version` both complete well under a second.
    assert!(
        text.contains("ms"),
        "Should show 'ms' duration suffix for sub-second steps, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// History shows `2 steps` for SIMPLE_CONFIG's two-step workflow.
#[test]
fn history_shows_step_counts() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    // SIMPLE_CONFIG has 2 steps: greet and farewell
    assert!(
        text.contains("2 steps"),
        "Should show '2 steps' for the two-step workflow, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// After a failed run, history shows the `Run History` header and the
/// `✗` failure icon (from `StatusKind::Failed::icon()`).
#[test]
fn history_shows_failure_indicator() {
    let temp = setup_project(FAILING_CONFIG);
    // Run and let it fail — we expect a non-zero exit, so use the
    // assert-cmd helper (which doesn't enforce success) rather than
    // `run_bivvy_silently` (which would panic on the failing run).
    let _ = bivvy_assert_cmd(temp.path())
        .args(["run", "--non-interactive"])
        .output()
        .expect("Failed to spawn bivvy");

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header after failed run, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("\u{2717}"),
        "Should show failure icon '✗' (StatusKind::Failed), got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS
// =====================================================================

/// `--detail` prints the `Steps:` label and lists each executed step
/// name under every run entry.
#[test]
fn history_detail_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Steps:"),
        "Detail should show 'Steps:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("greet") && text.contains("farewell"),
        "Detail should show step names 'greet' and 'farewell', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// `--limit 1` restricts output to exactly one run entry, even with
/// three runs recorded.
#[test]
fn history_limit_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--limit", "1"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    // With --limit 1, only one "default" entry should appear
    let default_count = text.matches("default").count();
    assert_eq!(
        default_count, 1,
        "With --limit 1, should show exactly 1 run entry, found {} in: {}",
        default_count,
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// `--json` output parses as a JSON array with one entry for one run
/// and contains all documented fields: `workflow`, `timestamp`,
/// `duration_ms`, `status`, `steps_run`. The structure is also
/// snapshot via `insta` (with redactions for time-varying fields) so
/// schema regressions are caught.
#[test]
fn history_json_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    // Extract the JSON array from the output
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("JSON output should be valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 1, "Should have exactly 1 run entry");
    let entry = &arr[0];
    assert_eq!(
        entry["workflow"].as_str(),
        Some("default"),
        "JSON entry should have workflow 'default'"
    );
    assert!(
        entry.get("timestamp").is_some(),
        "JSON entry should have 'timestamp' field"
    );
    assert!(
        entry.get("duration_ms").is_some(),
        "JSON entry should have 'duration_ms' field"
    );
    assert!(
        entry.get("status").is_some(),
        "JSON entry should have 'status' field"
    );
    assert!(
        entry.get("steps_run").is_some(),
        "JSON entry should have 'steps_run' field"
    );

    // Snapshot the JSON structure with redactions for time-varying fields
    // so schema drift is caught without flaking on timing.
    insta::assert_json_snapshot!(
        "history_json_flag_structure",
        parsed,
        {
            "[].timestamp" => "[timestamp]",
            "[].duration_ms" => "[duration_ms]",
            "[].steps_run" => insta::sorted_redaction(),
        }
    );

    assert_exit_code(&s, 0);
}

/// `--json --detail` contains the full step list in `steps_run`.
#[test]
fn history_json_structure() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--json", "--detail"], temp.path());

    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("JSON output should be valid JSON");
    let arr = parsed.as_array().expect("JSON output should be an array");
    let entry = &arr[0];

    // Verify steps_run contains actual step names
    let steps = entry["steps_run"]
        .as_array()
        .expect("steps_run should be an array");
    let step_names: Vec<&str> = steps.iter().filter_map(|s| s.as_str()).collect();
    assert!(
        step_names.contains(&"greet") && step_names.contains(&"farewell"),
        "steps_run should contain 'greet' and 'farewell', got: {:?}",
        step_names
    );
    assert_exit_code(&s, 0);
}

/// `--since 1h` includes runs from the last hour.
#[test]
fn history_since_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--since", "1h"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header for recent run within 1h, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show the recent run, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--step <name>` filters to runs that executed that step.
#[test]
fn history_step_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "greet"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Should show 'Run History' header when filtering by step, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Should show run that includes 'greet' step, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--detail --json` combined produces JSON where each entry has a
/// `steps_run` list — the detail flag doesn't alter JSON schema, but
/// the combination must still work and parse.
#[test]
fn history_detail_json() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--detail", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("Detail+JSON should produce valid JSON");
    let arr = parsed.as_array().expect("Should be a JSON array");
    assert!(
        !arr.is_empty(),
        "Detail+JSON should have at least one entry"
    );
    assert!(
        arr[0].get("steps_run").is_some(),
        "Detail+JSON entry should have 'steps_run' field"
    );
    assert_exit_code(&s, 0);
}

/// `--limit 5 --since 24h` combined — both filters apply without error.
#[test]
fn history_limit_since() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(
        &["history", "--limit", "5", "--since", "24h"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Limit+since should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "Limit+since should show the run, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--step greet --detail` filters then prints step detail.
#[test]
fn history_step_detail() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(
        &["history", "--step", "greet", "--detail"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Step+detail should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("Steps:"),
        "Step+detail should show 'Steps:' label, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("greet"),
        "Step+detail should show 'greet' step name, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// `--since` accepts minute (`30m`), hour (`2h`), and day (`7d`) units.
#[test]
fn history_since_various_formats() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    // Minutes
    let mut s = spawn_bivvy(&["history", "--since", "30m"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "30m format should show 'Run History', got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "30m format should show recent run, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);

    // Days
    let mut s = spawn_bivvy(&["history", "--since", "7d"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "7d format should show 'Run History', got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "7d format should show recent run, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);

    // Hours
    let mut s = spawn_bivvy(&["history", "--since", "2h"], temp.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "2h format should show 'Run History', got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("default"),
        "2h format should show recent run, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// Long history (5 runs) shows at least 5 `default` entries.
#[test]
fn history_long_history() {
    let temp = setup_project(SIMPLE_CONFIG);
    for _ in 0..5 {
        run_workflow_silently(temp.path());
    }

    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Long history should show 'Run History' header, got: {}",
        &text[..text.len().min(300)]
    );
    let default_count = text.matches("default").count();
    assert!(
        default_count >= 5,
        "Long history should show at least 5 'default' entries, found {} in: {}",
        default_count,
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// HELP (snapshot)
// =====================================================================

/// `bivvy history --help` output is snapshot so flag renames and
/// description changes are caught as regressions.
#[test]
fn history_help() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["history", "--help"], temp.path());
    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("history_tests_help", text);
    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// With no config file in the project, `bivvy history` still works and
/// shows the empty-history message.
#[test]
fn history_no_config() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["history"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history for this project."),
        "No config should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--step <unknown>` filters to nothing and shows the empty message.
#[test]
fn history_unknown_step() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--step", "ghost"], temp.path());

    let text = read_to_eof(&mut s);
    // "ghost" step was never run, so filter yields no results
    assert!(
        text.contains("No run history for this project."),
        "Unknown step filter should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--since 0m` with no runs shows the empty message.
#[test]
fn history_since_zero_matches() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history", "--since", "0m"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No run history for this project."),
        "Zero-duration since with no runs should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--limit 0` yields the empty message (no runs shown).
#[test]
fn history_limit_zero() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--limit", "0"], temp.path());

    let text = read_to_eof(&mut s);
    // --limit 0 means show 0 runs, which should yield "No run history"
    assert!(
        text.contains("No run history for this project."),
        "Limit 0 should show 'No run history for this project.', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--json` with no runs produces a valid empty JSON array.
#[test]
fn history_json_no_runs() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    // Should produce an empty JSON array
    let json_start = text.find('[').expect("JSON output should contain '['");
    let json_end = text.rfind(']').expect("JSON output should contain ']'");
    let json_str = &text[json_start..=json_end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("JSON with no runs should be valid JSON");
    let arr = parsed.as_array().expect("Should be a JSON array");
    assert!(arr.is_empty(), "JSON with no runs should be an empty array");
    assert_exit_code(&s, 0);
}

/// `--limit <non-numeric>` is rejected by clap with a parse error and
/// exit code 2 (clap's standard usage-error code).
#[test]
fn history_limit_invalid() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["history", "--limit", "not-a-number"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("invalid value"),
        "Invalid --limit value should produce an error message, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 2);
}

/// `--since <invalid-format>` is accepted by clap (it's a string) but
/// `parse_since` returns `None`, so the filter is a no-op. The run
/// should still be listed and exit 0. This documents the current
/// behaviour: invalid durations are silently ignored.
#[test]
fn history_since_invalid_format() {
    let temp = setup_project(SIMPLE_CONFIG);
    run_workflow_silently(temp.path());

    let mut s = spawn_bivvy(&["history", "--since", "not-a-duration"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Run History"),
        "Invalid --since should fall through to no filter, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("default"),
        "Invalid --since should still show the run, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}
