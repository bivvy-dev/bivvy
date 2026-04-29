//! System tests for `bivvy run` command flags and global flags.
//!
//! Verifies every CLI flag on the run command and all global flags
//! work correctly. Uses external programs (git, rustc, cargo)
//! instead of shell builtins.
//!
//! All tests run with `HOME` set to a temp dir so the global bivvy
//! store (`~/.bivvy/projects/`) is isolated from the real user
//! environment.  Exit-code tests use `assert_cmd::Command` chained
//! with `predicates` per community Rust CLI testing norms.
#![cfg(unix)]
// `assert_cmd::Command::cargo_bin` is soft-deprecated in 2.1.0+ but the
// rest of the crate still builds against it; the project-wide tests use
// it under `#[allow(deprecated)]` for consistency.
#![allow(deprecated)]

mod system;
use system::helpers::*;

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
//
// Every spawn in this file routes through the shared helpers in
// `tests/system/helpers.rs`, which pin `HOME` and all four `XDG_*`
// base-directory variables to `<project>/.test_home`.  This keeps the
// global bivvy store (`~/.bivvy/projects/`) out of the developer / CI
// user environment without needing a separate per-test home tempdir.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Configs
// ---------------------------------------------------------------------------

/// 4-step workflow with non-skippable steps. Uses real commands (git, rustc,
/// cargo) against a real project with a git repo.
const FLAG_CONFIG: &str = r#"
app_name: "FlagTest"

settings:
  defaults:
    output: verbose

steps:
  check-tools:
    title: "Check development tools"
    command: "rustc --version && cargo --version"
    skippable: false
    check:
      type: execution
      command: "rustc --version"

  inspect-repo:
    title: "Inspect repository"
    command: "git rev-parse --git-dir && git status --short"
    skippable: false
    depends_on: [check-tools]
    check:
      type: execution
      command: "git rev-parse --git-dir"

  analyze-source:
    title: "Analyze source files"
    command: "rustc --print cfg && cargo --version --verbose"
    skippable: false
    depends_on: [inspect-repo]

  generate-report:
    title: "Generate build report"
    command: "rustc --version --verbose > .build-report.txt && cargo --version >> .build-report.txt"
    skippable: false
    depends_on: [analyze-source]

workflows:
  default:
    steps: [check-tools, inspect-repo, analyze-source, generate-report]
  quick:
    steps: [check-tools, inspect-repo]
"#;

/// 3-step workflow with skippable steps (default) for testing --non-interactive
/// and --force. No completed_checks so steps always prompt interactively.
const INTERACTIVE_CONFIG: &str = r#"
app_name: "InteractiveFlags"

settings:
  defaults:
    output: verbose

steps:
  scan-files:
    title: "Scan project files"
    command: "git ls-files --cached"

  check-version:
    title: "Check version info"
    command: "cargo --version --verbose"
    depends_on: [scan-files]

  summarize:
    title: "Summarize project"
    command: "git log --oneline -3"
    depends_on: [check-version]

workflows:
  default:
    steps: [scan-files, check-version, summarize]
"#;

/// Config with environment-specific steps for --env flag testing.
const ENV_CONFIG: &str = r#"
app_name: "EnvTest"

settings:
  defaults:
    output: verbose

steps:
  always-run:
    title: "Always runs"
    command: "rustc --version"
    skippable: false

  dev-only:
    title: "Dev environment step"
    command: "git rev-parse HEAD"
    skippable: false
    only_environments: [development]
    depends_on: [always-run]

  ci-only:
    title: "CI environment step"
    command: "cargo --version"
    skippable: false
    only_environments: [ci]
    depends_on: [always-run]

workflows:
  default:
    steps: [always-run, dev-only, ci-only]
"#;

/// Config with a step that fails for exit code testing.
const FAILING_CONFIG: &str = r#"
app_name: "FailTest"
steps:
  fail-step:
    title: "Failing step"
    command: "git --no-such-flag-xyz"
    skippable: false
workflows:
  default:
    steps: [fail-step]
"#;

/// Config with a sensitive step for flag combination tests.
const SENSITIVE_FLAG_CONFIG: &str = r#"
app_name: "SensitiveFlagTest"
steps:
  prepare:
    title: "Prepare environment"
    command: "rustc --version"
    skippable: false
  sensitive-step:
    title: "Sensitive operation"
    command: "git --version && rustc --version"
    skippable: false
    sensitive: true
    depends_on: [prepare]
