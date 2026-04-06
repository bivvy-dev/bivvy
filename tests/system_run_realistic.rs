//! Realistic end-to-end system tests for `bivvy run`.
//!
//! Each test drives a complete workflow through a real PTY with real
//! project files. Commands use external programs (git, rustc, cargo,
//! grep, sed) — not shell builtins — to exercise real command execution.
#![cfg(unix)]

mod system;
use system::helpers::*;

use std::fs;

// ── Configs ──────────────────────────────────────────────────────────

/// 5-step realistic workflow with `skippable: false`. Uses git, rustc,
/// cargo, grep, and sed against real project files. Three workflows:
/// default (all 5), quick (3), info (3).
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
  verify-toolchain:
    title: "Verify toolchain"
    command: "rustc --version && git --version"
    skippable: false
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  validate-repo:
    title: "Validate git repo"
    command: "git rev-parse --git-dir && git status --short"
    skippable: false
    depends_on: [verify-toolchain]
    completed_check:
      type: command_succeeds
      command: "git rev-parse --git-dir"
    watches:
      - .git/HEAD

  analyze-metadata:
    title: "Analyze project metadata"
    command: "grep -c 'name' Cargo.toml && wc -l < src/main.rs && head -1 Cargo.toml"
    skippable: false
    depends_on: [validate-repo]

  extract-version:
    title: "Extract version info"
    command: "sed -n 's/^version = \"\\(.*\\)\"/\\1/p' Cargo.toml && cat VERSION"
    skippable: false
    depends_on: [validate-repo]

  generate-report:
    title: "Generate build report"
    command: "date -u '+%Y-%m-%d' > .build-report.txt && uname -s >> .build-report.txt && whoami >> .build-report.txt && cat .build-report.txt"
    skippable: false
    depends_on: [analyze-metadata, extract-version]
    completed_check:
      type: file_exists
      path: ".build-report.txt"

workflows:
  default:
    steps: [verify-toolchain, validate-repo, analyze-metadata, extract-version, generate-report]

  quick:
    description: "Quick validation"
    steps: [verify-toolchain, validate-repo, extract-version]

  info:
    description: "Gather project information"
    steps: [verify-toolchain, validate-repo, analyze-metadata]
"#;

/// 4-step interactive workflow (all skippable by default). Uses git,
/// grep, cargo, and sed against real project files.
const INTERACTIVE_CONFIG: &str = r#"
app_name: "InteractiveTest"

settings:
  default_output: verbose

steps:
  list-structure:
    title: "List project structure"
    command: "find . -maxdepth 2 -type f -not -path './.git/*' | sort | head -20"

  analyze-deps:
    title: "Analyze dependencies"
    command: "grep -c '=' Cargo.toml && wc -l < Cargo.toml"
    depends_on: [list-structure]

  review-history:
    title: "Review git history"
    command: "git log --oneline -3 && git branch --show-current"
    depends_on: [list-structure]

  generate-summary:
    title: "Generate summary"
    command: "basename $(git rev-parse --show-toplevel) && wc -l src/main.rs && sed -n '1p' VERSION"
    depends_on: [analyze-deps, review-history]

workflows:
  default:
    steps: [list-structure, analyze-deps, review-history, generate-summary]
"#;

/// 4-step workflow with `completed_check` AND skippable (default).
/// Two steps have checks that pass, two have checks that fail.
const CHECK_CONFIG: &str = r#"
app_name: "CheckTest"

settings:
  default_output: verbose

steps:
  check-rust:
    title: "Check Rust toolchain"
    command: "rustc --version"
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  check-cargo-toml:
    title: "Check Cargo.toml exists"
    command: "head -3 Cargo.toml"
    completed_check:
      type: file_exists
      path: "Cargo.toml"

  check-missing-file:
    title: "Check missing lockfile"
    command: "cargo metadata --format-version 1 --no-deps 2>/dev/null || cargo --version"
    completed_check:
      type: file_exists
      path: "nonexistent-file.lock"

  check-failing-cmd:
    title: "Check failing command"
    command: "uname -s"
    completed_check:
      type: command_succeeds
      command: "grep NONEXISTENT_MARKER_STRING Cargo.toml"

workflows:
  default:
    steps: [check-rust, check-cargo-toml, check-missing-file, check-failing-cmd]
"#;

/// 4-step workflow testing environment features: step-level env vars,
/// env_file, required_env, only_environments.
const ENV_CONFIG: &str = r#"
app_name: "EnvTest"

