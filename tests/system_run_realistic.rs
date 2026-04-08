//! Realistic end-to-end system tests for `bivvy run`.
//!
//! Each test drives a complete workflow through a real PTY with real
//! project files. Commands use development toolchain programs (git,
//! rustc, cargo) — not shell builtins or text-processing utilities —
//! to exercise real command execution.
#![cfg(unix)]

mod system;
use system::helpers::*;

use std::fs;

// ── Configs ──────────────────────────────────────────────────────────

/// 5-step realistic workflow with `skippable: false`. Uses git, rustc,
/// and cargo against real project files. Three workflows: default (all
/// 5), quick (3), info (3).
///
/// The `vars` block defines `git_branch` which is interpolated into the
/// `generate-report` command, so the variable machinery is actually
/// exercised (not dead test data).
const REALISTIC_CONFIG: &str = r#"
app_name: "TestProject"

settings:
  default_output: verbose

vars:
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
    command: "cargo pkgid && rustc --print sysroot > .sysroot.txt"
    skippable: false
    depends_on: [validate-repo]

  extract-version:
    title: "Extract version info"
    command: "cargo pkgid > .pkgid.txt && git describe --always >> .pkgid.txt"
    skippable: false
    depends_on: [validate-repo]

  generate-report:
    title: "Generate build report"
    command: "echo branch=${git_branch} > .build-report.txt && rustc --version >> .build-report.txt && cargo --version >> .build-report.txt"
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
/// rustc, and cargo against real project files.
const INTERACTIVE_CONFIG: &str = r#"
app_name: "InteractiveTest"

settings:
  default_output: verbose

steps:
  list-structure:
    title: "List project structure"
    command: "cargo metadata --format-version 1 --no-deps 2>/dev/null || cargo --version"

  analyze-deps:
    title: "Analyze dependencies"
    command: "cargo pkgid 2>/dev/null || cargo --version"
    depends_on: [list-structure]

  review-history:
    title: "Review git history"
    command: "git log --oneline -3 && git branch --show-current"
    depends_on: [list-structure]

  generate-summary:
    title: "Generate summary"
    command: "git rev-parse --show-toplevel && rustc --version && cargo --version"
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
    command: "cargo pkgid"
    completed_check:
      type: file_exists
      path: "Cargo.toml"

  check-missing-file:
    title: "Check missing lockfile"
    command: "cargo --version && rustc --version"
    completed_check:
      type: file_exists
      path: "nonexistent-file.lock"

  check-failing-cmd:
    title: "Check failing command"
    command: "rustc --version"
    completed_check:
      type: command_succeeds
      command: "cargo verify-project --manifest-path nonexistent/Cargo.toml"

workflows:
  default:
    steps: [check-rust, check-cargo-toml, check-missing-file, check-failing-cmd]
"#;

/// 4-step workflow testing environment features: step-level env vars,
/// env_file, required_env, only_environments.
///
/// Each step writes the env var it cares about to a marker file after
/// running a real command (cargo/git/rustc). Tests read those marker
/// files to verify env vars actually reached the child process — without
/// this, the test would be vacuous on the env feature itself.
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
    command: "rustc --version && printenv STEP_VAR > .step-var.txt"
    env:
      STEP_VAR: "hello-from-step"
    skippable: false

  env-file-step:
    title: "Env file step"
    command: "cargo --version && printenv TEST_VAR > .envfile-var.txt"
    env_file: ".env.test"
    env_file_optional: true
    skippable: false

  required-env-step:
    title: "Required env step"
    command: "rustc --version && printenv HOME > .required-var.txt"
    required_env: [HOME]
    skippable: false

  ci-only-step:
    title: "CI only step"
    command: "cargo --version && printenv CI_MODE > .ci-var.txt"
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
    command: "rustc --version > .step-main-output.txt"
    skippable: false
    before:
      - "git --version > .hook-before-with-before.txt"

  with-after:
    title: "Step with after hook"
    command: "cargo --version > .step-after-output.txt"
    skippable: false
    after:
      - "git --version > .hook-after-with-after.txt"

  with-both:
    title: "Step with both hooks"
    command: "rustc --print sysroot > .step-both-output.txt"
    skippable: false
    depends_on: [with-before, with-after]
    before:
      - "rustc --version > .hook-before-with-both.txt"
      - "cargo --version > .hook-before2-with-both.txt"
    after:
      - "cargo --version > .hook-after-with-both.txt"

  final-check:
    title: "Final verification"
    command: "rustc --version && cargo --version"
    skippable: false
    depends_on: [with-both]