workflows:
  default:
    steps: [prepare, sensitive-step]
"#;

/// Config with preconditions for flag testing.
const PRECONDITION_FLAG_CONFIG: &str = r#"
app_name: "PreconditionFlagTest"
steps:
  guarded:
    title: "Guarded step"
    command: "rustc --version && cargo --version"
    skippable: false
    precondition:
      type: execution
      command: "rustc --version"
  guarded-fail:
    title: "Blocked step"
    command: "git --version"
    skippable: false
    precondition:
      type: execution
      command: "git --no-such-flag-xyz"
    depends_on: [guarded]
workflows:
  default:
    steps: [guarded, guarded-fail]
"#;

// ===========================================================================
// --only flag
// ===========================================================================

/// `--only check-tools,analyze-source` runs only those 2 steps.
#[test]
fn only_runs_specified_steps() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(
        &["run", "--only", "check-tools,analyze-source"],
        temp.path(),
    );

    expect_or_dump(&mut s, "FlagTest", "Header");
    expect_or_dump(&mut s, "Check development tools", "check-tools step title");
    expect_or_dump(&mut s, "Analyze source files", "analyze-source step title");
    expect_or_dump(
        &mut s,
        "2 run · 0 skipped",
        "Summary shows 2 steps run, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // generate-report was not run, so no side-effect file
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "--only should not run generate-report"
    );
}

// ===========================================================================
// --skip flag
// ===========================================================================

/// `--skip analyze-source` excludes that step (and dependents via default
/// skip_with_dependents behavior).
#[test]
fn skip_excludes_steps() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--skip", "analyze-source"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "Header");
    expect_or_dump(&mut s, "Check development tools", "check-tools step runs");
    expect_or_dump(&mut s, "Inspect repository", "inspect-repo step runs");
    // analyze-source is skipped; generate-report depends on it so is also
    // skipped via skip_with_dependents default behavior.
    expect_or_dump(
        &mut s,
        "2 run · 2 skipped",
        "2 steps ran (check-tools + inspect-repo), 2 skipped (analyze-source + generate-report)",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // generate-report was skipped, so no side-effect file
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "--skip should also skip dependents, so generate-report should not run"
    );
}

// ===========================================================================
// --force flag
// ===========================================================================

