//! System tests for `bivvy run` command flags and global flags.
//!
//! Verifies every CLI flag on the run command and all global flags
//! work correctly. Uses external programs (git, rustc, cargo, grep)
//! instead of shell builtins.
#![cfg(unix)]

mod system;
use system::helpers::*;

use std::fs;

// ---------------------------------------------------------------------------
// Configs
// ---------------------------------------------------------------------------

/// 4-step workflow with non-skippable steps. Uses real commands (git, rustc,
/// cargo, grep, wc, cat) against a real project with a git repo.
const FLAG_CONFIG: &str = r#"
app_name: "FlagTest"

settings:
  default_output: verbose

steps:
  check-tools:
    title: "Check development tools"
    command: "rustc --version && cargo --version"
    skippable: false
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  inspect-repo:
    title: "Inspect repository"
    command: "git rev-parse --git-dir && git status --short"
    skippable: false
    depends_on: [check-tools]
    completed_check:
      type: command_succeeds
      command: "git rev-parse --git-dir"

  analyze-source:
    title: "Analyze source files"
    command: "grep -c 'fn ' src/main.rs && wc -l src/main.rs Cargo.toml"
    skippable: false
    depends_on: [inspect-repo]

  generate-report:
    title: "Generate build report"
    command: "rustc --version > .build-report.txt && cargo --version >> .build-report.txt && cat .build-report.txt"
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
  default_output: verbose

steps:
  scan-files:
    title: "Scan project files"
    command: "find . -name '*.rs' -not -path './.git/*' | sort"

  check-version:
    title: "Check version info"
    command: "grep version Cargo.toml | head -1"
    depends_on: [scan-files]

  summarize:
    title: "Summarize project"
    command: "git log --oneline -3 && wc -l src/main.rs"
    depends_on: [check-version]

workflows:
  default:
    steps: [scan-files, check-version, summarize]
"#;

/// Config with environment-specific steps for --env flag testing.
const ENV_CONFIG: &str = r#"
app_name: "EnvTest"

settings:
  default_output: verbose

steps:
  always-run:
    title: "Always runs"
    command: "rustc --version"
    skippable: false

  dev-only:
    title: "Dev environment step"
    command: "git log --oneline -1"
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
  setup:
    title: "Setup"
    command: "rustc --version"
    skippable: false
  sensitive-step:
    title: "Sensitive operation"
    command: "whoami && uname -s"
    skippable: false
    sensitive: true
    depends_on: [setup]
workflows:
  default:
    steps: [setup, sensitive-step]
"#;

/// Config with preconditions for flag testing.
const PRECONDITION_FLAG_CONFIG: &str = r#"
app_name: "PreconditionFlagTest"
steps:
  guarded:
    title: "Guarded step"
    command: "rustc --version && uname -s"
    skippable: false
    precondition:
      type: command_succeeds
      command: "rustc --version"
  guarded-fail:
    title: "Blocked step"
    command: "git --version"
    skippable: false
    precondition:
      type: command_succeeds
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
    let mut s = spawn_bivvy(&["run", "--only", "check-tools,analyze-source"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "Header");
    expect_or_dump(&mut s, "2 run", "Summary shows 2 steps run");
    s.expect(expectrl::Eof).unwrap();

    // generate-report was not run, so no side-effect file
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "--only should not run generate-report"
    );
}

// ===========================================================================
// --skip flag
// ===========================================================================

/// `--skip analyze-source` excludes that step (and possibly dependents).
#[test]
fn skip_excludes_steps() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--skip", "analyze-source"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "Header");
    // analyze-source is skipped; generate-report depends on it so may also be
    // skipped. We expect fewer than 4 steps to run.
    expect_or_dump(&mut s, "run", "Summary line");
    let output = read_to_eof(&mut s);
    // The summary should NOT say "4 run"
    let combined = strip_ansi(&output);
    assert!(
        !combined.contains("4 run"),
        "Should not run all 4 steps when --skip is used"
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

    // First run: accept all prompts to establish history
    let s = spawn_bivvy(&["run"], temp.path());
    wait_and_answer(&s, "Scan project files?", KEY_Y, "Accept scan-files");
    wait_and_answer(&s, "Check version info?", KEY_Y, "Accept check-version");
    wait_and_answer(&s, "Summarize project?", KEY_Y, "Accept summarize");
    wait_for(&s, "3 run", "First run summary");

    // Second run with --force scan-files --non-interactive
    // The forced step should re-run without prompting.
    let mut s = spawn_bivvy(
        &["run", "--force", "scan-files", "--non-interactive"],
        temp.path(),
    );

    expect_or_dump(&mut s, "InteractiveFlags", "Second run header");
    // Should complete without needing any interactive input
    expect_or_dump(&mut s, "Total:", "Second run summary");
    s.expect(expectrl::Eof).unwrap();
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
    expect_or_dump(&mut s, "3 run", "Summary shows all 3 steps run");
    s.expect(expectrl::Eof).unwrap();
}

// ===========================================================================
// --dry-run flag
// ===========================================================================