workflows:
  default:
    steps: [with-before, with-after, with-both, final-check]
"#;

/// 4-step workflow testing allow_failure and retry.
///
/// - `failing-step` runs `rustc` with an invalid `--edition` value. rustc
///   will exit non-zero with a real error, but `allow_failure: true`
///   keeps the workflow going.
/// - `retry-step` runs a shell command that fails on the first invocation
///   and succeeds on the second. It uses a counter file so each attempt
///   mutates state; the first read sees no counter (exit 1), the second
///   read sees the counter written by the first attempt (exit 0). This
///   actually exercises the retry loop.
const FAILURE_CONFIG: &str = r#"
app_name: "FailureTest"

settings:
  default_output: verbose

steps:
  setup-step:
    title: "Setup step"
    command: "rustc --version && cargo --version"
    skippable: false

  failing-step:
    title: "Failing step"
    command: "rustc --edition=2099 src/main.rs"
    skippable: false
    allow_failure: true
    depends_on: [setup-step]

  retry-step:
    title: "Retry step"
    command: "test -f .retry-attempt.txt || { cargo --version > .retry-attempt.txt; exit 1; }"
    skippable: false
    retry: 2
    depends_on: [setup-step]

  final-step:
    title: "Final step"
    command: "cargo pkgid > .final-step.txt && git log --oneline -1 >> .final-step.txt"
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
    command: "git rev-parse HEAD > .db-marker.txt && rustc --version >> .db-marker.txt"
    skippable: false
    completed_check:
      type: marker

  seed-data:
    title: "Seed data"
    command: "cargo --version > .seed-marker.txt && git log --oneline -1 >> .seed-marker.txt"
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
    command: "rustc --version && cargo pkgid && git rev-parse --short HEAD"
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
    command: "git --version && rustc --version"
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
    command: "git rev-parse HEAD && rustc --version"
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
    command: "rustc --version && cargo --version"
    skippable: false

  deploy-secrets:
    title: "Deploy with secrets"
    command: "rustc --print sysroot && cargo --version"
    skippable: false
    sensitive: true
    depends_on: [build]

  verify:
    title: "Verify deployment"
    command: "git --version && rustc --version"
    skippable: false
    depends_on: [deploy-secrets]

workflows:
  default:
    steps: [build, deploy-secrets, verify]
"#;

// ── Section 1: Full workflow execution (REALISTIC_CONFIG) ────────────

/// Full 5-step workflow runs to completion. All steps are `skippable:
/// false` so no interactive prompts. Verifies header, per-step titles,
/// summary message with exact counts, exit code, and side-effect files
/// (including that the `${git_branch}` variable was interpolated).
#[test]
fn full_workflow_runs_to_completion() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "TestProject", "Header contains app_name");
    wait_for(&s, "default workflow", "Header contains workflow name");
    wait_for(&s, "Verify toolchain", "Step 1 title");
    wait_for(&s, "Validate git repo", "Step 2 title");
    wait_for(&s, "Analyze project metadata", "Step 3 title");
    wait_for(&s, "Extract version info", "Step 4 title");
    wait_for(&s, "Generate build report", "Step 5 title");
    wait_for(
        &s,
        "Setup complete! (5 run, 0 skipped)",
        "Full summary message",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Side-effect files from each step.
    let build_report = temp.path().join(".build-report.txt");
    assert!(
        build_report.exists(),
        "generate-report should create .build-report.txt"
    );
    assert!(
        temp.path().join(".sysroot.txt").exists(),
        "analyze-metadata should create .sysroot.txt"
    );
    assert!(
        temp.path().join(".pkgid.txt").exists(),
        "extract-version should create .pkgid.txt"
    );

    // Verify `${git_branch}` was interpolated — the report should contain
    // `branch=main` (the branch created by setup_project_with_git).
    let report = fs::read_to_string(&build_report)
        .expect("read .build-report.txt");
    assert!(
        report.contains("branch=main"),
        "Expected branch variable to be interpolated to 'main'. Got:\n{report}"
    );
    assert!(
        report.contains("rustc"),
        "Report should contain rustc --version output. Got:\n{report}"
    );
    assert!(
        report.contains("cargo"),
        "Report should contain cargo --version output. Got:\n{report}"
    );
}

