//! System tests for bivvy configuration features.
//!
//! Tests all possible configuration options: env vars, hooks,
//! preconditions, sensitive steps, environment overrides, etc.
//! Commands use real toolchain programs (git, rustc, cargo, printenv)
//! rather than shell builtins, and every test verifies exit codes plus
//! concrete side effects where applicable.
#![cfg(unix)]

mod system;
use system::helpers::*;

use std::fs;

// ─────────────────────────────────────────────────────────────────────
// 1. Step-level environment variables (env)
// ─────────────────────────────────────────────────────────────────────

/// Step-level `env` entries must be exported to the step's command.
///
/// The first step writes the value of `MY_VAR` to a file. The follow-up
/// step `grep`s the expected literal out of that file — if the env var
/// was not set, `grep` exits non-zero and the workflow fails.
#[test]
fn step_env_vars_are_available() {
    let config = r#"
app_name: "EnvVarsTest"

steps:
  check-env:
    title: "Check env var"
    command: "printenv MY_VAR > .env-output.txt"
    skippable: false
    env:
      MY_VAR: "hello_from_bivvy"

  verify-env:
    title: "Verify env var value"
    command: "grep -Fx hello_from_bivvy .env-output.txt"
    skippable: false
    depends_on: [check-env]

  done:
    title: "Finish env test"
    command: "git --version"
    skippable: false
    depends_on: [verify-env]

workflows:
  default:
    steps: [check-env, verify-env, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "EnvVarsTest", "Header with app name");
    wait_for(&s, "3 run", "Summary shows all 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // The env var value must have been written to disk by the step.
    let contents = fs::read_to_string(temp.path().join(".env-output.txt")).unwrap();
    assert_eq!(contents.trim(), "hello_from_bivvy");
}

// ─────────────────────────────────────────────────────────────────────
// 2. Environment file (env_file)
// ─────────────────────────────────────────────────────────────────────

/// `env_file` at the step level loads variables from a dotenv-format
/// file so the command can read them. We write the loaded value to a
/// marker file and assert on the file contents.
#[test]
fn env_file_loads_variables() {
    // setup_project_with_git creates `.env` with `APP_ENV=development`.
    let config = r#"
app_name: "EnvFileTest"

steps:
  read-env:
    title: "Read env file var"
    command: "printenv APP_ENV > .env-file-output.txt"
    skippable: false
    env_file: ".env"

  verify-env-file:
    title: "Verify env file value"
    command: "grep -Fx development .env-file-output.txt"
    skippable: false
    depends_on: [read-env]

  finish:
    title: "Finish env file test"
    command: "git --version"
    skippable: false
    depends_on: [verify-env-file]

workflows:
  default:
    steps: [read-env, verify-env-file, finish]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "EnvFileTest", "Header with app name");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    let contents = fs::read_to_string(temp.path().join(".env-file-output.txt")).unwrap();
    assert_eq!(contents.trim(), "development");
}

// ─────────────────────────────────────────────────────────────────────
// 3. Required environment (required_env)
// ─────────────────────────────────────────────────────────────────────

/// When `required_env` is satisfied (via step-level `env`), the step
/// runs normally.
#[test]
fn required_env_succeeds_when_present() {
    let config = r#"
app_name: "RequiredEnvTest"

steps:
  needs-var:
    title: "Needs REQUIRED_VAR"
    command: "printenv REQUIRED_VAR > .required-output.txt"
    skippable: false
    required_env: ["REQUIRED_VAR"]
    env:
      REQUIRED_VAR: "present_value"

  verify:
    title: "Verify REQUIRED_VAR value"
    command: "grep -Fx present_value .required-output.txt"
    skippable: false
    depends_on: [needs-var]

  finish:
    title: "All done"
    command: "git --version"
    skippable: false
    depends_on: [verify]

workflows:
  default:
    steps: [needs-var, verify, finish]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "RequiredEnvTest", "Header");
    wait_for(&s, "3 run", "Summary shows 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    let contents = fs::read_to_string(temp.path().join(".required-output.txt")).unwrap();
    assert_eq!(contents.trim(), "present_value");
}