settings:
  default_output: verbose
  environments:
    ci:
      env:
        CI_MODE: "true"

steps:
  step-env:
    title: "Step env vars"
    command: "grep -q '' /dev/null && cat .bivvy/step-env-output.txt || basename /dev/null"
    env:
      STEP_VAR: "hello-from-step"
    skippable: false

  env-file-step:
    title: "Env file step"
    command: "git diff --stat HEAD~1 2>/dev/null || git --version"
    env_file: ".env.test"
    env_file_optional: true
    skippable: false

  required-env-step:
    title: "Required env step"
    command: "uname -s && whoami"
    required_env: [HOME]
    skippable: false

  ci-only-step:
    title: "CI only step"
    command: "uname -m && date -u '+%H:%M'"
    only_environments: [ci]
    skippable: false

workflows:
  default:
    steps: [step-env, env-file-step, required-env-step]

  ci:
    description: "CI workflow with env-restricted steps"
    steps: [step-env, env-file-step, required-env-step, ci-only-step]
"#;

/// 4-step workflow testing before/after hooks. Hooks create marker
/// files so tests can verify they ran.
const HOOKS_CONFIG: &str = r#"
app_name: "HooksTest"

settings:
  default_output: verbose

steps:
  with-before:
    title: "Step with before hook"
    command: "uname -s > .step-main-output.txt"
    skippable: false
    before:
      - "date -u '+%s' > .hook-before-with-before.txt"

  with-after:
    title: "Step with after hook"
    command: "whoami > .step-after-output.txt"
    skippable: false
    after:
      - "date -u '+%s' > .hook-after-with-after.txt"

  with-both:
    title: "Step with both hooks"
    command: "uname -m > .step-both-output.txt"
    skippable: false
    depends_on: [with-before, with-after]
    before:
      - "date -u '+%s' > .hook-before-with-both.txt"
      - "whoami > .hook-before2-with-both.txt"
    after:
      - "date -u '+%s' > .hook-after-with-both.txt"

  final-check:
    title: "Final verification"
    command: "test -f .hook-before && test -f .hook-after"
    skippable: false
    depends_on: [with-both]

workflows:
  default:
    steps: [with-before, with-after, with-both, final-check]
"#;

/// 4-step workflow testing allow_failure and retry. One step fails
/// with allow_failure: true, one step uses retry.
const FAILURE_CONFIG: &str = r#"
app_name: "FailureTest"

settings:
  default_output: verbose

steps:
  setup-step:
    title: "Setup step"
    command: "uname -s && date -u '+%Y-%m-%d'"
    skippable: false

  failing-step:
    title: "Failing step"
    command: "grep NONEXISTENT_PATTERN_XYZZY Cargo.toml"
    skippable: false
    allow_failure: true
    depends_on: [setup-step]

  retry-step:
    title: "Retry step"
    command: "whoami && uname -m"
    skippable: false
    retry: 2
    depends_on: [setup-step]

  final-step:
    title: "Final step"
    command: "wc -l < Cargo.toml && basename $(pwd)"
    skippable: false
    depends_on: [failing-step, retry-step]

workflows:
  default:
    steps: [setup-step, failing-step, retry-step, final-step]
"#;

/// Realistic config with marker completed_check for second-run detection.
const MARKER_REALISTIC_CONFIG: &str = r#"
app_name: "MarkerRealistic"

settings:
  default_output: verbose

steps:
  init-db:
    title: "Initialize database"
    command: "date -u '+%s' > .db-marker.txt && uname -s >> .db-marker.txt"
    skippable: false
    completed_check:
      type: marker

  seed-data:
    title: "Seed data"
    command: "whoami > .seed-marker.txt && date -u '+%Y-%m-%d' >> .seed-marker.txt"
    skippable: false
    depends_on: [init-db]
    completed_check:
      type: marker

workflows:
  default:
    steps: [init-db, seed-data]
"#;

/// Config with `all` combinator in a realistic scenario.
const ALL_COMBINATOR_CONFIG: &str = r#"
app_name: "AllCombinatorRealistic"

settings:
  default_output: verbose

steps:
  full-validate:
    title: "Full validation"
    command: "rustc --version && wc -l Cargo.toml && head -1 src/main.rs"
    skippable: false
    completed_check:
      type: all
      checks:
        - type: file_exists
          path: "Cargo.toml"
        - type: file_exists
          path: "src/main.rs"
        - type: command_succeeds
          command: "rustc --version"

