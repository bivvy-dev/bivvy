//! System tests for `bivvy run`.
//!
//! A mix of PTY-based interactive tests (for prompts and cascading behavior)
//! and `--non-interactive` subprocess tests (for exit codes and error paths
//! that are easier to verify outside a PTY). All tests isolate `HOME` so they
//! never leak state into the real user directory.
#![cfg(unix)]

mod system;

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::path::Path;
use std::process::Command;
use system::helpers::*;
use tempfile::TempDir;

/// Config where all steps have passing checks — triggers "Already complete" prompts.
/// Uses real commands (rustc, git, cargo) and real dependencies/workflows.
/// Steps are skippable (default true) so interactive prompts appear.
const COMPLETED_CONFIG: &str = r#"
app_name: "RunTest"
settings:
  defaults:
    output: verbose

steps:
  deps:
    title: "Install dependencies"
    command: "rustc --version && git --version"
    check:
      type: execution
      command: "rustc --version"

  build:
    title: "Build project"
    command: "cargo --version"
    depends_on: [deps]
    check:
      type: execution
      command: "cargo --version"

  test:
    title: "Run tests"
    command: "git status --short"
    depends_on: [build]

  lint:
    title: "Lint code"
    command: "rustc --version"
    depends_on: [build]

workflows:
  default:
    steps: [deps, build, test, lint]
  check:
    description: "Quick verification"
    steps: [lint, test]
"#;

/// Config with no checks — everything runs fresh.
/// Steps are NOT skippable so they run without prompts.
/// Uses real commands (git, rustc) and realistic features.
const FRESH_CONFIG: &str = r#"
app_name: "FreshApp"
steps:
  greet:
    title: "Say hello"
    command: "rustc --version"
    skippable: false
  farewell:
    title: "Say goodbye"
    command: "git --version"
    skippable: false
    depends_on: [greet]
workflows:
  default:
    steps: [greet, farewell]
"#;

// ── File-local helpers ────────────────────────────────────────────────
//
// These mirror the shared helpers in `system::helpers` but inject an isolated
// `HOME` so that `bivvy run` cannot read or write `~/.bivvy/` on the developer
// machine. Without this, every `bivvy run` invocation during the test suite
// leaks project state into the real home directory.

/// Create an isolated HOME directory for a test. The returned TempDir must
/// outlive any process that uses this path.
fn isolated_home() -> TempDir {
    TempDir::new().unwrap()
}

/// Spawn `bivvy` in a PTY with an isolated `HOME`.
fn spawn_bivvy_isolated(args: &[&str], project: &Path, home: &Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(project);
    cmd.env("HOME", home);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(TIMEOUT));
    session
}

/// Run `bivvy` as a normal subprocess with an isolated `HOME`.
/// Returns (stdout, stderr, exit code).
fn run_bivvy_isolated(
    args: &[&str],
    project: &Path,
    home: &Path,
) -> (String, String, Option<i32>) {
    let bin = cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(args)
        .current_dir(project)
        .env("HOME", home)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code(),
    )
}