/// First run completes all steps, second run with `--force scan-files` forces
/// re-run of that step without prompting.
#[test]
fn force_reruns_completed_steps() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);

    // First run: use --non-interactive to establish completion history
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());
    expect_or_dump(&mut s, "InteractiveFlags", "First run header");
    expect_or_dump(
        &mut s,
        "3 run · 0 skipped",
        "First run completes all 3 steps",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Second run with --force scan-files --non-interactive
    // The forced step should re-run without prompting.
    let mut s = spawn_bivvy(
        &["run", "--force", "scan-files", "--non-interactive"],
        temp.path(),
    );

    expect_or_dump(&mut s, "InteractiveFlags", "Second run header");
    expect_or_dump(&mut s, "Scan project files", "Forced step runs");
    expect_or_dump(
        &mut s,
        "3 run · 0 skipped",
        "All 3 steps run (forced + non-interactive)",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --non-interactive flag
// ===========================================================================

/// `--non-interactive` runs all skippable steps without prompting.
#[test]
fn non_interactive_skips_prompts() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    // Should complete without sending any keys at all
    expect_or_dump(&mut s, "InteractiveFlags", "Header");
    expect_or_dump(
        &mut s,
        "3 run · 0 skipped",
        "Summary shows all 3 steps run, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --dry-run flag
// ===========================================================================

/// `--dry-run` shows a plan preview without executing any steps.
#[test]
fn dry_run_shows_plan_without_executing() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run message",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // No side effects should be created
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "Dry run must not create side-effect files"
    );
}

// ===========================================================================
// --env flag
// ===========================================================================

/// `--env development` includes dev-only step and excludes ci-only step.
#[test]
fn env_flag_passes_environment() {
    let temp = setup_project_with_git(ENV_CONFIG);
    let mut s = spawn_bivvy(&["run", "--env", "development"], temp.path());

    expect_or_dump(&mut s, "EnvTest", "Header");
    // always-run + dev-only = 2 steps; ci-only is excluded
    expect_or_dump(
        &mut s,
        "2 run · 0 skipped",
        "Summary shows 2 steps for dev env, ci-only excluded",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --verbose / -v flag
// ===========================================================================

/// `--verbose` shows step titles and command details in output.
#[test]
fn verbose_shows_extra_output() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Run with --verbose — should show step titles and detailed output
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());
    expect_or_dump(&mut s, "FlagTest", "Verbose header");
    expect_or_dump(&mut s, "Check development tools", "Verbose shows step title");
    expect_or_dump(&mut s, "Inspect repository", "Verbose shows inspect-repo title");
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "All 4 steps ran, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --quiet / -q flag
// ===========================================================================

/// `--quiet` suppresses step titles and verbose output, showing only the
/// summary. Quiet output must not contain step titles but must still show
/// the final summary line.
#[test]
fn quiet_reduces_output() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Run with --quiet — should suppress step titles and details
    let mut quiet_s = spawn_bivvy(&["run", "--quiet"], temp.path());
    let quiet_output = read_to_eof(&mut quiet_s);

    // Quiet mode should NOT show individual step titles
    assert!(
        !quiet_output.contains("Check development tools"),
        "Quiet mode should not show step titles, but found 'Check development tools'. Output:\n{quiet_output}"
    );
    assert!(
        !quiet_output.contains("Inspect repository"),
        "Quiet mode should not show step titles, but found 'Inspect repository'. Output:\n{quiet_output}"
    );

    // But the summary line must still appear (Quiet mode has shows_status == true)
    assert!(
        quiet_output.contains("4 run · 0 skipped"),
        "Quiet mode should still show summary, but did not find '4 run · 0 skipped'. Output:\n{quiet_output}"
    );

    assert_exit_code(&quiet_s, 0);
}

// ===========================================================================
// --no-color flag
// ===========================================================================

/// `--no-color` output contains no ANSI escape sequences.
#[test]
fn no_color_disables_ansi() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--no-color"], temp.path());

    // Verify the workflow completes before checking for ANSI
    expect_or_dump(&mut s, "FlagTest", "No-color header");
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "All 4 steps ran, none skipped",
    );

    // Read raw PTY output (don't use read_to_eof which strips ANSI)
    let eof_match = s.expect(expectrl::Eof).unwrap();
    let raw_output = String::from_utf8_lossy(eof_match.as_bytes()).to_string();

    // The raw output should contain no \x1b (ESC) characters
    assert!(
        !raw_output.contains('\x1b'),
        "Output with --no-color should contain no ANSI escape sequences, but found \\x1b in output"
    );

    assert_exit_code(&s, 0);
}

// ===========================================================================
// --debug flag
// ===========================================================================

/// `--debug` shows debug-level information including config loading details.
#[test]
fn debug_shows_debug_info() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--debug"], temp.path());

    // Debug mode should show the header and complete the workflow
    expect_or_dump(&mut s, "FlagTest", "Debug header");
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "All 4 steps ran in debug mode",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --config / -c flag
// ===========================================================================

/// `--config <PATH>` loads configuration from a non-standard location.
#[test]
fn config_flag_overrides_path() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Move config to a non-standard location
    let custom_path = temp.path().join("custom-config.yml");
    fs::copy(temp.path().join(".bivvy/config.yml"), &custom_path).unwrap();

    // Remove the default config so bivvy MUST use the custom one
    fs::remove_file(temp.path().join(".bivvy/config.yml")).unwrap();

    let mut s = spawn_bivvy(
        &["run", "--config", custom_path.to_str().unwrap()],
        temp.path(),
    );

    expect_or_dump(
        &mut s,
        "FlagTest",
        "Config flag header — loaded from custom path",
    );
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "All 4 steps ran from custom config, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --project / -p flag
// ===========================================================================

/// `--project <PATH>` runs bivvy against a project in a different directory.
#[test]
fn project_flag_overrides_project_root() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Create a separate directory to run bivvy from.  `spawn_bivvy`
    // will automatically pin HOME to `<other_dir>/.test_home` so we
    // still get full isolation even though the project path is
    // elsewhere.
    let other_dir = TempDir::new().unwrap();

    // Spawn in the "other_dir" cwd and pass --project pointing at the
    // real project.
    let mut s = spawn_bivvy(
        &["run", "--project", temp.path().to_str().unwrap()],
        other_dir.path(),
    );

    expect_or_dump(
        &mut s,
        "FlagTest",
        "Project flag header — loaded from --project path",
    );
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "All 4 steps ran via --project, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --workflow flag
// ===========================================================================

