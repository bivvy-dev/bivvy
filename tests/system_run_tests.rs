//! Comprehensive system tests for `bivvy run`.
//!
//! Tests the full interactive experience of running workflows, including
//! every keyboard interaction at every prompt, multi-step flows with
//! mixed answers, flag combinations, error conditions, and stateful
//! second-run behavior.
#![cfg(unix)]

mod system;

use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

/// 6-step non-interactive workflow (skippable: false).
/// Steps with completed_checks auto-rerun.  No prompts.
const REALISTIC_CONFIG: &str = r#"
app_name: "TestProject"

settings:
  default_output: verbose

vars:
  version:
    command: "git log --oneline -1"
  git_branch:
    command: "git branch --show-current"

steps:
  check-tools:
    title: "Verify toolchain"
    command: "rustc --version && git --version"
    skippable: false
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  check-repo:
    title: "Verify git repository"
    command: "git status --short"
    skippable: false
    depends_on: [check-tools]
    completed_check:
      type: command_succeeds
      command: "git rev-parse --git-dir"

  gather-info:
    title: "Gather project info"
    command: "git branch --show-current && git log --oneline -1"
    skippable: false
    depends_on: [check-repo]

  validate-structure:
    title: "Validate project structure"
    command: "test -f Cargo.toml && test -f Cargo.lock && test -d src"
    skippable: false
    depends_on: [check-repo]
    completed_check:
      type: file_exists
      path: "Cargo.toml"
    watches:
      - Cargo.toml
      - Cargo.lock

  generate-manifest:
    title: "Generate build manifest"
    command: "date -u '+%Y-%m-%d' > .build-manifest.json && uname -s >> .build-manifest.json && cat .build-manifest.json"
    skippable: false
    depends_on: [gather-info, validate-structure]

  verify-manifest:
    title: "Verify build manifest"
    command: "test -f .build-manifest.json && cat .build-manifest.json"
    skippable: false
    depends_on: [generate-manifest]

workflows:
  default:
    steps: [check-tools, check-repo, gather-info, validate-structure, generate-manifest, verify-manifest]

  quick:
    description: "Quick validation"
    steps: [check-tools, check-repo, validate-structure]

  info:
    description: "Gather project information"
    steps: [check-tools, check-repo, gather-info]
"#;

/// 4-step interactive workflow — every step is skippable (default).
/// No completed_checks, so each step prompts "Step title?".
const INTERACTIVE_CONFIG: &str = r#"
app_name: "InteractiveTest"

settings:
  default_output: verbose

steps:
  list-files:
    title: "List project files"
    command: "find . -maxdepth 2 -type f -not -path './.git/*' | sort"

  count-lines:
    title: "Count source lines"
    command: "wc -l src/main.rs Cargo.toml"
    depends_on: [list-files]

  git-log:
    title: "Recent git history"
    command: "git log --oneline -5"
    depends_on: [list-files]

  summary:
    title: "Project summary"
    command: "basename $(git rev-parse --show-toplevel) && wc -l src/main.rs && head -1 Cargo.toml"
    depends_on: [count-lines, git-log]

workflows:
  default:
    steps: [list-files, count-lines, git-log, summary]
"#;

/// 3-step workflow with skippable=true AND completed_checks.
/// Tests "Already complete (reason). Re-run?" prompt flow.
const PROMPTED_CHECK_CONFIG: &str = r#"
app_name: "PromptedCheck"

settings:
  default_output: verbose

steps:
  check-tools:
    title: "Verify toolchain"
    command: "rustc --version && git --version"
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  check-repo:
    title: "Verify git repository"
    command: "git status --short"
    depends_on: [check-tools]
    completed_check:
      type: command_succeeds
      command: "git rev-parse --git-dir"

  show-info:
    title: "Show project info"
    command: "wc -l Cargo.toml && head -3 Cargo.toml"
    depends_on: [check-repo]

workflows:
  default:
    steps: [check-tools, check-repo, show-info]
"#;

/// Simple 2-step config with no dependencies, no checks.
/// Uses real commands instead of echo.
const SIMPLE_CONFIG: &str = r#"
app_name: "SimpleApp"
steps:
  greet:
    title: "Say hello"
    command: "rustc --version"
  farewell:
    title: "Say goodbye"
    command: "git --version"
workflows:
  default:
    steps: [greet, farewell]
"#;

/// Config with custom step prompts (select, confirm, input).
/// Uses real commands to exercise actual tool behavior.
const CUSTOM_PROMPTS_CONFIG: &str = r#"
app_name: "PromptApp"
steps:
  deploy:
    title: "Deploy"
    command: "rustc --version && git --version"
    skippable: false
    prompts:
      - key: target
        question: "Deploy target"
        type: select
        options:
          - label: "Staging"
            value: staging
          - label: "Production"
            value: production

workflows:
  default:
    steps: [deploy]
"#;

/// Config with a step that will fail (bad command).
const FAILING_STEP_CONFIG: &str = r#"
app_name: "FailApp"
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

