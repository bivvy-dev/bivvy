//! Comprehensive system tests for `bivvy config`.
//!
//! Tests display of resolved configuration in YAML and JSON formats,
//! merged config with local overrides, variables, and error conditions.
//!
//! These tests verify documented behavior of the `config` command:
//!   * Default output is YAML (same as `--yaml`).
//!   * `--json` emits a parseable JSON object of the resolved config.
//!   * `--merged` applies `.bivvy/config.local.yml` on top of the project
//!     config and lists both source files in the header.
//!   * Missing configuration exits with code 2 and the documented
//!     `No configuration found. Run 'bivvy init' first.` error.
//!   * Malformed YAML exits non-zero with a parse error surfaced to the
//!     user.
//!
//! Output assertions prefer `insta` snapshots over `contains(...)` checks
//! so structural regressions in YAML/JSON are caught rather than glossed
//! over by fuzzy substring matches, and every test verifies the process
//! exit code.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
//
// Every fixture uses real tool invocations (git/cargo/rustc) rather
// than shell builtins. Even though `config` never executes the steps,
// the commands are preserved in the rendered config and show up in
// snapshots, so keeping them realistic prevents the file from drifting
// into a pile of `echo foo` placeholders over time.
// ─────────────────────────────────────────────────────────────────────

/// Non-trivial baseline config: two steps with a dependency relationship
/// and an explicit `defaults.output` setting. Exercised by most of the
/// happy-path snapshot tests.
const CONFIG: &str = r#"app_name: "ConfigTest"
settings:
  defaults:
    output: verbose
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
  build:
    title: "Build project"
    command: "rustc --version"
    depends_on: [deps]
workflows:
  default:
    steps: [deps, build]
"#;

/// Smallest usable config — single step, single workflow. Used to
/// verify the config command doesn't crash on minimal input and to
/// pin down the "small config" snapshot shape.
const MINIMAL_CONFIG: &str = r#"app_name: "Minimal"
steps:
  one:
    command: "git --version"
workflows:
  default:
    steps: [one]
"#;

/// Config using the `vars` feature — exercises variable interpolation
/// captured in the resolved output.
const CONFIG_WITH_VARS: &str = r#"app_name: "VarConfig"
vars:
  version:
    command: "git --version"
  env_name:
    value: "development"
steps:
  show:
    command: "git log --grep '${version}' --branches '${env_name}'"
workflows:
  default:
    steps: [show]
"#;

/// Config with step-level `env:` entries. Ensures the config command
/// faithfully round-trips env blocks (including keys that look like
/// secrets — `config` is documented to show them verbatim; masking is
/// a run-time concern).
const CONFIG_WITH_SECRETS: &str = r#"app_name: "SecretConfig"
steps:
  deploy:
    command: "git --version"
    env:
      API_KEY: "secret-key-123"
      DB_PASSWORD: "hunter2"
workflows:
  default:
    steps: [deploy]
"#;

/// Config using the `environments` feature — staging and production
/// overrides of a single step. Verifies that environment-scoped step
/// overrides show up in the resolved output.
const CONFIG_WITH_ENVIRONMENTS: &str = r#"app_name: "EnvConfig"
settings:
  environments:
    staging:
      detect:
        - env: STAGING
    production:
      detect:
        - env: PRODUCTION
steps:
  deploy:
    command: "git --version"
    environments:
      staging:
        command: "git log --oneline -1"
      production:
        command: "cargo --version"
workflows:
  default:
    steps: [deploy]
"#;

/// Config with three workflows of varying length and descriptions.
/// Drives the snapshot that pins down workflow rendering.
const CONFIG_MULTIPLE_WORKFLOWS: &str = r#"app_name: "MultiWorkflow"
steps:
  build:
    command: "rustc --version"
  test:
    command: "cargo fmt --version"
    depends_on: [build]
  deploy:
    command: "git --version"
    depends_on: [test]
workflows:
  default:
    steps: [build, test]
  release:
    description: "Full release pipeline"
    steps: [build, test, deploy]
  quick:
    description: "Quick check"
    steps: [build]
"#;

/// Local override that replaces the project's `defaults.output`. Used
/// by the `--merged` tests to verify that `.bivvy/config.local.yml` is
/// layered on top of `.bivvy/config.yml`.
const LOCAL_OVERRIDE_SETTINGS: &str = "settings:\n  defaults:\n    output: quiet\n";