/// `--workflow quick` selects the quick workflow (2 steps).
#[test]
fn workflow_flag_selects_workflow() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "quick"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "Quick workflow header");
    expect_or_dump(
        &mut s,
        "2 run · 0 skipped",
        "Summary shows 2 steps for quick workflow",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // generate-report not in quick workflow — no side-effect file
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "Quick workflow should not run generate-report"
    );
}

// ===========================================================================
// --ci flag
// ===========================================================================

/// `--ci` flag is accepted (deprecated alias for --non-interactive).
#[test]
fn ci_flag_accepted() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--ci"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "CI flag header");
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "Summary shows all 4 steps run, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `--ci` with interactive config runs without prompting.
#[test]
fn ci_flag_non_interactive_behavior() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--ci"], temp.path());

    // Should complete without interactive input
    expect_or_dump(&mut s, "InteractiveFlags", "CI flag header");
    expect_or_dump(
        &mut s,
        "3 run · 0 skipped",
        "All 3 steps run non-interactively",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// --force with all steps (force-all pattern)
// ===========================================================================

/// `--force` with all step names forces re-run of every completed step.
#[test]
fn force_all_steps_bypasses_completed_checks() {
    let config = r#"
app_name: "ForceAllTest"
steps:
  step-a:
    title: "Step A"
    command: "rustc --version && cargo --version"
    skippable: false
    check:
      type: execution
      command: "rustc --version"
  step-b:
    title: "Step B"
    command: "git --version && git rev-parse HEAD"
    skippable: false
    check:
      type: execution
      command: "git --version"
    depends_on: [step-a]
workflows:
  default:
    steps: [step-a, step-b]
"#;
    let temp = setup_project_with_git(config);

    // First run to establish completion (HOME isolation is automatic).
    run_bivvy_silently(temp.path(), &["run", "--non-interactive"]);

    // Second run forcing all steps
    let mut s = spawn_bivvy(&["run", "--force", "step-a,step-b"], temp.path());

    expect_or_dump(&mut s, "ForceAllTest", "Force-all header");
    expect_or_dump(
        &mut s,
        "2 run · 0 skipped",
        "Both steps ran despite being complete",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// Combined flags
// ===========================================================================

/// `--non-interactive --quiet` runs without prompts and with minimal output.
#[test]
fn non_interactive_with_quiet() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);

    // Run quiet + non-interactive — should suppress step titles
    let mut quiet_s = spawn_bivvy(&["run", "--non-interactive", "--quiet"], temp.path());
    let quiet_output = read_to_eof(&mut quiet_s);

    // Quiet mode should NOT show individual step titles
    assert!(
        !quiet_output.contains("Scan project files"),
        "Quiet non-interactive should not show step titles, but found 'Scan project files'. Output:\n{quiet_output}"
    );
    assert!(
        !quiet_output.contains("Check version info"),
        "Quiet non-interactive should not show step titles, but found 'Check version info'. Output:\n{quiet_output}"
    );

    // But the summary line must still appear (Quiet mode has shows_status == true)
    assert!(
        quiet_output.contains("3 run · 0 skipped"),
        "Quiet non-interactive should still show summary, but did not find '3 run · 0 skipped'. Output:\n{quiet_output}"
    );

    assert_exit_code(&quiet_s, 0);
}

/// `--verbose --no-color` shows verbose output without ANSI escapes.
#[test]
fn verbose_with_no_color() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose", "--no-color"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "Verbose no-color header");
    expect_or_dump(&mut s, "Check development tools", "Step title visible in verbose no-color");
    expect_or_dump(
        &mut s,
        "4 run · 0 skipped",
        "All 4 steps ran, none skipped",
    );

    // Read raw PTY output (don't use read_to_eof which strips ANSI)
    let eof_match = s.expect(expectrl::Eof).unwrap();
    let raw_output = String::from_utf8_lossy(eof_match.as_bytes()).to_string();

    // No ANSI escape characters
    assert!(
        !raw_output.contains('\x1b'),
        "Output with --no-color should contain no ANSI escape sequences"
    );

    assert_exit_code(&s, 0);
}