/// Normalize output for snapshotting: strip ANSI, replace the tempdir path
/// with a stable placeholder, and trim trailing whitespace on each line.
fn normalize_for_snapshot(output: &str, tempdir: &Path) -> String {
    let stripped = strip_ansi(output);
    let tempdir_str = tempdir.to_string_lossy().to_string();
    stripped
        .replace(&tempdir_str, "[TEMPDIR]")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Default workflow (bare `bivvy`)
// ---------------------------------------------------------------------------

#[test]
fn bare_bivvy_runs_default_workflow() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&[], temp.path(), home.path());

    // Interactive mode shows app name in header
    expect_or_dump(&mut s, "FreshApp", "Header should show app name");
    // Header includes "default workflow" label
    expect_or_dump(
        &mut s,
        "default workflow",
        "Header should show 'default workflow'",
    );

    // Both steps run without prompts (skippable: false).
    // Summary footer shows "Total: ... · 2 run · 0 skipped".
    expect_or_dump(&mut s, "2 run", "Summary should show '2 run'");
    expect_or_dump(&mut s, "0 skipped", "Summary should show '0 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ---------------------------------------------------------------------------
// `bivvy run` — basic execution
// ---------------------------------------------------------------------------

#[test]
fn run_default_workflow() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&["run"], temp.path(), home.path());

    expect_or_dump(&mut s, "FreshApp", "Header should show app name");
    expect_or_dump(
        &mut s,
        "default workflow",
        "Header should show 'default workflow'",
    );
    // Steps are skippable: false so they run without prompts
    expect_or_dump(&mut s, "2 run", "Summary should show '2 run'");
    expect_or_dump(&mut s, "0 skipped", "Summary should show '0 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_named_workflow() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(
        &["run", "--workflow", "check", "--dry-run"],
        temp.path(),
        home.path(),
    );

    // Header shows the named workflow (not "default workflow")
    expect_or_dump(
        &mut s,
        "check workflow",
        "Header should show 'check workflow'",
    );
    // Dry-run mode message
    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run banner should appear",
    );
    // The check workflow contains the 'lint' and 'test' steps — their titles
    // should appear in the summary.
    expect_or_dump(&mut s, "Lint code", "Summary should list 'Lint code' step");
    expect_or_dump(&mut s, "Run tests", "Summary should list 'Run tests' step");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ---------------------------------------------------------------------------
// `bivvy run` — interactive prompts for completed steps
// ---------------------------------------------------------------------------

#[test]
fn run_interactive_completed_step_shows_rerun_prompt() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&["run"], temp.path(), home.path());

    // deps has check `rustc --version`, so the prompt reads
    // "Already complete (command: rustc --version). Re-run?".
    wait_and_answer(
        &s,
        "Already complete (command: rustc --version). Re-run?",
        KEY_N,
        "deps step should show rerun prompt with rustc command",
    );

    // Declining deps cascades: build depends on deps → auto-skipped without
    // showing its own prompt. test and lint depend on build → also auto-skipped.
    // Final summary: 0 run, 4 skipped.
    wait_for(&s, "0 run", "Summary should show '0 run'");
    wait_for(&s, "4 skipped", "Summary should show '4 skipped'");
    // Verify the process actually exits cleanly — without this the test
    // would pass even if bivvy hung or crashed after printing the summary.
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_interactive_decline_second_rerun_prompt() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&["run"], temp.path(), home.path());

    // Accept rerun of deps so build's prompt actually appears.
    wait_and_answer(
        &s,
        "Already complete (command: rustc --version). Re-run?",
        KEY_Y,
        "deps step should prompt for rerun",
    );

    // Decline build's rerun prompt — the full question includes the command.
    wait_and_answer(
        &s,
        "Already complete (command: cargo --version). Re-run?",
        KEY_N,
        "build step should prompt for rerun",
    );

    // Declining build cascades: test and lint depend on build → auto-skipped.
    // Summary: 1 run (deps), 3 skipped (build, test, lint).
    wait_for(&s, "1 run", "Summary should show '1 run'");
    wait_for(&s, "3 skipped", "Summary should show '3 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_interactive_accept_rerun_completed_step() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&["run"], temp.path(), home.path());

    // Accept rerun of deps
    wait_and_answer(
        &s,
        "Already complete (command: rustc --version). Re-run?",
        KEY_Y,
        "deps step should prompt for rerun",
    );

    // Accept rerun of build
    wait_and_answer(
        &s,
        "Already complete (command: cargo --version). Re-run?",
        KEY_Y,
        "build step should prompt for rerun",
    );

    // Remaining steps (test, lint) have no check, so they are
    // skippable and prompt with their title.
    wait_and_answer(&s, "Run tests?", KEY_Y, "test step should show run prompt");
    wait_and_answer(&s, "Lint code?", KEY_Y, "lint step should show run prompt");

    // All 4 steps should have run
    wait_for(&s, "4 run", "Summary should show '4 run'");
    wait_for(&s, "0 skipped", "Summary should show '0 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ---------------------------------------------------------------------------
// `bivvy run` — flags
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_flag() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&["run", "--dry-run"], temp.path(), home.path());

    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run banner should appear",
    );
    expect_or_dump(&mut s, "Summary", "Summary section should appear");
    // In dry-run mode, checks are not prompted (non-interactive-ish),
    // so 4 steps should be listed in the summary.
    expect_or_dump(
        &mut s,
        "Install dependencies",
        "Summary should include 'Install dependencies' step",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_verbose_flag() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(&["run", "--verbose"], temp.path(), home.path());

    expect_or_dump(&mut s, "FreshApp", "Header should show app name");
    // Verbose should show step titles, a Summary section, and the run count.
    expect_or_dump(&mut s, "Say hello", "Verbose output should show step title");
    expect_or_dump(&mut s, "Summary", "Summary section should appear");
    expect_or_dump(&mut s, "2 run", "Summary should show '2 run'");
    expect_or_dump(&mut s, "0 skipped", "Summary should show '0 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_quiet_flag() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let (stdout, stderr, code) = run_bivvy_isolated(
        &["run", "--quiet", "--non-interactive"],
        temp.path(),
        home.path(),
    );
    assert_eq!(code, Some(0), "Quiet mode should exit 0, stderr: {stderr}");
    // Quiet mode suppresses most output — the verbose summary box must not
    // appear, but the command should still have completed successfully.
    assert!(
        !stdout.contains("Summary"),
        "Quiet mode should suppress summary output, got: {stdout}"
    );
    // It should also not contain the step title (verbose output).
    assert!(
        !stdout.contains("Say hello"),
        "Quiet mode should suppress step titles, got: {stdout}"
    );
}