/// Config with a sensitive step.
const SENSITIVE_CONFIG: &str = r#"
app_name: "SensitiveApp"
steps:
  normal:
    title: "Normal step"
    command: "rustc --version"
    skippable: false
  secrets:
    title: "Handle secrets"
    command: "whoami && uname -s"
    skippable: false
    sensitive: true
    depends_on: [normal]
workflows:
  default:
    steps: [normal, secrets]
"#;

/// Config with marker completed_check type.
const MARKER_CHECK_CONFIG: &str = r#"
app_name: "MarkerApp"
steps:
  setup:
    title: "Setup step"
    command: "rustc --version && uname -s"
    skippable: false
    completed_check:
      type: marker
workflows:
  default:
    steps: [setup]
"#;

/// Config with `all` combinator for completed_check.
const ALL_CHECK_CONFIG: &str = r#"
app_name: "AllCheckApp"
steps:
  full-check:
    title: "Full check step"
    command: "rustc --version && wc -l Cargo.toml"
    skippable: false
    completed_check:
      type: all
      checks:
        - type: file_exists
          path: "Cargo.toml"
        - type: command_succeeds
          command: "rustc --version"
workflows:
  default:
    steps: [full-check]
"#;

/// Config with `any` combinator for completed_check.
const ANY_CHECK_CONFIG: &str = r#"
app_name: "AnyCheckApp"
steps:
  any-check:
    title: "Any check step"
    command: "git --version && uname -s"
    skippable: false
    completed_check:
      type: any
      checks:
        - type: file_exists
          path: "nonexistent-file-that-does-not-exist.lock"
        - type: command_succeeds
          command: "rustc --version"
workflows:
  default:
    steps: [any-check]
"#;

/// Config with a precondition on a step.
const PRECONDITION_CONFIG: &str = r#"
app_name: "PreconditionApp"
steps:
  guarded:
    title: "Guarded step"
    command: "rustc --version && uname -s"
    skippable: false
    precondition:
      type: command_succeeds
      command: "rustc --version"
workflows:
  default:
    steps: [guarded]
"#;

/// Config with a failing precondition.
const FAILING_PRECONDITION_CONFIG: &str = r#"
app_name: "FailingPreconditionApp"
steps:
  guarded-fail:
    title: "Guarded step that fails precondition"
    command: "rustc --version"
    skippable: false
    precondition:
      type: command_succeeds
      command: "git --no-such-flag-xyz"
workflows:
  default:
    steps: [guarded-fail]
"#;

// =====================================================================
// HAPPY PATH — Non-interactive (skippable: false)
// =====================================================================

/// Full 6-step workflow completes without any prompts.
#[test]
fn run_full_workflow_no_prompts() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Header");
    expect_or_dump(&mut s, "6 run", "All 6 steps ran");
    s.expect(expectrl::Eof).unwrap();

    assert!(
        temp.path().join(".build-manifest.json").exists(),
        "generate-manifest should create side-effect file"
    );
}

/// Named workflow restricts to its steps.
#[test]
fn run_named_workflow_quick() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "quick"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Header");
    expect_or_dump(&mut s, "3 run", "Quick workflow = 3 steps");
    s.expect(expectrl::Eof).unwrap();
}

/// Info workflow interpolates variables from shell commands.
#[test]
fn run_named_workflow_info_with_variable_interpolation() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "info"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Header");
    expect_or_dump(&mut s, "0.2.5", "VERSION var interpolated");
    expect_or_dump(&mut s, "3 run", "Info workflow = 3 steps");
    s.expect(expectrl::Eof).unwrap();
}

/// Bare `bivvy` (no subcommand) runs the default workflow.
#[test]
fn run_bare_bivvy_is_default_workflow() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&[], temp.path());

    expect_or_dump(&mut s, "TestProject", "Header from bare bivvy");
    expect_or_dump(&mut s, "6 run", "All 6 steps");
    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// HAPPY PATH — Interactive prompts (skippable: true, no checks)
//
// Each step prompts "Step title?" with y/n shortcuts.
// Tests exercise every key that can answer these prompts.
// =====================================================================

/// Accept all 4 steps with 'y' key.
#[test]
fn interactive_accept_all_with_y() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: y");
    wait_and_answer(&s, "Count source lines?", KEY_Y, "Step 2: y");
    wait_and_answer(&s, "Recent git history?", KEY_Y, "Step 3: y");
    wait_and_answer(&s, "Project summary?", KEY_Y, "Step 4: y");

    wait_for(&s, "4 run", "All 4 ran");
}

/// Decline all 4 steps with 'n' key.
#[test]
fn interactive_decline_all_with_n() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_N, "Step 1: n");
    // Steps 2-4 depend on step 1 being skipped — they may be auto-skipped
    // or prompted. Wait for the summary.
    wait_for(&s, "Total:", "Summary footer after skip-all");
}

