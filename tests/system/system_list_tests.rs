//! Comprehensive system tests for `bivvy list`.
//!
//! Tests listing steps and workflows with filtering, output formats,
//! dependency info, template references, descriptions, environment
//! skipping, and error conditions.
//!
//! # Isolation
//!
//! Every spawn sets `HOME` (and `XDG_CONFIG_HOME`) to a fresh tempdir.
//! `bivvy list` calls `load_merged_config`, which reads the user's global
//! `~/.bivvy/config.yml`. Without isolation, a user's real global config
//! could leak into the test output and break snapshots.
//!
//! # Snapshots
//!
//! Human-readable CLI output is verified via `insta` snapshots rather than
//! substring matching. The one exception is a few spot checks on parsed
//! JSON values that cannot easily be made path-stable.
#![cfg(unix)]

mod system;

use expectrl::Session;
use std::path::Path;
use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────

/// Spawn `bivvy list ...` with an isolated `HOME` directory so the user's
/// global `~/.bivvy/config.yml` cannot leak into the test output.
///
/// Returns the session alongside the `HOME` tempdir — the caller must keep
/// the tempdir alive for the duration of the test.
fn spawn_bivvy_isolated(args: &[&str], dir: &Path) -> (Session, TempDir) {
    let home = TempDir::new().unwrap();
    let home_str = home.path().to_string_lossy().into_owned();
    // Point every XDG base-dir variable at the temp `HOME` so nothing
    // leaks from the developer's real environment on either macOS or
    // Linux. `read_to_eof` strips ANSI escapes, so color handling does
    // not need to be tweaked.
    let session = spawn_bivvy_with_env(
        args,
        dir,
        &[
            ("HOME", home_str.as_str()),
            ("XDG_CONFIG_HOME", home_str.as_str()),
            ("XDG_DATA_HOME", home_str.as_str()),
            ("XDG_CACHE_HOME", home_str.as_str()),
        ],
    );
    (session, home)
}

/// Normalize the tempdir path in an error message so snapshots are stable.
///
/// `bivvy list` error messages include the full project path (e.g.
/// `/tmp/.tmpXYZ/.bivvy/config.yml`). We replace the tempdir prefix with
/// a placeholder so the snapshot is deterministic.
fn normalize_temp_path(text: &str, temp_dir: &Path) -> String {
    let temp_str = temp_dir.to_string_lossy();
    text.replace(temp_str.as_ref(), "<TEMP>")
}

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const CONFIG: &str = r#"
app_name: "ListTest"
steps:
  deps:
    command: "cargo --version"
    description: "Install all project dependencies"
  build:
    title: "Build project"
    command: "npm run build"
    depends_on: [deps]
  test:
    title: "Run tests"
    command: "cargo fmt --version"
    depends_on: [build]
workflows:
  default:
    steps: [deps, build, test]
  quick:
    description: "Quick check"
    steps: [test]
"#;

const SINGLE_STEP_CONFIG: &str = r#"
app_name: "Minimal"
steps:
  only:
    title: "Only step"
    command: "rustc --version"
workflows:
  default:
    steps: [only]
"#;

const TEMPLATE_CONFIG: &str = r#"
app_name: "TemplateTest"
steps:
  install:
    template: yarn-install
    description: "Install JS dependencies via yarn"
  lint:
    command: "npm run lint"
    description: "Run the linter"
  build:
    title: "Build"
    command: "npm run build"
    depends_on: [install, lint]
workflows:
  default:
    description: "Full build pipeline"
    steps: [install, lint, build]
  ci:
    description: "CI-only pipeline"
    steps: [lint, build]
"#;

const ENV_SKIP_CONFIG: &str = r#"
app_name: "EnvSkipTest"
steps:
  setup:
    title: "Setup"
    command: "cargo --version"
  staging_only:
    title: "Staging deploy"
    command: "git log --oneline -1"
    only_environments: [staging]
  prod_only:
    title: "Prod deploy"
    command: "cargo --version"
    only_environments: [production]
workflows:
  default:
    steps: [setup, staging_only, prod_only]
"#;

// =====================================================================
// HAPPY PATH — full-output snapshots
// =====================================================================

/// Full human-readable output for the default CONFIG in `development` env.
/// Snapshot covers: Environment header, Steps section (with dependency
/// tree, inline commands, descriptions, and title-as-fallback), and the
/// Workflows section (with arrow formatting and workflow descriptions).
#[test]
fn list_shows_steps_and_workflows() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_shows_steps_and_workflows", text);

    assert_exit_code(&s, 0);
}