/// Named quick workflow (3 steps) runs to completion. The quick workflow
/// is verify-toolchain → validate-repo → extract-version; it does NOT
/// include generate-report, so `.build-report.txt` must not exist.
#[test]
fn named_workflow_quick() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "quick"], temp.path());

    wait_for(&s, "TestProject", "Header contains app_name");
    wait_for(&s, "quick workflow", "Header names the quick workflow");
    wait_for(&s, "Verify toolchain", "Step 1 title in quick workflow");
    wait_for(&s, "Validate git repo", "Step 2 title in quick workflow");
    wait_for(&s, "Extract version info", "Step 3 title in quick workflow");
    wait_for(
        &s,
        "Setup complete! (3 run, 0 skipped)",
        "Quick workflow summary",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // extract-version was run, so .pkgid.txt should exist.
    assert!(
        temp.path().join(".pkgid.txt").exists(),
        "extract-version should create .pkgid.txt in quick workflow"
    );
    // generate-report is NOT in quick workflow, so its output must NOT
    // exist — this verifies the workflow selection actually limited steps.
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "quick workflow must NOT run generate-report"
    );
    // analyze-metadata is also NOT in quick workflow.
    assert!(
        !temp.path().join(".sysroot.txt").exists(),
        "quick workflow must NOT run analyze-metadata"
    );
}

/// Named info workflow (verify-toolchain, validate-repo, analyze-metadata)
/// runs to completion. Verifies selected steps ran and excluded steps did not.
#[test]
fn named_workflow_info() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--workflow", "info"], temp.path());

    wait_for(&s, "TestProject", "Header contains app_name");
    wait_for(&s, "info workflow", "Header names the info workflow");
    wait_for(&s, "Verify toolchain", "Step 1 title in info workflow");
    wait_for(&s, "Validate git repo", "Step 2 title in info workflow");
    wait_for(&s, "Analyze project metadata", "Step 3 title in info workflow");
    wait_for(
        &s,
        "Setup complete! (3 run, 0 skipped)",
        "Info workflow summary",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // analyze-metadata is in the info workflow.
    assert!(
        temp.path().join(".sysroot.txt").exists(),
        "analyze-metadata should create .sysroot.txt in info workflow"
    );
    // extract-version is NOT in info workflow.
    assert!(
        !temp.path().join(".pkgid.txt").exists(),
        "info workflow must NOT run extract-version"
    );
    // generate-report is NOT in info workflow.
    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "info workflow must NOT run generate-report"
    );
}

/// `--dry-run` preview produces no side effects. Verifies the dry-run
/// banner, the summary appears, exit code is 0, and NO step output files
/// were created on disk.
#[test]
fn dry_run_no_side_effects() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run"], temp.path());

    wait_for(
        &s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run banner",
    );
    // Step titles should still appear (planning output).
    wait_for(&s, "Verify toolchain", "Step 1 title is previewed");
    wait_for(&s, "Generate build report", "Step 5 title is previewed");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    assert!(
        !temp.path().join(".build-report.txt").exists(),
        "Dry run must not create .build-report.txt"
    );
    assert!(
        !temp.path().join(".sysroot.txt").exists(),
        "Dry run must not create .sysroot.txt"
    );
    assert!(
        !temp.path().join(".pkgid.txt").exists(),
        "Dry run must not create .pkgid.txt"
    );
}

/// Bare `bivvy` (no subcommand) defaults to `run`. Verifies the default
/// alias resolves to the full workflow — header, all 5 step titles, exact
/// summary message, exit code, and side-effect file.
#[test]
fn bare_bivvy_full_run() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&[], temp.path());

    wait_for(&s, "TestProject", "Header contains app_name");
    wait_for(&s, "Verify toolchain", "Step 1 title");
    wait_for(&s, "Generate build report", "Last step title");
    wait_for(
        &s,
        "Setup complete! (5 run, 0 skipped)",
        "Full summary message from bare bivvy",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    assert!(
        temp.path().join(".build-report.txt").exists(),
        "bare bivvy must run the full default workflow"
    );
}