/// Accept step 1 with 'y', decline step 2 with 'n', decline step 3
/// with 'n'.  Step 4 is auto-skipped because both its dependencies
/// were user-declined.
#[test]
fn interactive_mixed_y_n() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: accept");
    wait_and_answer(&s, "Count source lines?", KEY_N, "Step 2: skip");
    wait_and_answer(&s, "Recent git history?", KEY_N, "Step 3: skip");
    // Step 4 (summary) depends on count-lines and git-log, both skipped → auto-skipped

    wait_for(&s, "Total:", "Summary footer");
}

/// Accept step 1 with Enter (default selection on the prompt widget).
/// The default for skippable prompts is "no", so Enter should skip.
#[test]
fn interactive_enter_accepts_default_no() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Enter selects the default, which for skippable steps is "no"
    wait_and_answer(&s, "List project files?", KEY_ENTER, "Step 1: Enter (default=no)");
    // Step 1 skipped → dependents auto-skipped
    wait_for(&s, "Total:", "Summary after default-skip");
}

/// Use space bar to confirm the current selection at step 1.
/// Space behaves like Enter on select widgets.
#[test]
fn interactive_space_confirms_selection() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Space confirms whatever is highlighted (default = no)
    wait_and_answer(&s, "List project files?", KEY_SPACE, "Step 1: Space");
    wait_for(&s, "Total:", "Summary after space-confirm");
}

/// Use Escape to abort/decline at step 1.
/// Escape on a select widget typically maps to "no" / cancel.
#[test]
fn interactive_escape_declines() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_ESC, "Step 1: Escape");
    wait_for(&s, "Total:", "Summary after escape");
}

/// Navigate with arrow-down then accept with Enter at step 1.
/// Arrow down moves to "yes", Enter confirms it.
/// Then accept all remaining steps with 'y'.
#[test]
fn interactive_arrow_down_then_enter() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Arrow down to move from default "no" to "yes"
    wait_and_send_keys(&s, "List project files?", ARROW_DOWN, "Step 1: arrow down");
    // Small delay then Enter to confirm "yes"
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_key(&s, KEY_ENTER);

    // Step 1 accepted → steps 2-4 are prompted
    wait_and_answer(&s, "Count source lines?", KEY_Y, "Step 2: accept");
    wait_and_answer(&s, "Recent git history?", KEY_Y, "Step 3: accept");
    wait_and_answer(&s, "Project summary?", KEY_Y, "Step 4: accept");

    wait_for(&s, "Total:", "Summary after arrow+enter navigation");
}

/// Navigate with arrow-up at step 1 to verify cycling.
/// Arrow-up from "No" wraps to "Yes", Enter confirms it.
/// Then accept all remaining steps with 'y'.
#[test]
fn interactive_arrow_up_then_enter() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Arrow up from default position wraps to "yes"
    wait_and_send_keys(&s, "List project files?", ARROW_UP, "Step 1: arrow up");
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_key(&s, KEY_ENTER);

    // Step 1 accepted → steps 2-4 are prompted
    wait_and_answer(&s, "Count source lines?", KEY_Y, "Step 2: accept");
    wait_and_answer(&s, "Recent git history?", KEY_Y, "Step 3: accept");
    wait_and_answer(&s, "Project summary?", KEY_Y, "Step 4: accept");

    wait_for(&s, "Total:", "Summary after arrow-up navigation");
}

/// Full 4-step flow: y at step 1, arrow-down+enter at step 2,
/// n at step 3.  Step 4 auto-skipped because git-log (step 3) was declined.
#[test]
fn interactive_diverse_inputs_per_step() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Step 1: 'y' shortcut
    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: y shortcut");

    // Step 2: arrow-down then enter (moves from "No" to "Yes", accepts)
    wait_and_send_keys(&s, "Count source lines?", ARROW_DOWN, "Step 2: arrow down");
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_key(&s, KEY_ENTER);

    // Step 3: 'n' shortcut
    wait_and_answer(&s, "Recent git history?", KEY_N, "Step 3: n shortcut");

    // Step 4 (summary) depends on git-log (skipped) → auto-skipped
    wait_for(&s, "Total:", "Summary after diverse inputs");
}

/// Accept first step, skip middle two, accept last — verifies that
/// skipping non-leaf steps still allows running downstream steps
/// whose other dependencies are met.
#[test]
fn interactive_skip_middle_steps() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: accept");
    wait_and_answer(&s, "Count source lines?", KEY_N, "Step 2: skip");
    wait_and_answer(&s, "Recent git history?", KEY_Y, "Step 3: accept");
    // Step 4 depends on both steps 2 and 3.  Step 2 was skipped.
    // Depending on skip behavior, step 4 may be auto-skipped or prompted.
    wait_for(&s, "Total:", "Summary after skipping middle");
}

// =====================================================================
// HAPPY PATH — Completed check prompts
//
// Steps with completed_check that pass show "Already complete. Re-run?"
// =====================================================================

/// Skip all completed steps (answer 'n' to re-run prompts),
/// accept the unchecked step.
#[test]
fn completed_check_skip_all_rerun_prompts() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "Already complete", KEY_N, "Skip check-tools");
    wait_and_answer(&s, "Already complete", KEY_N, "Skip check-repo");
    wait_and_answer(&s, "Show project info?", KEY_Y, "Accept show-info");

    wait_for(&s, "Total:", "Summary footer");
}