/// Local override that adds a brand new step. Used by the `--merged`
/// tests to verify new steps from the local config appear in the
/// resolved output. Uses `git --version` rather than `echo ...` so the
/// snapshot exercises a real command.
const LOCAL_OVERRIDE_ADD_STEP: &str =
    "steps:\n  local-step:\n    command: \"git --version\"\n";

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Strip the `# <path>` header block that `bivvy config` prints before
/// the serialized body.
///
/// The header contains tempdir-specific paths, so stripping it is
/// essential for stable snapshots. The header consists of one or more
/// `# ...` comment lines followed by a blank line.
fn strip_path_header(output: &str) -> String {
    let mut lines = output.lines().peekable();
    while let Some(line) = lines.peek() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            lines.next();
        } else {
            break;
        }
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    let trimmed = body.trim_end();
    format!("{trimmed}\n")
}

/// Extract the first balanced JSON object from PTY output.
///
/// `bivvy config --json` prints a `# <path>` header followed by a
/// pretty-printed JSON object. This helper finds the object bounds and
/// returns the JSON substring, panicking with a helpful message if no
/// object is present.
fn extract_json(output: &str) -> &str {
    let start = output
        .find('{')
        .unwrap_or_else(|| panic!("No JSON object in output:\n{output}"));
    let end = output
        .rfind('}')
        .unwrap_or_else(|| panic!("No closing '}}' in JSON output:\n{output}"));
    &output[start..=end]
}

// =====================================================================
// HAPPY PATH — YAML output
// =====================================================================

/// Default `bivvy config` output is YAML. Snapshot the full body so any
/// formatting or field-ordering regression is caught, and verify exit
/// code 0.
#[test]
fn config_tests_default_yaml_body() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_default_yaml_body", body);
    assert_exit_code(&s, 0);
}

/// The path header must point at the real project config file.
#[test]
fn config_tests_default_yaml_header_lists_project_config() {
    let temp = setup_project(CONFIG);
    let expected_header =
        format!("# {}", temp.path().join(".bivvy/config.yml").display());

    let mut s = spawn_bivvy(&["config"], temp.path());
    let text = read_to_eof(&mut s);

    assert!(
        text.contains(&expected_header),
        "Default YAML output should contain header line {expected_header:?}, got:\n{text}"
    );
    assert_exit_code(&s, 0);
}

/// Variables defined under `vars:` must round-trip into the resolved
/// YAML output. Snapshot the body for regression coverage.
#[test]
fn config_tests_vars_yaml_body() {
    let temp = setup_project(CONFIG_WITH_VARS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_vars_yaml_body", body);
    assert_exit_code(&s, 0);
}

/// A minimal one-step config still produces a valid, snapshotable
/// output and exits cleanly.
#[test]
fn config_tests_minimal_yaml_body() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_minimal_yaml_body", body);
    assert_exit_code(&s, 0);
}