#[test]
fn run_only_flag_filters_steps() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let mut s =
        spawn_bivvy_isolated(&["run", "--only", "greet"], temp.path(), home.path());

    // The filtered step's title should appear in the output
    expect_or_dump(
        &mut s,
        "Say hello",
        "Filtered step title should appear in output",
    );
    // Summary should show 1 run, 0 skipped
    expect_or_dump(&mut s, "1 run", "Summary should show '1 run'");
    expect_or_dump(&mut s, "0 skipped", "Summary should show '0 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_skip_flag_skips_steps() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let mut s =
        spawn_bivvy_isolated(&["run", "--skip", "farewell"], temp.path(), home.path());

    // The non-skipped step's title should appear in the output
    expect_or_dump(
        &mut s,
        "Say hello",
        "Non-skipped step title should appear in output",
    );
    // Summary should show 1 run, 1 skipped (farewell was skipped)
    expect_or_dump(&mut s, "1 run", "Summary should show '1 run'");
    expect_or_dump(&mut s, "1 skipped", "Summary should show '1 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_force_flag_reruns_completed() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(
        &["run", "--force", "deps", "--only", "deps"],
        temp.path(),
        home.path(),
    );

    // Forced step title should appear in the output — no "Already complete"
    // prompt because --force bypasses the completed check.
    expect_or_dump(
        &mut s,
        "Install dependencies",
        "Forced step title should appear in output",
    );
    // Verify step actually ran (summary shows 1 run, not skipped)
    expect_or_dump(&mut s, "1 run", "Summary should show '1 run'");
    expect_or_dump(&mut s, "0 skipped", "Summary should show '0 skipped'");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

#[test]
fn run_env_flag() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let mut s = spawn_bivvy_isolated(
        &["run", "--env", "ci", "--dry-run"],
        temp.path(),
        home.path(),
    );

    // Header should show environment in the run header line
    expect_or_dump(
        &mut s,
        "env: ci",
        "Run header should include 'env: ci'",
    );
    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run banner should appear",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ---------------------------------------------------------------------------
// `bivvy run` — error paths
// ---------------------------------------------------------------------------

#[test]
fn run_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let home = isolated_home();
    let (stdout, stderr, code) = run_bivvy_isolated(&["run"], temp.path(), home.path());

    let combined = format!("{stdout}{stderr}");
    // Match the full user-facing error including the hint
    assert!(
        combined.contains("No configuration found. Run 'bivvy init' first."),
        "Should show full 'No configuration found. Run 'bivvy init' first.' error, \
         got stdout: {stdout}, stderr: {stderr}",
    );
    assert_eq!(code, Some(2), "Missing config should exit 2");

    // Snapshot the normalized stderr so that regressions in wording or
    // formatting are caught, not just the one substring above.
    let normalized = normalize_for_snapshot(&stderr, temp.path());
    insta::assert_snapshot!("run_no_config_stderr", normalized);
}