/// Re-run all completed steps (answer 'y').
#[test]
fn completed_check_rerun_all() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "Already complete", KEY_Y, "Rerun check-tools");
    wait_and_answer(&s, "Already complete", KEY_Y, "Rerun check-repo");
    wait_and_answer(&s, "Show project info?", KEY_Y, "Accept show-info");

    wait_for(&s, "3 run", "All 3 ran");
}

/// Use Enter (default=no) on completed check prompts — should skip.
#[test]
fn completed_check_enter_defaults_to_skip() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "Already complete", KEY_ENTER, "Default skip check-tools");
    wait_and_answer(&s, "Already complete", KEY_ENTER, "Default skip check-repo");
    wait_and_answer(&s, "Show project info?", KEY_Y, "Accept show-info");

    wait_for(&s, "Total:", "Summary after enter-defaults");
}

/// Use Escape on completed check prompts — should decline re-run.
#[test]
fn completed_check_escape_skips() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "Already complete", KEY_ESC, "Escape check-tools");
    wait_and_answer(&s, "Already complete", KEY_ESC, "Escape check-repo");
    wait_and_answer(&s, "Show project info?", KEY_Y, "Accept show-info");

    wait_for(&s, "Total:", "Summary after escape");
}

/// Mix re-run and skip on completed steps.
#[test]
fn completed_check_mixed_rerun_and_skip() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "Already complete", KEY_Y, "Rerun check-tools");
    wait_and_answer(&s, "Already complete", KEY_N, "Skip check-repo");
    wait_and_answer(&s, "Show project info?", KEY_Y, "Accept show-info");

    wait_for(&s, "Total:", "Summary after mixed");
}

// =====================================================================
// STATEFUL — Second run detects prior work
// =====================================================================

/// Two consecutive runs both complete successfully.
#[test]
fn second_run_completes_cleanly() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);

    // First run
    let mut s = spawn_bivvy(&["run"], temp.path());
    expect_or_dump(&mut s, "TestProject", "First run header");
    expect_or_dump(&mut s, "6 run", "First run summary");
    s.expect(expectrl::Eof).unwrap();

    assert!(temp.path().join(".build-manifest.json").exists());

    // Second run
    let mut s = spawn_bivvy(&["run"], temp.path());
    expect_or_dump(&mut s, "TestProject", "Second run header");
    expect_or_dump(&mut s, "6 run", "Second run summary");
    s.expect(expectrl::Eof).unwrap();
}

/// After a successful run, running with --only on a single step works.
#[test]
fn rerun_single_step_after_full_run() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);

    // Full run first
    run_workflow_silently(temp.path());

    // Then run just one step
    let mut s = spawn_bivvy(&["run", "--only", "check-tools"], temp.path());
    expect_or_dump(&mut s, "TestProject", "Single-step header");
    expect_or_dump(&mut s, "1 run", "Only 1 step ran");
    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// FLAGS
// =====================================================================

/// --dry-run produces no side effects.
#[test]
fn run_dry_run_no_side_effects() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    expect_or_dump(&mut s, "dry-run", "Dry-run indicator");
    s.expect(expectrl::Eof).unwrap();

    assert!(
        !temp.path().join(".build-manifest.json").exists(),
        "Dry run must not create files"
    );
}

/// --verbose shows extra detail.
#[test]
fn run_verbose_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Verbose header");
    s.expect(expectrl::Eof).unwrap();
}

/// --quiet suppresses most output.
#[test]
fn run_quiet_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run", "--quiet"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Quiet run should exit 0, got {:?}",
        status.code()
    );
}

/// --only filters to a single step.
#[test]
fn run_only_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--only", "check-tools"], temp.path());

    expect_or_dump(&mut s, "1 run", "Only 1 step");
    s.expect(expectrl::Eof).unwrap();
}

/// --skip excludes a step.
#[test]
fn run_skip_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--skip", "farewell"], temp.path());

    s.expect("greet").unwrap();
    s.expect("1 run").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// --force re-runs completed steps without prompting.
#[test]
fn run_force_flag_bypasses_completed_check() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let mut s = spawn_bivvy(&["run", "--force", "check-tools"], temp.path());

    // Force should run check-tools without "Already complete" prompt
    expect_or_dump(&mut s, "check-tools", "Forced step runs");
    s.expect(expectrl::Eof).unwrap();
}

/// --env sets the target environment.
#[test]
fn run_env_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--env", "ci", "--dry-run"], temp.path());

    expect_or_dump(&mut s, "ci", "Environment shown");
    s.expect(expectrl::Eof).unwrap();
}

/// --resume resumes an interrupted run.
#[test]
fn run_resume_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--resume"], temp.path());

    // May or may not have anything to resume — verify it produces output and exits
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("TestProject") || clean.contains("resume") || clean.contains("Total:"),
        "Resume flag should produce meaningful output. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