/// Step-level `env:` entries (including secret-shaped keys) must appear
/// verbatim in the resolved output — `config` does not mask them.
#[test]
fn config_tests_env_block_yaml_body() {
    let temp = setup_project(CONFIG_WITH_SECRETS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_env_block_yaml_body", body);
    assert_exit_code(&s, 0);
}

/// The `environments` feature (detection rules + per-environment step
/// overrides) must round-trip into the resolved YAML.
#[test]
fn config_tests_environments_yaml_body() {
    let temp = setup_project(CONFIG_WITH_ENVIRONMENTS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_environments_yaml_body", body);
    assert_exit_code(&s, 0);
}

/// Multiple workflows (with descriptions of varying lengths) must all
/// render in the output.
#[test]
fn config_tests_multiple_workflows_yaml_body() {
    let temp = setup_project(CONFIG_MULTIPLE_WORKFLOWS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_multiple_workflows_yaml_body", body);
    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS
// =====================================================================

/// `--json` must emit parseable JSON that contains every field from
/// the input config. Parse the output and make targeted assertions on
/// the fields, then snapshot the parsed value so ordering differences
/// don't cause churn.
#[test]
fn config_tests_json_flag_parseable() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse JSON output: {e}\nJSON was:\n{json_str}"));

    // Targeted assertions — a bad JSON gives a specific failure rather
    // than an opaque snapshot diff.
    assert_eq!(parsed["app_name"], "ConfigTest");
    assert_eq!(parsed["settings"]["defaults"]["output"], "verbose");
    assert_eq!(parsed["steps"]["deps"]["command"], "cargo --version");
    assert_eq!(parsed["steps"]["build"]["command"], "rustc --version");
    assert_eq!(parsed["steps"]["build"]["depends_on"][0], "deps");
    assert_eq!(parsed["workflows"]["default"]["steps"][0], "deps");
    assert_eq!(parsed["workflows"]["default"]["steps"][1], "build");

    insta::assert_json_snapshot!("config_tests_json_full", parsed);
    assert_exit_code(&s, 0);
}

/// `--json` with a config containing `vars:` must include the vars
/// object in the output.
#[test]
fn config_tests_json_with_vars_parseable() {
    let temp = setup_project(CONFIG_WITH_VARS);
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse JSON output: {e}\nJSON was:\n{json_str}"));

    assert_eq!(parsed["app_name"], "VarConfig");
    assert_eq!(parsed["vars"]["version"]["command"], "git --version");
    assert_eq!(parsed["vars"]["env_name"]["value"], "development");

    insta::assert_json_snapshot!("config_tests_json_with_vars", parsed);
    assert_exit_code(&s, 0);
}

/// `--yaml` is the explicit form of the default output; its body must
/// be byte-identical to the default `bivvy config` output so that a
/// divergence between the two code paths is caught.
#[test]
fn config_tests_yaml_flag_matches_default() {
    let temp = setup_project(CONFIG);

    let mut default_session = spawn_bivvy(&["config"], temp.path());
    let default_text = read_to_eof(&mut default_session);
    let default_body = strip_path_header(&default_text);
    assert_exit_code(&default_session, 0);

    let mut yaml_session = spawn_bivvy(&["config", "--yaml"], temp.path());
    let yaml_text = read_to_eof(&mut yaml_session);
    let yaml_body = strip_path_header(&yaml_text);
    assert_exit_code(&yaml_session, 0);

    assert_eq!(
        default_body, yaml_body,
        "--yaml output should be byte-identical to default output\n\
         default:\n{default_body}\n\
         --yaml:\n{yaml_body}"
    );
}

/// `--merged` with no local override is equivalent to reading the
/// project config, and must still exit 0 with the full body rendered.
#[test]
fn config_tests_merged_without_local() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_merged_without_local", body);
    assert_exit_code(&s, 0);
}

/// `--merged` + `--json` combined must still emit parseable JSON with
/// the project config as its content (no local override present).
#[test]
fn config_tests_merged_json_parseable() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse merged JSON: {e}\nJSON was:\n{json_str}"));

    assert_eq!(parsed["app_name"], "ConfigTest");
    assert_eq!(parsed["settings"]["defaults"]["output"], "verbose");

    insta::assert_json_snapshot!("config_tests_merged_json", parsed);
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — Config with local overrides
// =====================================================================

/// `config.local.yml` changing `app_name` must be reflected in the
/// `--merged` output, and the header must list both source files.
#[test]
fn config_tests_merged_applies_local_override() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        "app_name: LocalOverride\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["config", "--merged", "--json"], temp.path());
    let text = read_to_eof(&mut s);

    // Verify both source paths appear in the header.
    let project_header =
        format!("# {}", temp.path().join(".bivvy/config.yml").display());
    let local_header = format!(
        "# {}",
        temp.path().join(".bivvy/config.local.yml").display()
    );
    assert!(
        text.contains(&project_header),
        "Merged header should list project config {project_header:?}, got:\n{text}"
    );
    assert!(
        text.contains(&local_header),
        "Merged header should list local config {local_header:?}, got:\n{text}"
    );

    // Parse the JSON body and verify the override took effect.
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse merged JSON: {e}\nJSON was:\n{json_str}"));

    assert_eq!(
        parsed["app_name"], "LocalOverride",
        "local override should replace project app_name"
    );
    // Project-defined steps should still be present.
    assert_eq!(parsed["steps"]["deps"]["command"], "cargo --version");
    assert_eq!(parsed["steps"]["build"]["command"], "rustc --version");

    insta::assert_json_snapshot!("config_tests_merged_app_name_override", parsed);
    assert_exit_code(&s, 0);
}

/// Adding a brand-new step via `config.local.yml` must show up in the
/// merged resolved config alongside the project steps.
#[test]
fn config_tests_merged_local_override_adds_step() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        LOCAL_OVERRIDE_ADD_STEP,
    )
    .unwrap();

    let mut s = spawn_bivvy(&["config", "--merged", "--json"], temp.path());
    let text = read_to_eof(&mut s);
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse merged JSON: {e}\nJSON was:\n{json_str}"));

    // Project steps preserved.
    assert_eq!(parsed["steps"]["deps"]["command"], "cargo --version");
    assert_eq!(parsed["steps"]["build"]["command"], "rustc --version");
    // Local-added step is present with its command intact.
    assert_eq!(
        parsed["steps"]["local-step"]["command"], "git --version",
        "locally added step should appear in merged config"
    );

    insta::assert_json_snapshot!("config_tests_merged_adds_step", parsed);
    assert_exit_code(&s, 0);
}