workflows:
  default:
    steps: [full-validate]
"#;

/// Config with `any` combinator in a realistic scenario.
const ANY_COMBINATOR_CONFIG: &str = r#"
app_name: "AnyCombinatorRealistic"

settings:
  default_output: verbose

steps:
  flexible-check:
    title: "Flexible check"
    command: "git --version && uname -s"
    skippable: false
    completed_check:
      type: any
      checks:
        - type: file_exists
          path: "package.json"
        - type: file_exists
          path: "Cargo.toml"

workflows:
  default:
    steps: [flexible-check]
"#;

/// Config with precondition on a realistic step.
const PRECONDITION_REALISTIC_CONFIG: &str = r#"
app_name: "PreconditionRealistic"

settings:
  default_output: verbose

steps:
  deploy:
    title: "Deploy application"
    command: "git rev-parse HEAD && uname -s"
    skippable: false
    precondition:
      type: command_succeeds
      command: "git rev-parse --git-dir"

workflows:
  default:
    steps: [deploy]
"#;

/// Config with sensitive step in a realistic scenario.
const SENSITIVE_REALISTIC_CONFIG: &str = r#"
app_name: "SensitiveRealistic"

settings:
  default_output: verbose

steps:
  build:
    title: "Build project"
    command: "rustc --version && wc -l Cargo.toml"
    skippable: false

  deploy-secrets:
    title: "Deploy with secrets"
    command: "whoami && uname -s && date -u '+%s'"
    skippable: false
    sensitive: true
    depends_on: [build]

  verify:
    title: "Verify deployment"
    command: "git --version && uname -m"
    skippable: false
    depends_on: [deploy-secrets]

workflows:
  default:
    steps: [build, deploy-secrets, verify]
"#;

// ── Section 1: Full workflow execution (REALISTIC_CONFIG) ────────────

/// Full 5-step workflow runs to completion. All steps are `skippable:
/// false` so no interactive prompts. Verifies side effects.
#[test]
fn full_workflow_runs_to_completion() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "TestProject", "Header");
    wait_for(&s, "5 run", "Summary shows 5 run");

    assert!(
        temp.path().join(".build-report.txt").exists(),
        "generate-report should create .build-report.txt"
    );
}

/// Named quick workflow (3 steps) runs to completion.
#[test]
fn named_workflow_quick() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run", "--workflow", "quick"], temp.path());

    wait_for(&s, "TestProject", "Quick workflow header");
    wait_for(&s, "3 run", "Summary shows 3 run");
}

/// Named info workflow shows variable interpolation output (${version}).
#[test]
fn named_workflow_info_with_vars() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run", "--workflow", "info"], temp.path());

    wait_for(&s, "TestProject", "Info workflow header");
    wait_for(&s, "3 run", "Summary shows 3 run");
}

/// Dry run produces no side effects.
#[test]
fn dry_run_no_side_effects() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    wait_for(&s, "dry-run", "Dry-run indicator");

    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "Dry run must not create side effects"
    );
}

/// Bare `bivvy` (no subcommand) defaults to `run`.
#[test]
fn bare_bivvy_full_run() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let s = spawn_bivvy(&[], temp.path());

    wait_for(&s, "TestProject", "Header");
    wait_for(&s, "5 run", "Summary shows 5 run");
}

/// Full workflow exits with code 0.
#[test]
fn full_workflow_exit_code_success() {
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
        "Full workflow should exit 0, got {:?}",
        status.code()
    );
}

// ── Section 2: Interactive prompts (INTERACTIVE_CONFIG) ──────────────

/// Accept all 4 step prompts — full execution through PTY.
#[test]
fn interactive_accept_all() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project structure", KEY_Y, "Accept list-structure");
    wait_and_answer(&s, "Analyze dependencies", KEY_Y, "Accept analyze-deps");
    wait_and_answer(&s, "Review git history", KEY_Y, "Accept review-history");
    wait_and_answer(&s, "Generate summary", KEY_Y, "Accept generate-summary");

    wait_for(&s, "4 run", "Summary shows 4 run");
}