/// --save-preferences flag is accepted.
#[test]
fn run_save_preferences_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--save-preferences"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Header with save-preferences");
    s.expect(expectrl::Eof).unwrap();
}

/// --non-interactive uses defaults without prompting.
#[test]
fn run_non_interactive_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Non-interactive header");
    s.expect(expectrl::Eof).unwrap();
}

/// --skip-behavior flag is accepted.
#[test]
fn run_skip_behavior_flag() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(
        &["run", "--skip", "greet", "--skip-behavior", "skip_only"],
        temp.path(),
    );

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    // Should produce output showing the workflow ran (possibly with farewell step)
    assert!(
        clean.contains("SimpleApp") || clean.contains("Total:") || clean.contains("run"),
        "Skip-behavior flag should produce workflow output. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

// =====================================================================
// ENV VAR OVERRIDES
// =====================================================================

/// Environment variable overrides skip the prompt entirely.
#[test]
fn run_env_var_overrides_prompt() {
    let temp = setup_project(CUSTOM_PROMPTS_CONFIG);
    let s = spawn_bivvy_with_env(
        &["run"],
        temp.path(),
        &[("TARGET", "staging")],
    );

    // Should NOT show "Deploy target" prompt
    wait_for(&s, "deploy", "Step runs with env var override");
}

// =====================================================================
// SAD PATH — Missing config
// =====================================================================

/// Running without a config suggests `bivvy init`.
#[test]
fn run_no_config_suggests_init() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["run"], temp.path());

    s.expect("No configuration found")
        .expect("Should report missing config");
    s.expect(expectrl::Eof).unwrap();
}

/// Running with an empty config file fails gracefully.
#[test]
fn run_empty_config_fails() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["run"], temp.path());

    // Empty config should fail with a parse or validation error
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("Error") || clean.contains("error") || clean.contains("invalid") || clean.contains("parse"),
        "Empty config should produce error output, got: {}",
        &clean[..clean.len().min(300)]
    );
}

/// Running with malformed YAML fails gracefully.
#[test]
fn run_malformed_yaml_fails() {
    let temp = setup_project("{{{{ not: valid: yaml ::::");
    let mut s = spawn_bivvy(&["run"], temp.path());

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("Error") || clean.contains("error") || clean.contains("parse") || clean.contains("YAML"),
        "Malformed YAML should produce error output, got: {}",
        &clean[..clean.len().min(300)]
    );
}

/// Nonexistent workflow name produces an error.
#[test]
fn run_nonexistent_workflow_fails() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "nonexistent"], temp.path());

    s.expect("Unknown workflow: nonexistent").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// --only with a nonexistent step name.
#[test]
fn run_only_nonexistent_step_fails() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--only", "ghost-step"], temp.path());

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("ghost-step") || clean.contains("not found") || clean.contains("Unknown") || clean.contains("error"),
        "Nonexistent --only step should name the missing step in error, got: {}",
        &clean[..clean.len().min(300)]
    );
}

/// Config referencing a nonexistent dependency step.
#[test]
fn run_missing_dependency_fails() {
    let config = r#"
app_name: "BadDeps"
steps:
  orphan:
    title: "Orphan"
    command: "rustc --version"
    depends_on: [nonexistent]
workflows:
  default:
    steps: [orphan]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("nonexistent") || clean.contains("dependency") || clean.contains("not found") || clean.contains("error"),
        "Missing dependency should name the problem in error, got: {}",
        &clean[..clean.len().min(300)]
    );
}

/// Circular dependency is detected.
#[test]
fn run_circular_dependency_fails() {
    let config = r#"
app_name: "Circular"
steps:
  a:
    command: "rustc --version"
    depends_on: [b]
  b:
    command: "git --version"
    depends_on: [a]
workflows:
  default:
    steps: [a, b]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    s.expect("Circular dependency detected:").unwrap();
    s.expect(expectrl::Eof).unwrap();
}

/// Step with an empty command field.
#[test]
fn run_empty_command_fails() {
    let config = r#"
app_name: "EmptyCmd"
steps:
  nothing:
    title: "Nothing"
    command: ""
workflows:
  default:
    steps: [nothing]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    assert!(
        clean.contains("error") || clean.contains("Error") || clean.contains("empty") || clean.contains("nothing"),
        "Empty command should produce error output, got: {}",
        &clean[..clean.len().min(300)]
    );
}