// ===========================================================================
// Exit code tests for flag combinations
// ===========================================================================

/// `--only` with valid step exits 0.
#[test]
fn exit_code_only_flag_success() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    bivvy_assert_cmd(temp.path())
        .args(["run", "--only", "check-tools"])
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("1 run · 0 skipped"));
}

/// `--dry-run` always exits 0 (no execution).
#[test]
fn exit_code_dry_run_always_zero() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    bivvy_assert_cmd(temp.path())
        .args(["run", "--dry-run"])
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains(
            "Running in dry-run mode - no commands will be executed",
        ));
}

/// Failing step with --non-interactive exits with code 1 and surfaces the
/// failing step's title in output.
#[test]
fn exit_code_failing_step_non_zero() {
    let temp = setup_project(FAILING_CONFIG);
    // The failing step's user-facing title is "Failing step" — assert on
    // the exact title, not a partial / alternative match.
    bivvy_assert_cmd(temp.path())
        .args(["run", "--non-interactive"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("Failing step"));
}

/// `--quiet --non-interactive` with successful workflow exits 0, produces
/// no step titles, and still emits the summary line.
#[test]
fn exit_code_quiet_non_interactive_success() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    bivvy_assert_cmd(temp.path())
        .args(["run", "--quiet", "--non-interactive"])
        .assert()
        .success()
        .code(0)
        .stdout(
            predicate::str::contains("Check development tools")
                .not()
                .and(predicate::str::contains("4 run · 0 skipped")),
        );
}

/// `--ci` flag exits 0 on success.
#[test]
fn exit_code_ci_flag_success() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    bivvy_assert_cmd(temp.path())
        .args(["run", "--ci"])
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("4 run · 0 skipped"));
}

// ===========================================================================
// Sensitive step with flags
// ===========================================================================

/// `--non-interactive` with sensitive step masks output.
#[test]
fn sensitive_step_non_interactive_masks_output() {
    let temp = setup_project(SENSITIVE_FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive", "--verbose"], temp.path());

    // Normal step output should be visible
    expect_or_dump(&mut s, "Prepare environment", "Prepare step title");
    // Sensitive step should show [SENSITIVE] marker for its command
    expect_or_dump(&mut s, "[SENSITIVE]", "Sensitive step command is masked");
    expect_or_dump(
        &mut s,
        "2 run · 0 skipped",
        "Both steps ran, none skipped",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `--dry-run` with sensitive step should mask command text.
#[test]
fn sensitive_step_dry_run_masks_command() {
    let temp = setup_project(SENSITIVE_FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run", "--verbose"], temp.path());

    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run message",
    );
    // In dry-run mode, sensitive step commands should show "Would run: [SENSITIVE]"
    expect_or_dump(&mut s, "Would run: [SENSITIVE]", "Sensitive command masked in dry-run");
    expect_or_dump(
        &mut s,
        "2 run · 0 skipped",
        "Both steps reported in summary",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// ===========================================================================
// Precondition with flags
// ===========================================================================

/// `--force` does not bypass preconditions.
#[test]
fn force_flag_does_not_bypass_precondition() {
    let temp = setup_project_with_git(PRECONDITION_FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--force", "guarded-fail"], temp.path());

    // First step (guarded) should pass its precondition and run
    expect_or_dump(&mut s, "Guarded step", "First step runs with passing precondition");
    // Second step (guarded-fail) has precondition that fails — should report failure
    // The executor formats this as "Precondition failed: command '<cmd>' failed"
    expect_or_dump(
        &mut s,
        "Precondition failed",
        "Precondition failure despite --force",
    );
    s.expect(expectrl::Eof).unwrap();
    // Failed precondition should produce a non-zero exit code.
    assert_exit_code(&s, 1);
}

/// Precondition with --non-interactive still enforces the check.
#[test]
fn precondition_enforced_in_non_interactive() {
    let temp = setup_project_with_git(PRECONDITION_FLAG_CONFIG);
    // guarded-fail has precondition `git --no-such-flag-xyz` which always
    // fails, so the workflow must surface the precondition failure in
    // output AND exit 1.  We assert on both via `assert_cmd` chaining so
    // the test cannot pass without the specific failure message appearing.
    bivvy_assert_cmd(temp.path())
        .args(["run", "--non-interactive"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("Precondition failed"));
}