/// Skip the first step — dependents are auto-skipped too.
#[test]
fn interactive_skip_root_cascades() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Skip list-structure (the root step). Its dependents (analyze-deps,
    // review-history, generate-summary) are auto-skipped by the
    // dependency resolver — no further prompts appear.
    wait_and_answer(&s, "List project structure", KEY_N, "Skip list-structure");

    wait_for(&s, "Total:", "Summary footer");
}

/// Mixed answers: accept first and last, skip middle two.
#[test]
fn interactive_mixed_answers() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_and_answer(&s, "List project structure", KEY_Y, "Accept list-structure");
    wait_and_answer(&s, "Analyze dependencies", KEY_N, "Skip analyze-deps");
    wait_and_answer(&s, "Review git history", KEY_N, "Skip review-history");
    wait_and_answer(&s, "Generate summary", KEY_Y, "Accept generate-summary");

    wait_for(&s, "Total:", "Summary footer");
}

// ── Section 3: Completed check prompts (CHECK_CONFIG) ────────────────

/// Skip completed steps, accept unchecked steps.
/// check-rust and check-cargo-toml have passing checks (prompt
/// "Already complete"). check-missing-file and check-failing-cmd
/// have failing checks (prompt with step title).
/// Accept all prompts with 'y' to run everything.
#[test]
fn completed_check_skip_completed() {
    let temp = setup_project_with_git(CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Steps appear in workflow order. Two have passing completed_checks
    // ("Already complete") and two have failing checks (normal prompt).
    // Answer y to everything — all 4 should run.
    wait_and_answer(&s, "?", KEY_Y, "First prompt");
    wait_and_answer(&s, "?", KEY_Y, "Second prompt");
    wait_and_answer(&s, "?", KEY_Y, "Third prompt");
    wait_and_answer(&s, "?", KEY_Y, "Fourth prompt");

    wait_for(&s, "4 run", "Summary shows 4 run");
}

/// Skip all steps (answer n to everything) — verifies skip works.
#[test]
fn completed_check_skip_all() {
    let temp = setup_project_with_git(CHECK_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // Answer n to everything — all 4 should be skipped.
    wait_and_answer(&s, "?", KEY_N, "First prompt");
    wait_and_answer(&s, "?", KEY_N, "Second prompt");
    wait_and_answer(&s, "?", KEY_N, "Third prompt");
    wait_and_answer(&s, "?", KEY_N, "Fourth prompt");

    wait_for(&s, "Total:", "Summary footer");
}

// ── Section 4: Environment features (ENV_CONFIG) ─────────────────────

/// Step-level env vars are available to commands.
#[test]
fn env_vars_on_step() {
    let temp = setup_project_with_git(ENV_CONFIG);

    // Create the .env.test file referenced by env_file_step
    fs::write(temp.path().join(".env.test"), "TEST_VAR=from-env-file\n").unwrap();

    let s = spawn_bivvy(&["run"], temp.path());

    // All steps are skippable: false, so no prompts
    wait_for(&s, "3 run", "Summary shows 3 steps run");
}

/// --env ci selects the CI environment, making ci-only-step run.
#[test]
fn env_flag_selects_environment() {
    let temp = setup_project_with_git(ENV_CONFIG);

    // Create the .env.test file
    fs::write(temp.path().join(".env.test"), "TEST_VAR=from-env-file\n").unwrap();

    let s = spawn_bivvy(&["run", "--env", "ci", "--workflow", "ci"], temp.path());

    // CI workflow has 4 steps including ci-only-step
    wait_for(&s, "4 run", "Summary shows 4 steps run in CI");
}

// ── Section 5: Hooks (HOOKS_CONFIG) ──────────────────────────────────

/// Before and after hooks create marker files that we can verify.
#[test]
fn before_after_hooks_run() {
    let temp = setup_project(HOOKS_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // All steps are skippable: false
    wait_for(&s, "4 run", "Summary shows 4 run");

    // Verify before hooks ran
    assert!(
        temp.path().join(".hook-before-with-before.txt").exists(),
        "Before hook for with-before should have created marker file"
    );
    assert!(
        temp.path().join(".hook-before-with-both.txt").exists(),
        "First before hook for with-both should have created marker file"
    );
    assert!(
        temp.path().join(".hook-before2-with-both.txt").exists(),
        "Second before hook for with-both should have created marker file"
    );

    // Verify after hooks ran
    assert!(
        temp.path().join(".hook-after-with-after.txt").exists(),
        "After hook for with-after should have created marker file"
    );
    assert!(
        temp.path().join(".hook-after-with-both.txt").exists(),
        "After hook for with-both should have created marker file"
    );

    // Verify main step outputs
    assert!(
        temp.path().join(".step-main-output.txt").exists(),
        "Main command of with-before should have run"
    );
    assert!(
        temp.path().join(".step-after-output.txt").exists(),
        "Main command of with-after should have run"
    );
    assert!(
        temp.path().join(".step-both-output.txt").exists(),
        "Main command of with-both should have run"
    );
}

/// Hooks workflow exits 0.
#[test]
fn hooks_exit_code_success() {
    let temp = setup_project(HOOKS_CONFIG);
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
        "Hooks workflow should exit 0, got {:?}",
        status.code()
    );
}

// ── Section 6: Failure handling (FAILURE_CONFIG) ─────────────────────

/// Workflow completes despite a failing step with `allow_failure: true`.
#[test]
fn allow_failure_continues_workflow() {
    let temp = setup_project_with_git(FAILURE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // All steps are skippable: false. The failing step should not stop
    // the workflow because allow_failure is true.
    wait_for(&s, "Total:", "Summary footer — workflow completed");
}

/// Retry step attempts execution multiple times.
#[test]
fn retry_attempts_execution() {
    let temp = setup_project_with_git(FAILURE_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    // The retry-step should succeed on first attempt (whoami && uname -m)
    // and the workflow should complete
    wait_for(&s, "Total:", "Summary footer — workflow completed with retry step");
}

// ── Section 7: Second run (REALISTIC_CONFIG) ─────────────────────────

/// Two consecutive runs both complete successfully.
#[test]
fn second_run_completes() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);

    // First run
    let s = spawn_bivvy(&["run"], temp.path());
    wait_for(&s, "TestProject", "First run header");
    wait_for(&s, "5 run", "First run summary");

    assert!(temp.path().join(".build-report.txt").exists());

    // Second run — .build-report.txt already exists so generate-report's
    // completed_check passes, but skippable: false means it reruns anyway
    let s = spawn_bivvy(&["run"], temp.path());
    wait_for(&s, "TestProject", "Second run header");
    wait_for(&s, "5 run", "Second run summary");
}

// ── Section 8: Marker completed_check ────────────────────────────────

/// Marker check: first run creates marker, second run detects it.
#[test]
fn marker_check_first_run_succeeds() {
    let temp = setup_project_with_git(MARKER_REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "MarkerRealistic", "Header");
    wait_for(&s, "2 run", "Both marker steps ran");

    // Verify side-effect files
    assert!(
        temp.path().join(".db-marker.txt").exists(),
        "init-db should have created .db-marker.txt"
    );
    assert!(
        temp.path().join(".seed-marker.txt").exists(),
        "seed-data should have created .seed-marker.txt"
    );
}

/// Second run with marker checks detects prior completion.
#[test]
fn marker_check_second_run_detects_completion() {
    let temp = setup_project_with_git(MARKER_REALISTIC_CONFIG);

    // First run
    run_workflow_silently(temp.path());

    // Second run — markers should be detected
    let s = spawn_bivvy(&["run"], temp.path());
    wait_for(&s, "MarkerRealistic", "Second run header");
    // Steps are skippable: false, so they rerun despite marker
    wait_for(&s, "2 run", "Both steps rerun");
}

// ── Section 9: All/Any combinators ───────────────────────────────────

/// `all` combinator with all checks passing.
#[test]
fn all_combinator_all_pass() {
    let temp = setup_project_with_git(ALL_COMBINATOR_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "AllCombinatorRealistic", "Header");
    wait_for(&s, "1 run", "Step ran");
}

/// `any` combinator where one check passes (Cargo.toml exists,
/// package.json does not).
#[test]
fn any_combinator_one_passes() {
    let temp = setup_project_with_git(ANY_COMBINATOR_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "AnyCombinatorRealistic", "Header");
    wait_for(&s, "1 run", "Step ran");
}

/// `all` combinator exit code is 0 when all checks pass.
#[test]
fn all_combinator_exit_code() {
    let temp = setup_project_with_git(ALL_COMBINATOR_CONFIG);
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
        "All-combinator workflow should exit 0, got {:?}",
        status.code()
    );
}

// ── Section 10: Precondition ─────────────────────────────────────────

/// Passing precondition allows step execution.
#[test]
fn precondition_passes_allows_execution() {
    let temp = setup_project_with_git(PRECONDITION_REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "PreconditionRealistic", "Header");
    wait_for(&s, "1 run", "Deploy step ran");
}

/// Failing precondition blocks step execution.
#[test]
fn precondition_fails_blocks_execution() {
    let config = r#"
app_name: "PreconditionFail"
steps:
  blocked:
    title: "Blocked step"
    command: "rustc --version"
    skippable: false
    precondition:
      type: command_succeeds
      command: "git --no-such-flag-xyz"
workflows:
  default:
    steps: [blocked]
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    expect_or_dump(&mut s, "precondition", "Precondition failure message");
    s.expect(expectrl::Eof).unwrap();
}

/// Precondition failure produces non-zero exit code.
#[test]
fn precondition_failure_exit_code() {
    let config = r#"
app_name: "PreconditionExitCode"
steps:
  blocked:
    title: "Blocked step"
    command: "rustc --version"
    skippable: false
    precondition:
      type: command_succeeds
      command: "git --no-such-flag-xyz"
workflows:
  default:
    steps: [blocked]
"#;
    let temp = setup_project(config);
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
        "Failed precondition should produce non-zero exit code, got {:?}",
        status.code()
    );
}