/// `config.local.yml` changing `settings.defaults.output` must override
/// the project setting in the merged output.
#[test]
fn config_tests_merged_local_override_changes_settings() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        LOCAL_OVERRIDE_SETTINGS,
    )
    .unwrap();

    let mut s = spawn_bivvy(&["config", "--merged", "--json"], temp.path());
    let text = read_to_eof(&mut s);
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("Failed to parse merged JSON: {e}\nJSON was:\n{json_str}"));

    assert_eq!(
        parsed["settings"]["defaults"]["output"], "quiet",
        "local override should change defaults.output from verbose to quiet"
    );
    // Project fields preserved.
    assert_eq!(parsed["app_name"], "ConfigTest");

    insta::assert_json_snapshot!("config_tests_merged_settings_override", parsed);
    assert_exit_code(&s, 0);
}

// =====================================================================
// HELP
// =====================================================================

/// `bivvy config --help` must render clap-generated help for the
/// subcommand. Snapshot the full output so flag renames/description
/// changes are caught, and verify exit code 0.
#[test]
fn config_tests_help_snapshot() {
    let mut s = spawn_bivvy_global(&["config", "--help"]);
    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("config_tests_help", text);
    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file in the project directory must produce the documented
/// error message and exit with code 2 (config-not-found).
#[test]
fn config_tests_no_config_exits_2() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found. Run 'bivvy init' first."),
        "Missing config should print the documented error, got:\n{text}"
    );
    assert_exit_code(&s, 2);
}

/// `--merged` with no config file should exit 2 with the same error
/// message — the merged path has its own handling and must stay in
/// sync with the non-merged path.
#[test]
fn config_tests_merged_no_config_exits_2() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found. Run 'bivvy init' first."),
        "Merged with missing config should print the documented error, got:\n{text}"
    );
    assert_exit_code(&s, 2);
}

/// An empty config file is still a (trivial) valid YAML document.
/// `bivvy config` must accept it, render the default-initialized
/// config as YAML, and exit 0. The body is snapshotted so the
/// canonical "empty" rendering is pinned down.
#[test]
fn config_tests_empty_config_body() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_tests_empty_config_body", body);
    assert_exit_code(&s, 0);
}

/// An empty config rendered as JSON must still be parseable — the
/// output must be a valid JSON object (possibly with default fields
/// only) and exit 0.
#[test]
fn config_tests_empty_config_json_parseable() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let json_str = extract_json(&text);
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap_or_else(|e| {
        panic!("Empty-config JSON output should still parse: {e}\nJSON was:\n{json_str}")
    });

    assert!(
        parsed.is_object(),
        "empty config JSON should be a JSON object, got: {parsed:?}"
    );

    insta::assert_json_snapshot!("config_tests_empty_config_json", parsed);
    assert_exit_code(&s, 0);
}

/// Malformed YAML must produce a parse error surfaced to the user.
/// The command must exit with a non-success code and emit an error
/// message mentioning the failure on its output stream.
#[test]
fn config_tests_malformed_yaml_fails() {
    let temp = setup_project("app_name: [unterminated\nsteps:\n  : : :\n");
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);

    // The top-level error wrapper produced by `main` prefixes the
    // propagated error with `Error: `. A well-behaved failure path
    // should also mention "yaml" or "parse" somewhere in the message.
    assert!(
        text.contains("Error:"),
        "Malformed YAML should produce an `Error:` prefix, got:\n{text}"
    );
    let lower = text.to_lowercase();
    assert!(
        lower.contains("yaml") || lower.contains("parse"),
        "Malformed YAML error should mention yaml/parse, got:\n{text}"
    );

    // Snapshot the error output so regressions in the error message
    // (phrasing, formatting, missing context) are caught. The tempdir
    // path is replaced with a stable placeholder so the snapshot is
    // stable across runs.
    let temp_path = temp.path().display().to_string();
    let stable = text.replace(&temp_path, "[TEMPDIR]");
    insta::assert_snapshot!("config_tests_malformed_yaml_error", stable);

    // Malformed YAML returns from the command via `?`, which flows
    // through `main` and produces exit code 1.
    assert_exit_code(&s, 1);
}