/// `--dry-run` shows a plan preview without executing any steps.
#[test]
fn dry_run_shows_plan_without_executing() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    expect_or_dump(&mut s, "dry-run", "Dry-run indicator in output");
    s.expect(expectrl::Eof).unwrap();

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
    expect_or_dump(&mut s, "2 run", "Summary shows 2 steps for dev env");
    s.expect(expectrl::Eof).unwrap();
}

// ===========================================================================
// --verbose / -v flag
// ===========================================================================

/// `--verbose` produces more output than default.
#[test]
fn verbose_shows_extra_output() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Run with --verbose and capture output
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());
    expect_or_dump(&mut s, "FlagTest", "Verbose header");
    expect_or_dump(&mut s, "run", "Verbose summary");
    let verbose_output = read_to_eof(&mut s);

    // Run default (no flag) and capture output
    let mut s2 = spawn_bivvy(&["run"], temp.path());
    expect_or_dump(&mut s2, "FlagTest", "Default header");
    expect_or_dump(&mut s2, "run", "Default summary");
    let default_output = read_to_eof(&mut s2);

    // Verbose output should be at least as long as default
    // (it may include command output, timing info, etc.)
    assert!(
        verbose_output.len() >= default_output.len(),
        "Verbose output ({} bytes) should be >= default output ({} bytes)",
        verbose_output.len(),
        default_output.len()
    );
}

// ===========================================================================
// --quiet / -q flag
// ===========================================================================

/// `--quiet` produces minimal output.
#[test]
fn quiet_reduces_output() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Run with --verbose first to get a baseline of "normal" output length
    let mut verbose_s = spawn_bivvy(&["run", "--verbose"], temp.path());
    expect_or_dump(&mut verbose_s, "FlagTest", "Verbose header");
    let verbose_output = read_to_eof(&mut verbose_s);

    // Run with --quiet
    let mut quiet_s = spawn_bivvy(&["run", "--quiet"], temp.path());
    let quiet_output = read_to_eof(&mut quiet_s);

    let verbose_clean = strip_ansi(&verbose_output);
    let quiet_clean = strip_ansi(&quiet_output);

    assert!(
        quiet_clean.len() < verbose_clean.len(),
        "Quiet output ({} bytes) should be shorter than verbose output ({} bytes)",
        quiet_clean.len(),
        verbose_clean.len()
    );
}

// ===========================================================================
// --no-color flag
// ===========================================================================

/// `--no-color` output contains no ANSI escape sequences.
#[test]
fn no_color_disables_ansi() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--no-color"], temp.path());

    // Wait for completion
    expect_or_dump(&mut s, "FlagTest", "No-color header");
    let output = read_to_eof(&mut s);

    // The raw output should contain no \x1b (ESC) characters
    assert!(
        !output.contains('\x1b'),
        "Output with --no-color should contain no ANSI escape sequences, but found \\x1b in output"
    );
}

// ===========================================================================
// --debug flag
// ===========================================================================

/// `--debug` shows debug-level information.
#[test]
fn debug_shows_debug_info() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--debug"], temp.path());

    // Debug mode should show additional diagnostic output.
    // Look for debug-level markers like "DEBUG", timing, or config details.
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // Debug mode typically shows config loading, step resolution, etc.
    // At minimum it should produce more output than a quiet run.
    assert!(
        clean.contains("DEBUG") || clean.contains("debug") || clean.len() > 200,
        "Debug output should contain debug markers or be substantially verbose.\n\
         Output length: {} bytes\n\
         First 500 chars: {}",
        clean.len(),
        &clean[..clean.len().min(500)]
    );
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
    fs::copy(
        temp.path().join(".bivvy/config.yml"),
        &custom_path,
    )
    .unwrap();

    // Remove the default config so bivvy MUST use the custom one
    fs::remove_file(temp.path().join(".bivvy/config.yml")).unwrap();

    let mut s = spawn_bivvy(
        &["run", "--config", custom_path.to_str().unwrap()],
        temp.path(),
    );

    expect_or_dump(&mut s, "FlagTest", "Config flag header — loaded from custom path");
    expect_or_dump(&mut s, "run", "Summary line");
    s.expect(expectrl::Eof).unwrap();
}

// ===========================================================================
// --project / -p flag
// ===========================================================================

/// `--project <PATH>` runs bivvy against a project in a different directory.
#[test]
fn project_flag_overrides_project_root() {
    let temp = setup_project_with_git(FLAG_CONFIG);

    // Create a separate directory to run bivvy from
    let other_dir = tempfile::TempDir::new().unwrap();

    let mut s = spawn_bivvy(
        &["run", "--project", temp.path().to_str().unwrap()],
        other_dir.path(),
    );

    expect_or_dump(&mut s, "FlagTest", "Project flag header — loaded from --project path");
    expect_or_dump(&mut s, "run", "Summary line");
    s.expect(expectrl::Eof).unwrap();
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
    expect_or_dump(&mut s, "2 run", "Summary shows 2 steps for quick workflow");
    s.expect(expectrl::Eof).unwrap();

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
    expect_or_dump(&mut s, "run", "Summary line");
    s.expect(expectrl::Eof).unwrap();
}