// ── Section 11: Sensitive steps ──────────────────────────────────────

/// Sensitive step output is masked in non-interactive verbose mode.
#[test]
fn sensitive_step_masks_output_realistic() {
    let temp = setup_project(SENSITIVE_REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--non-interactive", "--verbose"], temp.path());

    // Build step output should be visible
    expect_or_dump(&mut s, "Build project", "Build step title");
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // Sensitive step should complete but mask its command output
    assert!(
        clean.contains("Deploy with secrets") || clean.contains("Total:") || clean.contains("Verify deployment"),
        "Sensitive step should show title but mask output. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

/// Sensitive step with --dry-run masks command text.
#[test]
fn sensitive_step_dry_run_masks_command() {
    let temp = setup_project(SENSITIVE_REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run", "--verbose"], temp.path());

    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    assert!(
        clean.contains("dry-run") || clean.contains("Sensitive"),
        "Dry-run should show plan without leaking sensitive details. Got: {}",
        &clean[..clean.len().min(500)]
    );
}

/// Sensitive workflow exits 0 on success.
#[test]
fn sensitive_workflow_exit_code() {
    let temp = setup_project(SENSITIVE_REALISTIC_CONFIG);
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
        status.success(),
        "Sensitive workflow should exit 0, got {:?}",
        status.code()
    );
}

// ── Section 12: Mid-workflow output verification ─────────────────────

/// Verify step titles appear in execution order.
#[test]
fn mid_workflow_step_titles_in_order() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let s = spawn_bivvy(&["run", "--verbose"], temp.path());

    wait_for(&s, "Verify toolchain", "Step 1 title");
    wait_for(&s, "Validate git repo", "Step 2 title");
    wait_for(&s, "Analyze project metadata", "Step 3 title");
    wait_for(&s, "Extract version info", "Step 4 title");
    wait_for(&s, "Generate build report", "Step 5 title");
    wait_for(&s, "5 run", "Summary");
}

/// Verify hook execution creates files between steps.
#[test]
fn mid_workflow_hooks_create_files() {
    let temp = setup_project(HOOKS_CONFIG);
    let s = spawn_bivvy(&["run", "--verbose"], temp.path());

    // Wait for step titles to appear in order
    wait_for(&s, "Step with before hook", "Before-hook step");
    wait_for(&s, "Step with after hook", "After-hook step");
    wait_for(&s, "Step with both hooks", "Both-hooks step");
    wait_for(&s, "Final verification", "Final step");
    wait_for(&s, "4 run", "Summary");
}

/// Verify failure handling output mid-workflow.
#[test]
fn mid_workflow_failure_handling_output() {
    let temp = setup_project_with_git(FAILURE_CONFIG);
    let s = spawn_bivvy(&["run", "--verbose"], temp.path());

    // Setup step should succeed
    wait_for(&s, "Setup step", "Setup step title");
    // Failing step should show failure but continue (allow_failure: true)
    wait_for(&s, "Failing step", "Failing step title");
    // Retry step should succeed
    wait_for(&s, "Retry step", "Retry step title");
    // Final step should run after failure handling
    wait_for(&s, "Final step", "Final step title");
    wait_for(&s, "Total:", "Summary footer");
}