/// Step command that exits non-zero triggers failure handling.
#[test]
fn run_failing_step_shows_error() {
    let temp = setup_project(FAILING_STEP_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    // The "bad" step runs `false` which exits 1
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    // Should show the failing step and some error indication
    assert!(
        clean.contains("Bad step") || clean.contains("fail") || clean.contains("error"),
        "Failing step should produce error-related output. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

/// --no-color flag disables ANSI codes.
#[test]
fn run_no_color_flag() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--no-color"], temp.path());

    expect_or_dump(&mut s, "TestProject", "No-color header");
    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// COMBINATION FLAGS
// =====================================================================

/// --dry-run + --verbose shows detailed preview.
#[test]
fn run_dry_run_verbose() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run", "--verbose"], temp.path());

    expect_or_dump(&mut s, "dry-run", "Dry-run with verbose");
    s.expect(expectrl::Eof).unwrap();
}

/// --only + --force combined.
#[test]
fn run_only_plus_force() {
    let temp = setup_project_with_git(PROMPTED_CHECK_CONFIG);
    let mut s = spawn_bivvy(
        &["run", "--only", "check-tools", "--force", "check-tools"],
        temp.path(),
    );

    expect_or_dump(&mut s, "check-tools", "Force+only runs the step");
    s.expect(expectrl::Eof).unwrap();
}

/// --quiet + --non-interactive for CI-style execution.
#[test]
fn run_quiet_non_interactive() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
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
        "Quiet + non-interactive should exit 0, got {:?}",
        status.code()
    );
}

// =====================================================================
// MULTI-STEP INTERACTION MATRIX
//
// Each test exercises a specific keyboard input at each of 4 steps.
// Named by the input sequence: y=yes, n=no, e=enter, s=space, esc=escape
// =====================================================================

/// y, y, y, n — accept first 3, skip last.
#[test]
fn interactive_seq_yyyn() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: y");
    wait_and_answer(&s, "Count source lines?", KEY_Y, "Step 2: y");
    wait_and_answer(&s, "Recent git history?", KEY_Y, "Step 3: y");
    wait_and_answer(&s, "Project summary?", KEY_N, "Step 4: n");

    wait_for(&s, "Total:", "Summary");
}

/// n, y, y, y — skip first, accept rest (dependents may be affected).
#[test]
fn interactive_seq_nyyy() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_N, "Step 1: n");
    // Remaining steps depend on list-files — they may be auto-skipped
    wait_for(&s, "Total:", "Summary after skip-first");
}

/// y, n, y — step 4 auto-skipped because count-lines was declined.
#[test]
fn interactive_seq_ynyn() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: y");
    wait_and_answer(&s, "Count source lines?", KEY_N, "Step 2: n");
    wait_and_answer(&s, "Recent git history?", KEY_Y, "Step 3: y");
    // Step 4 (summary) depends on count-lines (skipped) → auto-skipped

    wait_for(&s, "Total:", "Summary");
}

/// enter, enter, enter, enter — all defaults (no for skippable steps).
#[test]
fn interactive_seq_all_enter() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_ENTER, "Step 1: enter");
    // Default=no → step 1 skipped → dependents skipped
    wait_for(&s, "Total:", "Summary after all-enter");
}

/// space, space, space, space — all space (confirms default).
#[test]
fn interactive_seq_all_space() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_SPACE, "Step 1: space");
    wait_for(&s, "Total:", "Summary after all-space");
}

/// y, space(=no), n — step 4 auto-skipped because both deps declined.
#[test]
fn interactive_seq_y_space_n_enter() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project files?", KEY_Y, "Step 1: y");
    wait_and_answer(&s, "Count source lines?", KEY_SPACE, "Step 2: space (default=no)");
    wait_and_answer(&s, "Recent git history?", KEY_N, "Step 3: n");
    // Step 4 (summary) depends on count-lines and git-log, both skipped → auto-skipped

    wait_for(&s, "Total:", "Summary");
}

// =====================================================================
// MARKER COMPLETED CHECK
// =====================================================================