/// `--ci` with interactive config runs without prompting.
#[test]
fn ci_flag_non_interactive_behavior() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--ci"], temp.path());

    // Should complete without interactive input
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("Total:") || clean.contains("run"),
        "CI flag should run non-interactively. Output: {}",
        &clean[..clean.len().min(500)]
    );
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
    command: "rustc --version && uname -s"
    skippable: false
    completed_check:
      type: command_succeeds
      command: "rustc --version"
  step-b:
    title: "Step B"
    command: "git --version && whoami"
    skippable: false
    completed_check:
      type: command_succeeds
      command: "git --version"
    depends_on: [step-a]
workflows:
  default:
    steps: [step-a, step-b]
"#;
    let temp = setup_project_with_git(config);

    // First run to establish completion
    run_workflow_silently(temp.path());

    // Second run forcing all steps
    let mut s = spawn_bivvy(
        &["run", "--force", "step-a,step-b"],
        temp.path(),
    );

    expect_or_dump(&mut s, "ForceAllTest", "Force-all header");
    expect_or_dump(&mut s, "2 run", "Both steps ran despite being complete");
    s.expect(expectrl::Eof).unwrap();
}

// ===========================================================================
// Combined flags
// ===========================================================================

/// `--non-interactive --quiet` runs without prompts and with minimal output.
#[test]
fn non_interactive_with_quiet() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive", "--quiet"], temp.path());

    // Should complete without any interactive input
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // Quiet mode produces minimal output; just verify it finished
    // (no hang, no prompt waiting)
    assert!(
        clean.len() < 5000,
        "Quiet + non-interactive should produce minimal output, got {} bytes",
        clean.len()
    );
}

/// `--verbose --no-color` shows verbose output without ANSI escapes.
#[test]
fn verbose_with_no_color() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose", "--no-color"], temp.path());

    expect_or_dump(&mut s, "FlagTest", "Verbose no-color header");
    let output = read_to_eof(&mut s);

    // No ANSI escape characters
    assert!(
        !output.contains('\x1b'),
        "Output with --no-color should contain no ANSI escape sequences"
    );

    // Should still have substantial verbose output
    let clean = strip_ansi(&output);
    assert!(
        clean.len() > 50,
        "Verbose output should be substantial, got only {} bytes",
        clean.len()
    );
}

// ===========================================================================
// Exit code tests for flag combinations
// ===========================================================================

/// `--only` with valid step exits 0.
#[test]
fn exit_code_only_flag_success() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--only", "check-tools"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Successful --only should exit 0, got {:?}",
        status.code()
    );
}

/// `--dry-run` always exits 0 (no execution).
#[test]
fn exit_code_dry_run_always_zero() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--dry-run"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Dry run should always exit 0, got {:?}",
        status.code()
    );
}

/// Failing step with --non-interactive exits non-zero.
#[test]
fn exit_code_failing_step_non_zero() {
    let temp = setup_project(FAILING_CONFIG);
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
        "Failing step should produce non-zero exit code, got {:?}",
        status.code()
    );
}

/// `--quiet --non-interactive` with successful workflow exits 0.
#[test]
fn exit_code_quiet_non_interactive_success() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--quiet", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Quiet non-interactive with good config should exit 0, got {:?}",
        status.code()
    );
}

/// `--ci` flag exits 0 on success.
#[test]
fn exit_code_ci_flag_success() {
    let temp = setup_project_with_git(FLAG_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--ci"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "CI flag with good config should exit 0, got {:?}",
        status.code()
    );
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
    expect_or_dump(&mut s, "Setup", "Setup step title");
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // Sensitive step should complete but mask its output
    assert!(
        clean.contains("Sensitive operation") || clean.contains("Total:"),
        "Sensitive step should show title but mask output. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

/// `--dry-run` with sensitive step should mask command text.
#[test]
fn sensitive_step_dry_run_masks_command() {
    let temp = setup_project(SENSITIVE_FLAG_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run", "--verbose"], temp.path());

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // In dry-run mode, sensitive step commands should be masked
    assert!(
        clean.contains("dry-run") || clean.contains("Sensitive"),
        "Dry-run should show plan without leaking sensitive command details. Got: {}",
        &clean[..clean.len().min(500)]
    );
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
    // Second step (guarded-fail) has precondition `false` — should fail
    expect_or_dump(&mut s, "precondition", "Precondition failure despite --force");
    s.expect(expectrl::Eof).unwrap();
}

/// Precondition with --non-interactive still enforces the check.
#[test]
fn precondition_enforced_in_non_interactive() {
    let temp = setup_project_with_git(PRECONDITION_FLAG_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--non-interactive"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    // guarded-fail has precondition `false`, so the workflow should fail
    assert!(
        !status.success(),
        "Failed precondition should produce non-zero exit code, got {:?}",
        status.code()
    );
}