/// Minimal single-step config — full output snapshot.
#[test]
fn list_single_step_config() {
    let temp = setup_project(SINGLE_STEP_CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_single_step_config", text);

    assert_exit_code(&s, 0);
}

/// Template-based config — full output snapshot. Verifies the
/// `(template: <name>)` formatting, multi-workflow rendering, and
/// dependency tree with multiple dependencies.
#[test]
fn list_template_config_snapshot() {
    let temp = setup_project(TEMPLATE_CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_template_config", text);

    assert_exit_code(&s, 0);
}

/// Env-filtered list in the `staging` environment — full output snapshot.
/// Verifies that `staging_only` renders normally while `prod_only` is
/// shown as `(skipped in staging)`.
#[test]
fn list_env_staging_snapshot() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_env_staging", text);

    assert_exit_code(&s, 0);
}

/// Env-filtered list in the `development` environment — snapshot verifies
/// both `staging_only` and `prod_only` are shown as `(skipped in development)`.
#[test]
fn list_env_development_snapshot() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_env_development", text);

    assert_exit_code(&s, 0);
}

/// Env-filtered list in the `production` environment — snapshot verifies
/// `prod_only` renders normally while `staging_only` is skipped.
#[test]
fn list_env_production_snapshot() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "production"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_env_production", text);

    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS — human-readable
// =====================================================================

/// `--steps-only` hides the Workflows section. Full output snapshot.
#[test]
fn list_steps_only_flag() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--steps-only", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_steps_only", text);

    // Belt-and-braces: the Workflows section must not appear with
    // --steps-only, and verifying this at the assertion level (not just
    // the snapshot level) protects against accidental snapshot reviews
    // that add the header back.
    assert!(
        !text.contains("Workflows:"),
        "--steps-only output must not contain the 'Workflows:' header, got: {text}"
    );

    assert_exit_code(&s, 0);
}

/// `--workflows-only` hides the Steps section. Full output snapshot.
#[test]
fn list_workflows_only_flag() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(
        &["list", "--workflows-only", "--env", "development"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_workflows_only", text);

    // Same belt-and-braces: the Steps section must not appear.
    assert!(
        !text.contains("Steps:"),
        "--workflows-only output must not contain the 'Steps:' header, got: {text}"
    );

    assert_exit_code(&s, 0);
}

/// `--steps-only --workflows-only` combined — neither section renders,
/// only the environment header remains. Snapshot verifies that both
/// section headers are absent.
#[test]
fn list_steps_only_and_workflows_only_combined() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(
        &[
            "list",
            "--steps-only",
            "--workflows-only",
            "--env",
            "development",
        ],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_steps_only_and_workflows_only", text);

    assert!(
        !text.contains("Steps:"),
        "--steps-only --workflows-only output must not contain 'Steps:' header, got: {text}"
    );
    assert!(
        !text.contains("Workflows:"),
        "--steps-only --workflows-only output must not contain 'Workflows:' header, got: {text}"
    );

    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS — JSON
// =====================================================================

/// `--json` outputs structured data — full snapshot for regression
/// detection. Uses `insta::assert_json_snapshot!` so the snapshot is a
/// canonical pretty-printed JSON document.
#[test]
fn list_json_flag() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_full", parsed);

    assert_exit_code(&s, 0);
}

/// `--json` — focused structural assertions complement the full snapshot.
/// This test ensures specific step fields are present (command,
/// description, dependencies) so future field additions don't silently
/// regress these core fields even if the snapshot is accepted wholesale.
#[test]
fn list_json_includes_step_details() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    let steps = parsed["steps"].as_array().expect("`steps` should be an array");
    let deps_step = steps
        .iter()
        .find(|s| s["name"] == "deps")
        .expect("`deps` step should be in the JSON output");
    assert_eq!(deps_step["command"], "cargo --version");
    assert_eq!(deps_step["description"], "Install all project dependencies");

    let build_step = steps
        .iter()
        .find(|s| s["name"] == "build")
        .expect("`build` step should be in the JSON output");
    assert_eq!(build_step["command"], "npm run build");
    assert_eq!(build_step["depends_on"][0], "deps");

    // Also snapshot the parsed value so the overall shape is regression-checked.
    insta::assert_json_snapshot!("list_json_step_details_parsed", parsed);

    assert_exit_code(&s, 0);
}