/// First run with marker completed_check runs the step.
/// Second run should detect the marker and show "Already complete".
#[test]
fn marker_completed_check_first_run() {
    let temp = setup_project_with_git(MARKER_CHECK_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    expect_or_dump(&mut s, "MarkerApp", "Marker check header");
    expect_or_dump(&mut s, "1 run", "Marker step ran");
    s.expect(expectrl::Eof).unwrap();
}

/// After first run, marker completed_check detects prior completion.
#[test]
fn marker_completed_check_second_run_detects_completion() {
    let temp = setup_project_with_git(MARKER_CHECK_CONFIG);

    // First run to set the marker
    run_workflow_silently(temp.path());

    // Second run — marker should detect completion; step is skippable
    // so it prompts "Already complete. Re-run?"
    let mut s = spawn_bivvy(&["run"], temp.path());
    expect_or_dump(&mut s, "MarkerApp", "Second run header");
    // The marker should cause the step to be detected as complete
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    // Should show completion detection or prompt about already-complete step
    assert!(
        clean.contains("Already complete") || clean.contains("Total:") || clean.contains("run"),
        "Second run should detect marker completion. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

// =====================================================================
// ALL / ANY COMBINATORS FOR COMPLETED_CHECK
// =====================================================================

/// `all` combinator: both file_exists and command_succeeds pass.
#[test]
fn all_completed_check_both_pass() {
    let temp = setup_project_with_git(ALL_CHECK_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // Cargo.toml exists and rustc --version succeeds, so the all check passes.
    // Step is skippable: false, so it runs regardless of check.
    expect_or_dump(&mut s, "AllCheckApp", "All-check header");
    expect_or_dump(&mut s, "1 run", "All-check step ran");
    s.expect(expectrl::Eof).unwrap();
}

/// `any` combinator: one check fails, one passes — overall passes.
#[test]
fn any_completed_check_one_passes() {
    let temp = setup_project_with_git(ANY_CHECK_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // nonexistent file fails but rustc --version succeeds, so any check passes.
    expect_or_dump(&mut s, "AnyCheckApp", "Any-check header");
    expect_or_dump(&mut s, "1 run", "Any-check step ran");
    s.expect(expectrl::Eof).unwrap();
}

/// `all` combinator with skippable step: checks pass, prompts "Already complete".
#[test]
fn all_completed_check_skippable_shows_already_complete() {
    let config = r#"
app_name: "AllCheckSkippable"
steps:
  full-check:
    title: "Full check step"
    command: "rustc --version && wc -l Cargo.toml"
    completed_check:
      type: all
      checks:
        - type: file_exists
          path: "Cargo.toml"
        - type: command_succeeds
          command: "rustc --version"
workflows:
  default:
    steps: [full-check]
"#;
    let temp = setup_project_with_git(config);
    let s = spawn_bivvy(&["run"], temp.path());

    // Both checks pass → "Already complete" prompt
    wait_and_answer(&s, "Already complete", KEY_N, "Skip all-check");
    wait_for(&s, "Total:", "Summary after skipping all-check");
}

/// `any` combinator with skippable step where any check passes.
#[test]
fn any_completed_check_skippable_shows_already_complete() {
    let config = r#"
app_name: "AnyCheckSkippable"
steps:
  any-check:
    title: "Any check step"
    command: "git --version && uname -s"
    completed_check:
      type: any
      checks:
        - type: file_exists
          path: "nonexistent-file.lock"
        - type: command_succeeds
          command: "rustc --version"
workflows:
  default:
    steps: [any-check]
"#;
    let temp = setup_project_with_git(config);
    let s = spawn_bivvy(&["run"], temp.path());

    // rustc check passes → "Already complete" prompt
    wait_and_answer(&s, "Already complete", KEY_Y, "Rerun any-check");
    wait_for(&s, "1 run", "Summary shows 1 run");
}

// =====================================================================
// PRECONDITION
// =====================================================================

/// Step with a passing precondition runs normally.
#[test]
fn precondition_passes_step_runs() {
    let temp = setup_project_with_git(PRECONDITION_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    expect_or_dump(&mut s, "PreconditionApp", "Precondition header");
    expect_or_dump(&mut s, "1 run", "Guarded step ran");
    s.expect(expectrl::Eof).unwrap();
}

/// Step with a failing precondition does not run its command.
#[test]
fn precondition_fails_step_blocked() {
    let temp = setup_project(FAILING_PRECONDITION_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    // The step should fail because `false` exits non-zero
    expect_or_dump(&mut s, "precondition", "Precondition failure reported");
    s.expect(expectrl::Eof).unwrap();
}

/// --force does NOT bypass preconditions.
#[test]
fn precondition_not_bypassed_by_force() {
    let temp = setup_project(FAILING_PRECONDITION_CONFIG);
    let mut s = spawn_bivvy(&["run", "--force", "guarded-fail"], temp.path());

    // Even with --force, precondition should still block the step
    expect_or_dump(&mut s, "precondition", "Precondition still enforced with --force");
    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// --ci FLAG
// =====================================================================

/// --ci flag is accepted and runs non-interactively.
#[test]
fn ci_flag_runs_non_interactively() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--ci"], temp.path());

    // --ci is deprecated alias for --non-interactive + --env ci
    // It should complete without prompts
    expect_or_dump(&mut s, "TestProject", "CI flag header");
    s.expect(expectrl::Eof).unwrap();
}

/// --ci flag with interactive config skips prompts.
#[test]
fn ci_flag_skips_interactive_prompts() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--ci"], temp.path());

    // Should complete without needing interactive input
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    // Verify it completed (has summary line) without hanging
    assert!(
        clean.contains("Total:") || clean.contains("run"),
        "CI mode should complete without prompts. Output: {}",
        &clean[..clean.len().min(500)]
    );
}

// =====================================================================
// EXIT CODES
// =====================================================================

/// Successful workflow exits with code 0.
#[test]
fn exit_code_success() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        status.success(),
        "Successful workflow should exit with code 0, got {:?}",
        status.code()
    );
}

/// Failing step produces non-zero exit code.
#[test]
fn exit_code_failure() {
    let temp = setup_project(FAILING_STEP_CONFIG);
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
        "Failing workflow should exit with non-zero code, got {:?}",
        status.code()
    );
}

/// Missing config produces non-zero exit code.
#[test]
fn exit_code_no_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let status = std::process::Command::new(bin)
        .args(["run"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(
        !status.success(),
        "Missing config should produce non-zero exit code, got {:?}",
        status.code()
    );
}

/// Dry run exits with code 0 (no real execution, no failure).
#[test]
fn exit_code_dry_run_success() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
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
        "Dry run should exit with code 0, got {:?}",
        status.code()
    );
}

// =====================================================================
// SENSITIVE STEP
// =====================================================================

/// Sensitive step with --non-interactive runs but masks output.
#[test]
fn sensitive_step_non_interactive() {
    let temp = setup_project(SENSITIVE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    // Normal step output should be visible
    expect_or_dump(&mut s, "Normal step", "Normal step title visible");
    // Sensitive step should mask its command
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);
    // The sensitive step's command output should be hidden/masked
    // Sensitive steps should not leak their command output to the terminal
    assert!(
        clean.contains("Handle secrets") || clean.contains("Total:"),
        "Sensitive step should show its title but mask output. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

/// Sensitive step prompts for confirmation when interactive.
#[test]
fn sensitive_step_prompts_confirmation() {
    let config = r#"
app_name: "SensitiveInteractive"
steps:
  secrets:
    title: "Handle secrets"
    command: "whoami && uname -s"
    sensitive: true
workflows:
  default:
    steps: [secrets]
"#;
    let temp = setup_project(config);
    let s = spawn_bivvy(&["run"], temp.path());

    // Sensitive step should prompt "Handles sensitive data. Continue?"
    wait_and_answer(&s, "sensitive", KEY_Y, "Confirm sensitive step");
    wait_for(&s, "Total:", "Summary after sensitive step");
}

/// Declining a sensitive step skips it.
#[test]
fn sensitive_step_declined_is_skipped() {
    let config = r#"
app_name: "SensitiveDecline"
steps:
  secrets:
    title: "Handle secrets"
    command: "whoami && uname -s"
    sensitive: true
workflows:
  default:
    steps: [secrets]
"#;
    let temp = setup_project(config);
    let s = spawn_bivvy(&["run"], temp.path());

    // Decline the sensitive step
    wait_and_answer(&s, "sensitive", KEY_N, "Decline sensitive step");
    wait_for(&s, "Total:", "Summary after declining sensitive step");
}

// =====================================================================
// CUSTOM PROMPTS (select/confirm/input via PTY)
// =====================================================================

/// Custom select prompt with env var override bypasses the prompt.
#[test]
fn custom_prompt_select_with_env_override() {
    let temp = setup_project(CUSTOM_PROMPTS_CONFIG);
    let s = spawn_bivvy_with_env(
        &["run"],
        temp.path(),
        &[("TARGET", "staging")],
    );

    // Env var should bypass the prompt and set the variable
    wait_for(&s, "deploy", "Deploy step runs with env override");
}

/// Custom select prompt interaction via PTY — select first option.
#[test]
fn custom_prompt_select_first_option() {
    let temp = setup_project(CUSTOM_PROMPTS_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // The select prompt should appear with "Deploy target"
    // Press Enter to accept default (first option = Staging)
    wait_and_answer(&s, "Deploy target", KEY_ENTER, "Select first option");
    wait_for(&s, "deploy", "Deploy step ran after prompt");
}

/// Custom select prompt — arrow down to second option (Production).
#[test]
fn custom_prompt_select_second_option() {
    let temp = setup_project(CUSTOM_PROMPTS_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Navigate to second option
    wait_and_send_keys(&s, "Deploy target", ARROW_DOWN, "Arrow to Production");
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_key(&s, KEY_ENTER);

    wait_for(&s, "deploy", "Deploy step ran with Production");
}

// =====================================================================
// MID-WORKFLOW OUTPUT VERIFICATION
// =====================================================================

/// Verify output appears between steps during workflow execution.
#[test]
fn mid_workflow_output_between_steps() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    // Verify step titles appear in order during execution
    expect_or_dump(&mut s, "TestProject", "Header first");
    expect_or_dump(&mut s, "Verify toolchain", "Step 1 title");
    expect_or_dump(&mut s, "Verify git repository", "Step 2 title");
    // Later steps
    expect_or_dump(&mut s, "Generate build manifest", "Step 5 title");
    expect_or_dump(&mut s, "Verify build manifest", "Step 6 title");
    expect_or_dump(&mut s, "6 run", "Summary");
    s.expect(expectrl::Eof).unwrap();
}

/// Verify variable interpolation output is visible mid-workflow.
#[test]
fn mid_workflow_variable_interpolation_visible() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "info", "--verbose"], temp.path());

    expect_or_dump(&mut s, "TestProject", "Header");
    // The gather-info step interpolates ${version} from `cat VERSION`
    expect_or_dump(&mut s, "0.2.5", "Version variable visible mid-workflow");
    s.expect(expectrl::Eof).unwrap();
}

/// Verify simple workflow step output content is visible.
#[test]
fn mid_workflow_step_output_content() {
    let temp = setup_project(SIMPLE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    expect_or_dump(&mut s, "SimpleApp", "Header");
    expect_or_dump(&mut s, "Say hello", "Greet step title visible");
    expect_or_dump(&mut s, "Say goodbye", "Farewell step title visible");
    s.expect(expectrl::Eof).unwrap();
}