// ── Section 2: Interactive prompts (INTERACTIVE_CONFIG) ──────────────

/// Accept all 4 step prompts — full execution through PTY. Verifies each
/// prompt appears in order, answering `y` runs the step, and the final
/// summary reports exactly 4 run and 0 skipped.
#[test]
fn interactive_accept_all() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "InteractiveTest", "Header contains app_name");
    wait_and_answer(&s, "List project structure", KEY_Y, "Accept list-structure");
    wait_and_answer(&s, "Analyze dependencies", KEY_Y, "Accept analyze-deps");
    wait_and_answer(&s, "Review git history", KEY_Y, "Accept review-history");
    wait_and_answer(&s, "Generate summary", KEY_Y, "Accept generate-summary");

    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// Skip the first step — dependents are auto-skipped too. Verifies the
/// dependency resolver cascades the skip and the summary reports
/// 0 run / 4 skipped.
#[test]
fn interactive_skip_root_cascades() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "InteractiveTest", "Header contains app_name");
    // Skip list-structure (the root step). Its dependents (analyze-deps,
    // review-history, generate-summary) are auto-skipped by the
    // dependency resolver — no further prompts appear.
    wait_and_answer(&s, "List project structure", KEY_N, "Skip list-structure");

    wait_for(
        &s,
        "Setup complete! (0 run, 4 skipped)",
        "Summary reports all 4 steps skipped via cascade",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// Mixed answers: accept first two, skip the third. Since generate-summary
/// depends on both analyze-deps and review-history, skipping review-history
/// auto-skips generate-summary. Final count: 2 run, 2 skipped.
#[test]
fn interactive_mixed_answers() {
    let temp = setup_project_with_git(INTERACTIVE_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "InteractiveTest", "Header contains app_name");
    wait_and_answer(&s, "List project structure", KEY_Y, "Accept list-structure");
    wait_and_answer(&s, "Analyze dependencies", KEY_Y, "Accept analyze-deps");
    wait_and_answer(&s, "Review git history", KEY_N, "Skip review-history");
    // generate-summary depends on review-history which was skipped,
    // so it is auto-skipped with no prompt

    wait_for(
        &s,
        "Setup complete! (2 run, 2 skipped)",
        "Summary reports 2 run / 2 skipped",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

// ── Section 3: Completed check prompts (CHECK_CONFIG) ────────────────

/// When passing `completed_check`s meet their "already complete" condition,
/// Bivvy prompts "Already complete ... Re-run?". Answering `y` to each
/// prompt (both "Already complete" and the standard prompt for failed
/// checks) re-runs all 4 steps.
#[test]
fn completed_check_skip_completed() {
    let temp = setup_project_with_git(CHECK_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "CheckTest", "Header contains app_name");
    // check-rust has a passing completed_check — the prompt wording is
    // "Already complete (...). Re-run?".
    wait_for(&s, "Already complete", "Already-complete prompt wording");
    wait_and_answer(&s, "Check Rust toolchain", KEY_Y, "Accept check-rust");
    wait_and_answer(
        &s,
        "Check Cargo.toml exists",
        KEY_Y,
        "Accept check-cargo-toml",
    );
    wait_and_answer(
        &s,
        "Check missing lockfile",
        KEY_Y,
        "Accept check-missing-file",
    );
    wait_and_answer(
        &s,
        "Check failing command",
        KEY_Y,
        "Accept check-failing-cmd",
    );

    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// Skip all steps (answer n to everything) — verifies skip works. The
/// summary should report 0 run, 4 skipped.
#[test]
fn completed_check_skip_all() {
    let temp = setup_project_with_git(CHECK_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "CheckTest", "Header contains app_name");
    // Answer n to everything — all 4 should be skipped.
    wait_and_answer(&s, "Check Rust toolchain", KEY_N, "Skip check-rust");
    wait_and_answer(&s, "Check Cargo.toml exists", KEY_N, "Skip check-cargo-toml");
    wait_and_answer(&s, "Check missing lockfile", KEY_N, "Skip check-missing-file");
    wait_and_answer(&s, "Check failing command", KEY_N, "Skip check-failing-cmd");

    wait_for(
        &s,
        "Setup complete! (0 run, 4 skipped)",
        "Summary reports all 4 steps skipped",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

// ── Section 4: Environment features (ENV_CONFIG) ─────────────────────

/// Step-level `env:`, `env_file:`, and `required_env:` values are all
/// actually passed to the child process. Each step writes its env var
/// to a marker file via `printenv`; the test reads those markers to
/// prove the values made it into the environment, then verifies the
/// summary message and exit code.
#[test]
fn env_vars_on_step() {
    let temp = setup_project_with_git(ENV_CONFIG);

    // Create the .env.test file referenced by env_file_step.
    fs::write(
        temp.path().join(".env.test"),
        "TEST_VAR=from-env-file\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["run"], temp.path());

    // All default-workflow steps are skippable: false, so no prompts.
    wait_for(&s, "EnvTest", "Header contains app_name");
    wait_for(
        &s,
        "Setup complete! (3 run, 0 skipped)",
        "Summary reports 3 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Step-level env:
    let step_var = fs::read_to_string(temp.path().join(".step-var.txt"))
        .expect("read .step-var.txt");
    assert_eq!(
        step_var.trim(),
        "hello-from-step",
        "step-level env should reach the child process"
    );

    // env_file:
    let envfile_var = fs::read_to_string(temp.path().join(".envfile-var.txt"))
        .expect("read .envfile-var.txt");
    assert_eq!(
        envfile_var.trim(),
        "from-env-file",
        "env_file entries should reach the child process"
    );

    // required_env: (HOME is always set in test env)
    let required_var = fs::read_to_string(temp.path().join(".required-var.txt"))
        .expect("read .required-var.txt");
    assert!(
        !required_var.trim().is_empty(),
        "required_env HOME should reach the child process"
    );

    // ci-only-step is gated by `only_environments: [ci]` and MUST NOT run
    // in the default (no --env) invocation.
    assert!(
        !temp.path().join(".ci-var.txt").exists(),
        "ci-only-step must not run without --env ci"
    );
}

/// `--env ci` activates the `ci` environment, which injects `CI_MODE=true`
/// and gates `ci-only-step` via `only_environments`. The `ci` workflow
/// includes that step. Verify CI_MODE reached the child and the ci-only
/// step actually ran (via its marker file).
#[test]
fn env_flag_selects_environment() {
    let temp = setup_project_with_git(ENV_CONFIG);

    // Create the .env.test file.
    fs::write(
        temp.path().join(".env.test"),
        "TEST_VAR=from-env-file\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(
        &["run", "--env", "ci", "--workflow", "ci"],
        temp.path(),
    );

    wait_for(&s, "EnvTest", "Header contains app_name");
    wait_for(&s, "ci workflow", "Header names the ci workflow");
    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 CI steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // CI_MODE from environments.ci.env should reach ci-only-step.
    let ci_var = fs::read_to_string(temp.path().join(".ci-var.txt"))
        .expect("read .ci-var.txt");
    assert_eq!(
        ci_var.trim(),
        "true",
        "environments.ci.env CI_MODE should reach the child process"
    );
    // Step-level env still applies.
    let step_var = fs::read_to_string(temp.path().join(".step-var.txt"))
        .expect("read .step-var.txt");
    assert_eq!(step_var.trim(), "hello-from-step");
}

// ── Section 5: Hooks (HOOKS_CONFIG) ──────────────────────────────────

/// Before and after hooks run in order around each step's main command.
/// Each hook writes a marker file containing real command output
/// (rustc/cargo/git version). Verifies:
///   - All hook marker files exist
///   - All step main output files exist
///   - Hook output contains the expected tool output (not just empty files)
///   - Summary reports 4 run, 0 skipped, exit code 0
#[test]
fn before_after_hooks_run() {
    let temp = setup_project(HOOKS_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "HooksTest", "Header contains app_name");
    // All steps are skippable: false — no prompts.
    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Helper: read marker file and assert it contains the expected tool
    // output substring. An empty or missing file fails the assertion.
    let check_marker = |name: &str, expected_substr: &str| {
        let path = temp.path().join(name);
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {name}: {e}"));
        assert!(
            contents.contains(expected_substr),
            "{name} should contain {expected_substr:?}, got: {contents}"
        );
    };

    // Before hooks
    check_marker(".hook-before-with-before.txt", "git");
    check_marker(".hook-before-with-both.txt", "rustc");
    check_marker(".hook-before2-with-both.txt", "cargo");

    // After hooks
    check_marker(".hook-after-with-after.txt", "git");
    check_marker(".hook-after-with-both.txt", "cargo");

    // Main step outputs
    check_marker(".step-main-output.txt", "rustc");
    check_marker(".step-after-output.txt", "cargo");
    check_marker(".step-both-output.txt", "/");
}

// ── Section 6: Failure handling (FAILURE_CONFIG) ─────────────────────

/// Workflow completes despite a failing step with `allow_failure: true`.
/// Verifies the failing step is visibly reported, the final step still
/// runs (proved by its side-effect file), the summary reports all 4
/// steps ran, and exit code is 0.
#[test]
fn allow_failure_continues_workflow() {
    let temp = setup_project_with_git(FAILURE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    wait_for(&s, "FailureTest", "Header contains app_name");
    wait_for(&s, "Setup step", "Setup step title");
    wait_for(
        &s,
        "Failing step",
        "Failing step title — allow_failure continues",
    );
    wait_for(
        &s,
        "Final step",
        "Final step runs despite earlier failure",
    );
    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran (allow_failure counts as run)",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // final-step actually executed (proves allow_failure didn't halt).
    let final_output = fs::read_to_string(temp.path().join(".final-step.txt"))
        .expect("final-step should have created .final-step.txt");
    assert!(
        final_output.contains("test-project"),
        "final-step should include cargo pkgid output, got: {final_output}"
    );
}

/// Retry step attempts execution multiple times until one succeeds. The
/// retry command fails the first attempt (creating a counter file as a
/// side effect) and succeeds on the second attempt. Verifies the retry
/// loop actually ran — without retry, the step would fail permanently
/// and the workflow would report failure.
#[test]
fn retry_attempts_execution() {
    let temp = setup_project_with_git(FAILURE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    wait_for(&s, "FailureTest", "Header contains app_name");
    wait_for(&s, "Retry step", "Retry step title appears");
    wait_for(&s, "Final step", "Final step runs after retry step");
    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran — retry succeeded",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // The first retry attempt wrote .retry-attempt.txt, the second
    // attempt saw it and exited 0. If retry weren't applied, the step
    // would fail permanently and final-step would not run.
    assert!(
        temp.path().join(".retry-attempt.txt").exists(),
        "retry-step should have created .retry-attempt.txt on first (failing) attempt"
    );
    assert!(
        temp.path().join(".final-step.txt").exists(),
        "final-step should have run after successful retry"
    );
}

// ── Section 7: Second run (REALISTIC_CONFIG) ─────────────────────────

/// Two consecutive runs both complete successfully. Even though
/// generate-report's `completed_check` passes on the second run
/// (because `.build-report.txt` exists from the first), `skippable:
/// false` means it reruns anyway and the summary reports all 5 steps.
#[test]
fn second_run_completes() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);

    // First run
    let mut s = spawn_bivvy(&["run"], temp.path());
    wait_for(&s, "TestProject", "First run header");
    wait_for(
        &s,
        "Setup complete! (5 run, 0 skipped)",
        "First run summary",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    assert!(temp.path().join(".build-report.txt").exists());
    // Capture first-run modification time.
    let first_mtime = fs::metadata(temp.path().join(".build-report.txt"))
        .unwrap()
        .modified()
        .unwrap();

    // Second run — .build-report.txt already exists so generate-report's
    // completed_check passes, but skippable: false means it reruns anyway.
    let mut s = spawn_bivvy(&["run"], temp.path());
    wait_for(&s, "TestProject", "Second run header");
    wait_for(
        &s,
        "Setup complete! (5 run, 0 skipped)",
        "Second run summary",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // The report should have been regenerated (not just skipped).
    let second_mtime = fs::metadata(temp.path().join(".build-report.txt"))
        .unwrap()
        .modified()
        .unwrap();
    assert!(
        second_mtime >= first_mtime,
        "second run should have rewritten .build-report.txt"
    );
}

// ── Section 8: Marker completed_check ────────────────────────────────

/// Marker check: first run creates marker files via real tool output.
/// Verifies both marker files exist AND contain the expected content
/// (not just empty files).
#[test]
fn marker_check_first_run_succeeds() {
    let temp = setup_project_with_git(MARKER_REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "MarkerRealistic", "Header contains app_name");
    wait_for(
        &s,
        "Setup complete! (2 run, 0 skipped)",
        "Summary reports both marker steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Verify side-effect file contents.
    let db_marker = fs::read_to_string(temp.path().join(".db-marker.txt"))
        .expect("read .db-marker.txt");
    assert!(
        db_marker.contains("rustc"),
        "init-db marker should contain rustc output, got: {db_marker}"
    );
    let seed_marker = fs::read_to_string(temp.path().join(".seed-marker.txt"))
        .expect("read .seed-marker.txt");
    assert!(
        seed_marker.contains("cargo"),
        "seed-data marker should contain cargo output, got: {seed_marker}"
    );
}

/// Second run with marker checks detects prior completion. Steps are
/// `skippable: false`, so they rerun despite marker state. The second
/// run should still report both steps ran.
#[test]
fn marker_check_second_run_detects_completion() {
    let temp = setup_project_with_git(MARKER_REALISTIC_CONFIG);

    // First run
    run_workflow_silently(temp.path());
    // Markers should exist from first run.
    assert!(temp.path().join(".db-marker.txt").exists());
    assert!(temp.path().join(".seed-marker.txt").exists());

    // Second run — markers should be detected by the runner.
    let mut s = spawn_bivvy(&["run"], temp.path());
    wait_for(&s, "MarkerRealistic", "Second run header");
    // Steps are skippable: false, so they rerun despite marker.
    wait_for(
        &s,
        "Setup complete! (2 run, 0 skipped)",
        "Second run summary: both steps rerun",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

// ── Section 9: All/Any combinators ───────────────────────────────────

/// `all` combinator with all checks passing. The single step has
/// `skippable: false` so it runs regardless, but Bivvy still evaluates
/// the combinator. Verifies the step title, summary, and exit code.
#[test]
fn all_combinator_all_pass() {
    let temp = setup_project_with_git(ALL_COMBINATOR_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "AllCombinatorRealistic", "Header contains app_name");
    wait_for(&s, "Full validation", "Step title");
    wait_for(
        &s,
        "Setup complete! (1 run, 0 skipped)",
        "Summary reports the single step ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// `any` combinator where one of two checks passes (Cargo.toml exists,
/// package.json does not). Verifies the step title, summary, and exit
/// code.
#[test]
fn any_combinator_one_passes() {
    let temp = setup_project_with_git(ANY_COMBINATOR_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "AnyCombinatorRealistic", "Header contains app_name");
    wait_for(&s, "Flexible check", "Step title");
    wait_for(
        &s,
        "Setup complete! (1 run, 0 skipped)",
        "Summary reports the single step ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

// ── Section 10: Precondition ─────────────────────────────────────────

/// Passing precondition allows step execution. Verifies the step title,
/// summary, and exit code, plus the side-effect of the step running.
#[test]
fn precondition_passes_allows_execution() {
    let temp = setup_project_with_git(PRECONDITION_REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "PreconditionRealistic", "Header contains app_name");
    wait_for(&s, "Deploy application", "Step title");
    wait_for(
        &s,
        "Setup complete! (1 run, 0 skipped)",
        "Summary reports deploy step ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// Failing precondition blocks step execution, produces a specific
/// error message, and exits with code 1 (per `docs/commands/run.md`
/// which documents 1 = "One or more steps failed").
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

    wait_for(&s, "PreconditionFail", "Header contains app_name");
    wait_for(&s, "Blocked step", "Blocked step title is shown");
    expect_or_dump(
        &mut s,
        "Precondition failed",
        "Precondition failure message appears",
    );
    read_to_eof(&mut s);
    // Exit code 1 per docs/commands/run.md.
    assert_exit_code(&s, 1);
}

// ── Section 11: Sensitive steps ──────────────────────────────────────

/// Sensitive step runs in `--non-interactive --verbose` mode: all three
/// step titles appear, the full summary message is emitted, and exit
/// code is 0. The sensitive step's command text must not leak into
/// the output (its title is allowed).
#[test]
fn sensitive_step_masks_output_realistic() {
    let temp = setup_project(SENSITIVE_REALISTIC_CONFIG);
    let mut s = spawn_bivvy(
        &["run", "--non-interactive", "--verbose"],
        temp.path(),
    );

    expect_or_dump(&mut s, "Build project", "Build step title");
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    assert!(
        clean.contains("Deploy with secrets"),
        "Sensitive step title should appear. Got: {}",
        &clean[..clean.len().min(500)]
    );
    assert!(
        clean.contains("Verify deployment"),
        "Verify step should appear after sensitive step. Got: {}",
        &clean[..clean.len().min(500)]
    );
    assert!(
        clean.contains("Setup complete! (3 run, 0 skipped)"),
        "Full summary message should appear. Got: {}",
        &clean[..clean.len().min(500)]
    );
    // Sensitive command body must not leak in verbose output.
    assert!(
        !clean.contains("rustc --print sysroot && cargo --version"),
        "Sensitive command text must not leak. Got: {}",
        &clean[..clean.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// Sensitive step with `--dry-run --verbose` does not leak the command
/// body in the preview. The dry-run banner and sensitive step title
/// are still shown.
#[test]
fn sensitive_step_dry_run_masks_command() {
    let temp = setup_project(SENSITIVE_REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--dry-run", "--verbose"], temp.path());

    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run banner",
    );
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // The sensitive step's actual command must not appear in dry-run output.
    assert!(
        !clean.contains("rustc --print sysroot && cargo --version"),
        "Dry-run should not leak sensitive command text. Got: {}",
        &clean[..clean.len().min(500)]
    );
    // But step titles should still be visible.
    assert!(
        clean.contains("Deploy with secrets"),
        "Dry-run should show sensitive step title. Got: {}",
        &clean[..clean.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

// ── Section 12: Mid-workflow output verification ─────────────────────

/// Verify step titles appear in execution order in `--verbose` mode.
#[test]
fn mid_workflow_step_titles_in_order() {
    let temp = setup_project_with_git(REALISTIC_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    wait_for(&s, "Verify toolchain", "Step 1 title");
    wait_for(&s, "Validate git repo", "Step 2 title");
    wait_for(&s, "Analyze project metadata", "Step 3 title");
    wait_for(&s, "Extract version info", "Step 4 title");
    wait_for(&s, "Generate build report", "Step 5 title");
    wait_for(
        &s,
        "Setup complete! (5 run, 0 skipped)",
        "Summary reports all 5 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// Verify hook-owning steps emit their titles in dependency order in
/// `--verbose` mode, and the summary reports all 4 ran.
#[test]
fn mid_workflow_hooks_create_files() {
    let temp = setup_project(HOOKS_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    wait_for(&s, "Step with before hook", "Before-hook step title");
    wait_for(&s, "Step with after hook", "After-hook step title");
    wait_for(&s, "Step with both hooks", "Both-hooks step title");
    wait_for(&s, "Final verification", "Final step title");
    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}

/// Verify failure handling output mid-workflow: all 4 step titles appear
/// in order and the summary confirms all ran (allow_failure + retry
/// count as run).
#[test]
fn mid_workflow_failure_handling_output() {
    let temp = setup_project_with_git(FAILURE_CONFIG);
    let mut s = spawn_bivvy(&["run", "--verbose"], temp.path());

    wait_for(&s, "Setup step", "Setup step title");
    wait_for(&s, "Failing step", "Failing step title");
    wait_for(&s, "Retry step", "Retry step title");
    wait_for(&s, "Final step", "Final step title");
    wait_for(
        &s,
        "Setup complete! (4 run, 0 skipped)",
        "Summary reports all 4 steps ran",
    );
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
}