/// `--json` includes workflow description. Focused structural check plus
/// a snapshot of the workflows section.
#[test]
fn list_json_includes_workflow_description() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    let workflows = parsed["workflows"]
        .as_array()
        .expect("`workflows` should be an array");
    let quick_wf = workflows
        .iter()
        .find(|w| w["name"] == "quick")
        .expect("`quick` workflow should be in the JSON output");
    assert_eq!(quick_wf["description"], "Quick check");

    insta::assert_json_snapshot!(
        "list_json_workflow_description_parsed",
        parsed["workflows"]
    );

    assert_exit_code(&s, 0);
}

/// `--env` with `--json` — the JSON output must carry the `skipped` flag
/// for env-restricted steps. Full snapshot exercises the shape.
#[test]
fn list_env_flag_json() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--json", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_env_staging", parsed);

    // Focused structural check: `staging_only` must NOT be skipped, and
    // `prod_only` MUST be skipped.
    let steps = parsed["steps"].as_array().unwrap();
    let staging_only = steps.iter().find(|s| s["name"] == "staging_only").unwrap();
    let prod_only = steps.iter().find(|s| s["name"] == "prod_only").unwrap();
    assert_eq!(
        staging_only["skipped"], false,
        "`staging_only` must not be skipped in staging env"
    );
    assert_eq!(
        prod_only["skipped"], true,
        "`prod_only` must be skipped in staging env"
    );

    assert_exit_code(&s, 0);
}

/// `--steps-only --json` — no `workflows` key, verified via snapshot and
/// focused structural assertion.
#[test]
fn list_steps_only_json() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(
        &["list", "--steps-only", "--json", "--env", "development"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_steps_only", parsed);
    assert!(
        parsed.get("workflows").is_none() || parsed["workflows"].is_null(),
        "`--steps-only --json` must omit the workflows key, got: {parsed}"
    );

    assert_exit_code(&s, 0);
}

/// `--workflows-only --json` — no `steps` key, verified via snapshot and
/// focused structural assertion.
#[test]
fn list_workflows_only_json() {
    let temp = setup_project(CONFIG);
    let (mut s, _home) = spawn_bivvy_isolated(
        &["list", "--workflows-only", "--json", "--env", "development"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_workflows_only", parsed);
    assert!(
        parsed.get("steps").is_none() || parsed["steps"].is_null(),
        "`--workflows-only --json` must omit the steps key, got: {parsed}"
    );

    assert_exit_code(&s, 0);
}

/// `--json` with the single-step config — baseline snapshot for the
/// simplest possible input.
#[test]
fn list_json_single_step_snapshot() {
    let temp = setup_project(SINGLE_STEP_CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_single_step", parsed);

    assert_exit_code(&s, 0);
}

/// `--json` with the template config — the `template` field must appear
/// in JSON output for template-backed steps.
#[test]
fn list_json_template_config() {
    let temp = setup_project(TEMPLATE_CONFIG);
    let (mut s, _home) =
        spawn_bivvy_isolated(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_template_config", parsed);

    // Focused structural check: the `install` step must carry the
    // `template` field set to `yarn-install`.
    let steps = parsed["steps"].as_array().unwrap();
    let install = steps.iter().find(|s| s["name"] == "install").unwrap();
    assert_eq!(install["template"], "yarn-install");

    assert_exit_code(&s, 0);
}

// =====================================================================
// HELP
// =====================================================================

/// `list --help` output is verified via snapshot.
#[test]
fn list_help() {
    let mut s = spawn_bivvy_global(&["list", "--help"]);
    let text = read_to_eof(&mut s);

    insta::assert_snapshot!("list_help", text);

    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file — should show the exact error message and exit with
/// code 2 (BivvyError::ConfigNotFound path in list.rs).
#[test]
fn list_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let (mut s, _home) = spawn_bivvy_isolated(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    let normalized = normalize_temp_path(&text, temp.path());
    insta::assert_snapshot!("list_no_config_fails", normalized);

    assert_exit_code(&s, 2);
}

/// Empty config file — should succeed and show empty Steps and Workflows
/// sections. Full output snapshot.
#[test]
fn list_empty_config() {
    let temp = setup_project("");
    let (mut s, _home) = spawn_bivvy_isolated(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_empty_config", text);

    assert_exit_code(&s, 0);
}

/// Malformed YAML — should produce the "Failed to parse config" error
/// and exit with code 1 (BivvyError::ConfigParseError path in main.rs).
#[test]
fn list_malformed_yaml() {
    let temp = setup_project("{{{{ not valid yaml");
    let (mut s, _home) = spawn_bivvy_isolated(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    let normalized = normalize_temp_path(&text, temp.path());
    insta::assert_snapshot!("list_malformed_yaml", normalized);

    assert_exit_code(&s, 1);
}