#[test]
fn run_unknown_workflow_fails() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let (stdout, stderr, code) = run_bivvy_isolated(
        &["run", "--workflow", "nonexistent"],
        temp.path(),
        home.path(),
    );

    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Unknown workflow: nonexistent"),
        "Should show 'Unknown workflow: nonexistent' error, got stdout: {stdout}, stderr: {stderr}",
    );
    assert_eq!(code, Some(1), "Unknown workflow should exit 1");

    // Snapshot stderr for regression on error wording.
    let normalized = normalize_for_snapshot(&stderr, temp.path());
    insta::assert_snapshot!("run_unknown_workflow_stderr", normalized);
}

#[test]
fn run_invalid_yaml_fails() {
    // Malformed YAML: unterminated string followed by structurally invalid
    // content. This should produce a parse error on stderr and a non-zero
    // exit code.
    let bad_config = "app_name: \"unterminated\nsteps:\n  - not_a_map\n";
    let temp = setup_project(bad_config);
    let home = isolated_home();
    let (_stdout, stderr, code) = run_bivvy_isolated(&["run"], temp.path(), home.path());

    // We don't hard-code the exact wording (it comes from marked_yaml), but
    // we verify the shape: non-zero exit and non-empty stderr.
    assert_ne!(code, Some(0), "Invalid YAML should not exit 0");
    let normalized = normalize_for_snapshot(&stderr, temp.path());
    assert!(
        !normalized.trim().is_empty(),
        "Expected a parse error on stderr, got empty output"
    );
}

#[test]
fn run_unknown_only_step_fails() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let (stdout, stderr, code) = run_bivvy_isolated(
        &["run", "--only", "nonexistent_step", "--non-interactive"],
        temp.path(),
        home.path(),
    );

    // Unknown step in --only should produce a non-zero exit and mention the
    // offending step name somewhere in the output.
    assert_ne!(code, Some(0), "Unknown --only step should not exit 0");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("nonexistent_step"),
        "Error should reference the unknown step 'nonexistent_step', \
         got stdout: {stdout}, stderr: {stderr}",
    );
}

// ---------------------------------------------------------------------------
// `bivvy run` — exit code verification
// ---------------------------------------------------------------------------

#[test]
fn run_success_exit_code_zero() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let (stdout, stderr, code) =
        run_bivvy_isolated(&["run", "--non-interactive"], temp.path(), home.path());
    let combined = format!("{stdout}{stderr}");
    // Verify both content AND exit code — not just the exit code.
    assert!(
        combined.contains("2 run"),
        "Successful run should report '2 run', got stdout: {stdout}, stderr: {stderr}"
    );
    assert_eq!(code, Some(0), "Successful workflow should exit 0");
}

#[test]
fn run_dry_run_exit_code_zero() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let (stdout, stderr, code) =
        run_bivvy_isolated(&["run", "--dry-run"], temp.path(), home.path());
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Running in dry-run mode - no commands will be executed"),
        "Dry run should show dry-run banner, got stdout: {stdout}, stderr: {stderr}"
    );
    assert_eq!(code, Some(0), "Dry run should exit 0");
}

// ---------------------------------------------------------------------------
// `bivvy run` — step failure exit code
// ---------------------------------------------------------------------------

#[test]
fn run_failing_step_exit_code_one() {
    let config = r#"
app_name: "FailTest"
steps:
  bad_step:
    title: "Fail on purpose"
    command: "rustc --this-flag-does-not-exist"
    skippable: false
workflows:
  default:
    steps: [bad_step]
"#;
    let temp = setup_project(config);
    let home = isolated_home();
    let (stdout, stderr, code) =
        run_bivvy_isolated(&["run", "--non-interactive"], temp.path(), home.path());

    let combined = format!("{stdout}{stderr}");
    // Should report the failure — use the step title, not just the step name.
    assert!(
        combined.contains("Fail on purpose"),
        "Output should mention the failed step title 'Fail on purpose', \
         got stdout: {stdout}, stderr: {stderr}",
    );
    assert_eq!(code, Some(1), "Failed step should exit 1");
}