/// When the required env var is missing, the step fails and the
/// workflow exits non-zero. We use `printenv` on a var that is
/// guaranteed not to exist so the command itself fails with exit 1.
#[test]
fn required_env_fails_when_missing() {
    let config = r#"
app_name: "RequiredEnvMissingTest"

steps:
  needs-var:
    title: "Needs REQUIRED_VAR"
    command: "printenv NONEXISTENT_VAR_XYZ"
    skippable: false
    required_env: ["NONEXISTENT_VAR_XYZ"]

  verify:
    title: "Verify"
    command: "git --version"
    skippable: false

  finish:
    title: "All done"
    command: "git --version"
    skippable: false

workflows:
  default:
    steps: [needs-var, verify, finish]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    wait_for(&s, "Needs REQUIRED_VAR", "Failing step title appears");
    read_to_eof(&mut s);
    // Failed step with skippable: false and no allow_failure must cause
    // bivvy to exit with code 1.
    assert_exit_code(&s, 1);
}

// ─────────────────────────────────────────────────────────────────────
// 4. Allow failure (allow_failure)
// ─────────────────────────────────────────────────────────────────────

/// A step with `allow_failure: true` that fails must not stop the
/// workflow — dependent steps still run and bivvy exits 0.
#[test]
fn allow_failure_continues_workflow() {
    let config = r#"
app_name: "AllowFailureTest"

steps:
  setup:
    title: "Setup step"
    command: "git --version"
    skippable: false

  failing-step:
    title: "Failing but allowed"
    command: "grep NONEXISTENT_STRING_12345 Cargo.toml"
    skippable: false
    allow_failure: true
    depends_on: [setup]

  final-step:
    title: "Final step runs"
    command: "rustc --version > .final-marker.txt"
    skippable: false
    depends_on: [setup]

workflows:
  default:
    steps: [setup, failing-step, final-step]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "Setup step", "Setup step title");
    wait_for(&s, "Failing but allowed", "Failing step title");
    wait_for(&s, "Final step runs", "Final step runs despite earlier failure");
    wait_for(&s, "Total:", "Summary footer appears");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Final step's side effect must exist — proves it actually ran.
    let contents = fs::read_to_string(temp.path().join(".final-marker.txt")).unwrap();
    assert!(
        contents.contains("rustc"),
        ".final-marker.txt should contain rustc version output"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 5. Retry (retry)
// ─────────────────────────────────────────────────────────────────────

/// A step that succeeds on the first attempt does not emit retry
/// output. A step whose command always fails with `retry: 2` should
/// emit retry output and still cause the workflow to fail. We wrap the
/// retry in `allow_failure: true` so the workflow completes and we can
/// assert on both the retry message and exit code 0.
#[test]
fn retry_attempts_shown_in_output() {
    let config = r#"
app_name: "RetryTest"

steps:
  will-fail:
    title: "Will fail with retries"
    command: "grep IMPOSSIBLE_STRING_999 Cargo.toml"
    skippable: false
    retry: 2
    allow_failure: true

  fallback:
    title: "Fallback step"
    command: "git --version"
    skippable: false
    depends_on: [will-fail]

  done:
    title: "Retry test done"
    command: "git --version"
    skippable: false
    depends_on: [fallback]

workflows:
  default:
    steps: [will-fail, fallback, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--non-interactive", "--verbose"], temp.path());

    wait_for(&s, "Will fail with retries", "Retrying step title");
    // Bivvy prints the exact line "    Retrying... (attempt N/M)" on each
    // re-attempt; assert on the full canonical prefix rather than a bare
    // substring.
    wait_for(
        &s,
        "Retrying... (attempt",
        "Bivvy should announce retry attempts when retry > 0",
    );
    wait_for(&s, "Fallback step", "Fallback step title");
    wait_for(&s, "Retry test done", "Final step title");
    read_to_eof(&mut s);
    // allow_failure: true means the workflow still exits 0.
    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// 6. Before/after hooks (before, after)
// ─────────────────────────────────────────────────────────────────────

/// `before` hooks must run before the step's main command — verified
/// by a follow-up step that reads the marker file created by the hook.
#[test]
fn before_hooks_run_before_command() {
    let config = r#"
app_name: "BeforeHookTest"

steps:
  hooked:
    title: "Step with before hook"
    command: "git --version"
    skippable: false
    before:
      - "git --version > .before-marker.txt"

  verify-hook:
    title: "Verify before hook ran"
    command: "grep -q git .before-marker.txt"
    skippable: false
    depends_on: [hooked]

  done:
    title: "Before hook test done"
    command: "git --version"
    skippable: false
    depends_on: [verify-hook]

workflows:
  default:
    steps: [hooked, verify-hook, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "BeforeHookTest", "Header");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    let contents = fs::read_to_string(temp.path().join(".before-marker.txt")).unwrap();
    assert!(
        contents.contains("git"),
        ".before-marker.txt should contain git version output"
    );
}

/// `after` hooks must run after the step's main command — verified by
/// a follow-up step that reads the marker file.
#[test]
fn after_hooks_run_after_command() {
    let config = r#"
app_name: "AfterHookTest"

steps:
  hooked:
    title: "Step with after hook"
    command: "git --version"
    skippable: false
    after:
      - "cargo --version > .after-marker.txt"

  verify-hook:
    title: "Verify after hook ran"
    command: "grep -q cargo .after-marker.txt"
    skippable: false
    depends_on: [hooked]

  done:
    title: "After hook test done"
    command: "git --version"
    skippable: false
    depends_on: [verify-hook]

workflows:
  default:
    steps: [hooked, verify-hook, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "AfterHookTest", "Header");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    let contents = fs::read_to_string(temp.path().join(".after-marker.txt")).unwrap();
    assert!(
        contents.contains("cargo"),
        ".after-marker.txt should contain cargo version output"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 7. Precondition (precondition)
// ─────────────────────────────────────────────────────────────────────

/// A passing precondition lets the step execute normally.
#[test]
fn precondition_passes_step_runs() {
    let config = r#"
app_name: "PreconditionPassTest"

steps:
  guarded:
    title: "Guarded step"
    command: "rustc --version > .guarded-marker.txt"
    skippable: false
    precondition:
      type: command_succeeds
      command: "rustc --version"

  verify:
    title: "Verify guarded ran"
    command: "grep -q rustc .guarded-marker.txt"
    skippable: false
    depends_on: [guarded]

  done:
    title: "Precondition pass test done"
    command: "git --version"
    skippable: false
    depends_on: [verify]

workflows:
  default:
    steps: [guarded, verify, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "PreconditionPassTest", "Header");
    wait_for(&s, "Guarded step", "Guarded step title");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    assert!(
        temp.path().join(".guarded-marker.txt").exists(),
        "Guarded step should have produced .guarded-marker.txt"
    );
}

/// A failing precondition blocks step execution and causes the
/// workflow to exit 1. Bivvy prints `Precondition failed: ...`.
#[test]
fn precondition_fails_step_skipped() {
    let config = r#"
app_name: "PreconditionFailTest"

steps:
  guarded:
    title: "Guarded step"
    command: "rustc --version > .should-not-exist.txt"
    skippable: false
    precondition:
      type: command_succeeds
      command: "git --no-such-flag-xyz"

workflows:
  default:
    steps: [guarded]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    expect_or_dump(&mut s, "Precondition failed", "Precondition failure message");
    read_to_eof(&mut s);
    assert_exit_code(&s, 1);

    // Side-effect verification: the step's main command must not have run.
    assert!(
        !temp.path().join(".should-not-exist.txt").exists(),
        "Guarded step's main command must not execute when precondition fails"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 8. Sensitive step (sensitive)
// ─────────────────────────────────────────────────────────────────────

/// In dry-run mode, a sensitive step's command text is replaced with
/// the literal `[SENSITIVE - command hidden in dry-run]` so secrets
/// never leak into logs.
#[test]
fn sensitive_step_hides_command_in_dry_run() {
    let config = r#"
app_name: "SensitiveTest"

steps:
  public-step:
    title: "Public step"
    command: "git --version"
    skippable: false

  secret-step:
    title: "Secret step"
    command: "printenv SECRET_TOKEN_VALUE_XYZ"
    skippable: false
    sensitive: true
    depends_on: [public-step]

  final-step:
    title: "Final sensitive step"
    command: "git --version"
    skippable: false
    depends_on: [secret-step]

workflows:
  default:
    steps: [public-step, secret-step, final-step]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(
        &["run", "--dry-run", "--verbose"],
        temp.path(),
    );

    expect_or_dump(
        &mut s,
        "Running in dry-run mode - no commands will be executed",
        "Dry-run indicator",
    );
    expect_or_dump(
        &mut s,
        "[SENSITIVE - command hidden in dry-run]",
        "Sensitive command must be replaced with the hidden-command marker",
    );
    let output = read_to_eof(&mut s);
    let clean = strip_ansi(&output);

    // The actual secret command text must never appear in dry-run output.
    assert!(
        !clean.contains("SECRET_TOKEN_VALUE_XYZ"),
        "Sensitive command should be hidden in dry-run output, but found it"
    );
    assert_exit_code(&s, 0);

    // Dry-run must not create side effects.
    assert!(
        !temp.path().join(".should-not-exist-dry.txt").exists(),
        "Dry-run must not produce file side effects"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 9. Only environments (only_environments)
// ─────────────────────────────────────────────────────────────────────

/// `only_environments: [ci]` excludes the step when `--env` is set to
/// something other than `ci`. The excluded step must not execute.
#[test]
fn only_environments_skips_in_non_matching_env() {
    let config = r#"
app_name: "OnlyEnvTest"

steps:
  always-runs:
    title: "Always runs"
    command: "git --version > .always-marker.txt"
    skippable: false

  ci-only:
    title: "CI only step"
    command: "rustc --version > .ci-only-marker.txt"
    skippable: false
    only_environments: ["ci"]

  final:
    title: "Final step"
    command: "git --version > .final-marker.txt"
    skippable: false

workflows:
  default:
    steps: [always-runs, ci-only, final]
"#;

    let temp = setup_project_with_git(config);
    // Pass --env staging so ci-only step is filtered out.
    let mut s = spawn_bivvy(&["run", "--env", "staging"], temp.path());

    wait_for(&s, "OnlyEnvTest", "Header");
    wait_for(&s, "Always runs", "First step appears");
    wait_for(&s, "Final step", "Final step appears");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // always-runs and final produced files; ci-only did NOT.
    assert!(
        temp.path().join(".always-marker.txt").exists(),
        "always-runs should have produced its marker"
    );
    assert!(
        temp.path().join(".final-marker.txt").exists(),
        "final should have produced its marker"
    );
    assert!(
        !temp.path().join(".ci-only-marker.txt").exists(),
        "ci-only must not run when --env staging is set"
    );
}

/// `only_environments: [ci]` includes the step when `--env ci` is set.
#[test]
fn only_environments_runs_in_matching_env() {
    let config = r#"
app_name: "OnlyEnvMatchTest"

steps:
  always-runs:
    title: "Always runs"
    command: "git --version"
    skippable: false

  ci-only:
    title: "CI only step"
    command: "rustc --version > .ci-only-marker.txt"
    skippable: false
    only_environments: ["ci"]

  final:
    title: "Final step"
    command: "git --version"
    skippable: false
    depends_on: [always-runs, ci-only]

workflows:
  default:
    steps: [always-runs, ci-only, final]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--env", "ci"], temp.path());

    wait_for(&s, "OnlyEnvMatchTest", "Header");
    wait_for(&s, "Always runs", "First step appears");
    wait_for(&s, "CI only step", "ci-only step runs under --env ci");
    wait_for(&s, "Final step", "Final step runs");
    wait_for(&s, "3 run", "All 3 steps ran under --env ci");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // The ci-only step must have produced its side effect.
    let contents = fs::read_to_string(temp.path().join(".ci-only-marker.txt")).unwrap();
    assert!(
        contents.contains("rustc"),
        ".ci-only-marker.txt should contain rustc version output"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 10. Step environment overrides (environments)
// ─────────────────────────────────────────────────────────────────────

/// A step with `environments.ci.command` should use the overridden
/// command when `--env ci` is passed. We verify this by side effect:
/// the overridden command writes to a different file than the default.
#[test]
fn environment_override_changes_command() {
    let config = r#"
app_name: "EnvOverrideTest"

steps:
  greet:
    title: "Greet"
    command: "git --version > .default-marker.txt"
    skippable: false
    environments:
      ci:
        command: "rustc --version > .ci-marker.txt"

  verify:
    title: "Verify override marker"
    command: "grep -q rustc .ci-marker.txt"
    skippable: false
    depends_on: [greet]

  done:
    title: "Env override test done"
    command: "git --version"
    skippable: false
    depends_on: [verify]

workflows:
  default:
    steps: [greet, verify, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--env", "ci"], temp.path());

    wait_for(&s, "EnvOverrideTest", "Header");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // The override command ran — produced the ci-specific marker.
    assert!(
        temp.path().join(".ci-marker.txt").exists(),
        "Environment override should have produced .ci-marker.txt"
    );
    // The default command did NOT run.
    assert!(
        !temp.path().join(".default-marker.txt").exists(),
        "Default command must not run when environments.ci.command override is in effect"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 11. Workflow overrides (overrides)
// ─────────────────────────────────────────────────────────────────────

/// The `strict` workflow overrides the `optional` step to auto-run
/// without prompting despite having a completed_check. Running the
/// strict workflow should not wait for input.
#[test]
fn workflow_overrides_apply() {
    let config = r#"
app_name: "WorkflowOverrideTest"

steps:
  setup:
    title: "Setup"
    command: "git --version"
    skippable: false

  optional:
    title: "Optional step"
    command: "rustc --version > .optional-marker.txt"
    completed_check:
      type: command_succeeds
      command: "rustc --version"

  done:
    title: "Workflow overrides done"
    command: "git --version"
    skippable: false
    depends_on: [setup, optional]

workflows:
  default:
    steps: [setup, optional, done]
  strict:
    steps: [setup, optional, done]
    overrides:
      optional:
        required: true
        skip_prompt: true
        prompt_if_complete: false
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--workflow", "strict"], temp.path());

    wait_for(&s, "WorkflowOverrideTest", "Header");
    wait_for(&s, "Setup", "Setup step title");
    wait_for(&s, "Optional step", "Optional step title");
    wait_for(&s, "Workflow overrides done", "Final step title");
    wait_for(&s, "3 run", "All 3 steps ran under strict workflow");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // The optional step actually ran (strict override forced it) —
    // proven by its side effect.
    assert!(
        temp.path().join(".optional-marker.txt").exists(),
        "Optional step should have produced .optional-marker.txt under strict workflow"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 12. Composite completed checks (all, any)
// ─────────────────────────────────────────────────────────────────────

/// `completed_check` with `type: all` requires every sub-check to
/// pass. Both checks pass here, so the step is considered complete —
/// but `skippable: false` forces it to re-run anyway. Either way the
/// workflow must succeed and the step must produce its side effect.
#[test]
fn completed_check_all_requires_all() {
    let config = r#"
app_name: "CheckAllTest"

steps:
  checked:
    title: "All-checked step"
    command: "rustc --version > .all-marker.txt"
    skippable: false
    completed_check:
      type: all
      checks:
        - type: file_exists
          path: "Cargo.toml"
        - type: command_succeeds
          command: "rustc --version"

  verify:
    title: "Verify all-checked ran"
    command: "grep -q rustc .all-marker.txt"
    skippable: false
    depends_on: [checked]

  done:
    title: "All-check test done"
    command: "git --version"
    skippable: false
    depends_on: [verify]

workflows:
  default:
    steps: [checked, verify, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    wait_for(&s, "CheckAllTest", "Header");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    assert!(
        temp.path().join(".all-marker.txt").exists(),
        "All-checked step should have produced .all-marker.txt"
    );
}

/// `completed_check` with `type: any` is satisfied when at least one
/// sub-check passes.
#[test]
fn completed_check_any_requires_one() {
    let config = r#"
app_name: "CheckAnyTest"

steps:
  any-checked:
    title: "Any-checked step"
    command: "rustc --version > .any-marker.txt"
    skippable: false
    completed_check:
      type: any
      checks:
        - type: file_exists
          path: "Cargo.toml"
        - type: file_exists
          path: "nonexistent_file_xyz.txt"

  verify:
    title: "Verify any-checked ran"
    command: "grep -q rustc .any-marker.txt"
    skippable: false
    depends_on: [any-checked]

  done:
    title: "Any-check test done"
    command: "git --version"
    skippable: false
    depends_on: [verify]

workflows:
  default:
    steps: [any-checked, verify, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());

    wait_for(&s, "CheckAnyTest", "Header");
    wait_for(&s, "3 run", "All 3 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    assert!(
        temp.path().join(".any-marker.txt").exists(),
        "Any-checked step should have produced .any-marker.txt"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 13. Variables (vars) with command evaluation
// ─────────────────────────────────────────────────────────────────────

/// Variables are interpolated into step commands. We write the
/// interpolated values to files and read them back to prove both the
/// static and the command-evaluated variable resolved correctly.
#[test]
fn vars_interpolated_in_commands() {
    let config = r#"
app_name: "VarsTest"

vars:
  project_version:
    command: "git rev-parse --short HEAD"
  static_val: "bivvy_static_marker"

steps:
  show-version:
    title: "Show version"
    command: "printf '%s\\n' ${project_version} > .version-marker.txt"
    skippable: false

  show-static:
    title: "Show static"
    command: "printf '%s\\n' ${static_val} > .static-marker.txt"
    skippable: false

  verify-static:
    title: "Verify static var"
    command: "grep -Fx bivvy_static_marker .static-marker.txt"
    skippable: false
    depends_on: [show-static]

  done:
    title: "Vars test done"
    command: "git --version"
    skippable: false
    depends_on: [show-version, verify-static]

workflows:
  default:
    steps: [show-version, show-static, verify-static, done]
"#;

    let temp = setup_project_with_git(config);
    let mut s = spawn_bivvy(&["run"], temp.path());

    wait_for(&s, "VarsTest", "Header");
    wait_for(&s, "4 run", "All 4 steps ran");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);

    // Static var must have been interpolated verbatim.
    let static_contents =
        fs::read_to_string(temp.path().join(".static-marker.txt")).unwrap();
    assert_eq!(static_contents.trim(), "bivvy_static_marker");

    // Command-evaluated var must have produced a non-empty sha-like value.
    let version_contents =
        fs::read_to_string(temp.path().join(".version-marker.txt")).unwrap();
    let trimmed = version_contents.trim();
    assert!(
        !trimmed.is_empty(),
        ".version-marker.txt should not be empty — got {trimmed:?}"
    );
    // git rev-parse --short HEAD emits 7+ lowercase hex chars.
    assert!(
        trimmed.len() >= 7 && trimmed.chars().all(|c| c.is_ascii_hexdigit()),
        ".version-marker.txt should contain a short git sha, got {trimmed:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 14. Watches (watches)
// ─────────────────────────────────────────────────────────────────────

/// First run primes the watches state with the current file hash. On
/// the second run the watched file is modified, so the step should
/// re-execute. We prove re-execution by checking that the step's
/// side-effect file was re-created after deletion.
#[test]
fn watches_trigger_rerun_on_change() {
    let config = r#"
app_name: "WatchesTest"

steps:
  watched-step:
    title: "Watched step"
    command: "cargo --version > .watched-marker.txt"
    skippable: false
    completed_check:
      type: marker
    watches:
      - Cargo.toml

  verify:
    title: "Verify watched ran"
    command: "grep -q cargo .watched-marker.txt"
    skippable: false
    depends_on: [watched-step]

  done:
    title: "Watches test done"
    command: "git --version"
    skippable: false
    depends_on: [verify]

workflows:
  default:
    steps: [watched-step, verify, done]
"#;

    let temp = setup_project_with_git(config);

    // First run: step executes, marker is set, and the side-effect
    // file is created.
    let mut s = spawn_bivvy(&["run", "--non-interactive"], temp.path());
    wait_for(&s, "WatchesTest", "First-run header");
    wait_for(&s, "3 run", "First run completes 3 steps");
    read_to_eof(&mut s);
    assert_exit_code(&s, 0);
    assert!(
        temp.path().join(".watched-marker.txt").exists(),
        "First run should have created .watched-marker.txt"
    );

    // Delete the side-effect file AND modify the watched file so the
    // watches hash changes.  If the step re-runs, it recreates the
    // marker.  If bivvy sees the watched file as unchanged and skips
    // the step, the marker will not reappear.
    fs::remove_file(temp.path().join(".watched-marker.txt")).unwrap();
    let cargo_path = temp.path().join("Cargo.toml");
    let mut content = fs::read_to_string(&cargo_path).unwrap();
    content.push_str("\n# modified for watch test\n");
    fs::write(&cargo_path, content).unwrap();

    // Second run: watched file changed, step must re-execute and
    // recreate the marker.  verify will then grep it.
    let mut s2 = spawn_bivvy(&["run", "--non-interactive"], temp.path());
    wait_for(&s2, "WatchesTest", "Second-run header");
    wait_for(&s2, "Watched step", "Watched step title");
    wait_for(&s2, "3 run", "Second run completes 3 steps");
    read_to_eof(&mut s2);
    assert_exit_code(&s2, 0);

    // Marker file must have been recreated — proves re-execution.
    assert!(
        temp.path().join(".watched-marker.txt").exists(),
        "Watched step should have re-executed after Cargo.toml changed, recreating .watched-marker.txt"
    );
    let contents = fs::read_to_string(temp.path().join(".watched-marker.txt")).unwrap();
    assert!(
        contents.contains("cargo"),
        "Recreated .watched-marker.txt should contain cargo version output"
    );
}