// ---------------------------------------------------------------------------
// `bivvy run` — interactive interrupt (Ctrl+C → exit 130)
// ---------------------------------------------------------------------------

#[test]
fn run_interrupted_exit_code_130() {
    let temp = setup_project_with_git(COMPLETED_CONFIG);
    let home = isolated_home();
    let s = spawn_bivvy_isolated(&["run"], temp.path(), home.path());

    // Wait for the first interactive prompt, then send Ctrl-C instead of
    // answering. bivvy must exit with 130 per the documented exit codes.
    wait_for(
        &s,
        "Already complete (command: rustc --version). Re-run?",
        "deps rerun prompt should appear before interrupt",
    );
    send_key(&s, KEY_CTRL_C);

    // Poll the process until it exits (or timeout). Accept either
    // Exited(130) or signal-terminated by SIGINT.
    use expectrl::{Signal, WaitStatus};
    use std::time::{Duration, Instant};
    let start = Instant::now();
    let status = loop {
        match s.get_process().status() {
            Ok(WaitStatus::StillAlive) => {
                if start.elapsed() > Duration::from_secs(10) {
                    panic!("bivvy did not exit within 10s of Ctrl-C");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(other) => break other,
            Err(e) => panic!("waitpid failed: {e}"),
        }
    };
    match status {
        WaitStatus::Exited(_, code) => {
            assert_eq!(
                code, 130,
                "Ctrl+C should produce exit code 130, got {code}"
            );
        }
        WaitStatus::Signaled(_, sig, _) => {
            assert_eq!(
                sig,
                Signal::SIGINT,
                "Expected SIGINT if signal-terminated, got {sig:?}"
            );
        }
        other => panic!("Unexpected wait status: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// `bivvy run` — side effect verification
// ---------------------------------------------------------------------------

#[test]
fn run_creates_state_file() {
    let temp = setup_project_with_git(FRESH_CONFIG);
    let home = isolated_home();
    let (stdout1, stderr1, code1) =
        run_bivvy_isolated(&["run", "--non-interactive"], temp.path(), home.path());
    assert_eq!(
        code1,
        Some(0),
        "Run should succeed, stdout: {stdout1}, stderr: {stderr1}"
    );

    // After a successful run, state should be persisted.
    // Verify by running `bivvy last` which reads state. Both commands share
    // the same isolated HOME so the state from run #1 is visible to last.
    let (last_stdout, last_stderr, last_code) =
        run_bivvy_isolated(&["last"], temp.path(), home.path());
    assert_eq!(
        last_code,
        Some(0),
        "bivvy last should exit 0, stderr: {last_stderr}"
    );
    // `bivvy last` prints "Workflow:  default" (key-value line) for the
    // previous run — assert on both the label and the workflow name together.
    assert!(
        last_stdout.contains("Workflow:") && last_stdout.contains("default"),
        "bivvy last should show 'Workflow: default' from the previous run, got: {last_stdout}"
    );
}

// ---------------------------------------------------------------------------
// `bivvy run` — dependency chain with failure
// ---------------------------------------------------------------------------

#[test]
fn run_dependency_failure_blocks_dependents() {
    let config = r#"
app_name: "DepFailTest"
steps:
  good:
    title: "Good step"
    command: "rustc --version"
    skippable: false
  bad:
    title: "Bad step"
    command: "git --no-such-flag-xyz"
    skippable: false
    depends_on: [good]
  after-bad:
    title: "After bad"
    command: "git --version"
    skippable: false
    depends_on: [bad]
workflows:
  default:
    steps: [good, bad, after-bad]
"#;
    let temp = setup_project_with_git(config);
    let home = isolated_home();
    let (stdout, stderr, code) =
        run_bivvy_isolated(&["run", "--non-interactive"], temp.path(), home.path());

    let combined = format!("{stdout}{stderr}");
    // after-bad should be blocked because its dependency (bad) failed —
    // assert on the full status string from the summary table.
    assert!(
        combined.contains("Blocked (dependency failed)"),
        "Dependent step should show 'Blocked (dependency failed)' status, \
         got stdout: {stdout}, stderr: {stderr}",
    );
    assert_eq!(code, Some(1), "Workflow with failed step should exit 1");
}
