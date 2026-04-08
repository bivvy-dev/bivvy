//! System tests for `bivvy list` — all interactive, PTY-based.
//!
//! Every test runs with an isolated `HOME` pointing at a temp dir so the
//! global bivvy store (`~/.bivvy/`) on the developer machine cannot leak
//! into test state. Where output is deterministic (single-step configs,
//! help text, error messages, JSON with sorted keys) we use `insta`
//! snapshots for regression detection. Where `HashMap`-backed step or
//! workflow ordering makes rendered output non-deterministic across runs,
//! we fall back to content assertions on specific user-facing substrings.
#![cfg(unix)]

mod system;

use std::path::Path;
use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────
//
// Every spawn in this file routes through the shared `spawn_bivvy` /
// `spawn_bivvy_global` helpers in `tests/system/helpers.rs`, which pin
// `HOME` and all four `XDG_*` base-directory variables to an isolated
// tempdir (see the module docs there).  Project-scoped tests use
// `<project>/.test_home`; help-only tests use the shared global home.

/// Sort `steps` and `workflows` arrays in a list JSON document by `name`
/// so the rendered snapshot is stable across runs (the underlying config
/// uses `HashMap`, which has non-deterministic iteration order).
fn sort_list_json(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        for key in ["steps", "workflows"] {
            if let Some(arr) = obj.get_mut(key).and_then(|v| v.as_array_mut()) {
                arr.sort_by(|a, b| {
                    a["name"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["name"].as_str().unwrap_or(""))
                });
            }
        }
    }
    value
}

/// Replace the absolute tempdir path in a PTY capture with a stable
/// placeholder so error snapshots don't flake on per-run paths.
fn normalize_tempdir(text: &str, temp: &Path) -> String {
    let tempdir_str = temp.display().to_string();
    text.replace(&tempdir_str, "[TEMPDIR]")
}

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const CONFIG: &str = r#"
app_name: "ListTest"
steps:
  deps:
    title: "Install dependencies"
    command: "cargo --version"
    description: "Install all project dependencies"
  build:
    title: "Build project"
    command: "rustc --version"
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

const TEMPLATE_CONFIG: &str = r#"
app_name: "TemplateTest"
steps:
  install:
    template: yarn-install
    description: "Install JS dependencies via yarn"
  lint:
    title: "Lint"
    command: "cargo clippy --version"
    description: "Run the linter"
  build:
    title: "Build"
    command: "cargo build --version"
    depends_on: [install, lint]
workflows:
  default:
    description: "Full build pipeline"
    steps: [install, lint, build]
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

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Lists steps and workflows with the documented section headers and an
/// environment line. The step/workflow map uses `HashMap` so rendered
/// order is non-deterministic — we assert on content, not layout.
#[test]
fn list_shows_steps_and_workflows() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps:"),
        "Should have Steps section, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(text.contains("deps"), "Should list deps step");
    assert!(text.contains("build"), "Should list build step");
    assert!(text.contains("test"), "Should list test step");
    assert!(text.contains("Workflows:"), "Should have Workflows section");
    assert!(text.contains("default"), "Should list default workflow");
    assert!(text.contains("quick"), "Should list quick workflow");
    assert!(text.contains("Environment:"), "Should show environment info");

    assert_exit_code(&s, 0);
}

/// Single-step config is deterministic (one entry) so we can snapshot the
/// full rendered output to detect layout and styling regressions.
#[test]
fn list_single_step_snapshot() {
    let temp = setup_project(SINGLE_STEP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_single_step_output", text);

    assert_exit_code(&s, 0);
}

/// Shows dependency information with the exact format from the source.
#[test]
fn list_shows_dependency_info() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Source code uses "└── depends on:" prefix (list.rs line 228) followed by
    // the dep list joined with ", " (line 229).
    assert!(
        text.contains("└── depends on:"),
        "Should show '└── depends on:' tree prefix, got: {}",
        &text[..text.len().min(500)]
    );
    // Verify an actual dep relationship is rendered — build depends on deps.
    let dep_line = text
        .lines()
        .find(|l| l.contains("depends on:"))
        .unwrap_or_else(|| {
            panic!(
                "Should have a line with 'depends on:', got: {}",
                &text[..text.len().min(500)]
            )
        });
    assert!(
        dep_line.contains("deps"),
        "depends-on line should reference the 'deps' dependency, got: {dep_line}"
    );

    assert_exit_code(&s, 0);
}

/// Shows template references in step listing with exact format.
#[test]
fn list_shows_template_references() {
    let temp = setup_project(TEMPLATE_CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Source uses format "(template: {template})" for template steps — see
    // list.rs line 204.
    assert!(
        text.contains("(template: yarn-install)"),
        "Should show template reference as '(template: yarn-install)', got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// Shows step descriptions.
#[test]
fn list_shows_step_descriptions() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Install all project dependencies"),
        "Should show step description 'Install all project dependencies', got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// Shows inline commands in step listing.
#[test]
fn list_shows_inline_commands() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Source renders inline commands as "{name} — {cmd}" (em-dash separator,
    // list.rs lines 207-211). The em-dash (U+2014) is the visible marker that
    // distinguishes a command entry from a template entry.
    assert!(
        text.contains("\u{2014} cargo --version"),
        "Should show inline command prefixed with em-dash '— cargo --version', got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("\u{2014} rustc --version"),
        "Should show inline command prefixed with em-dash '— rustc --version', got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// Shows workflow arrow format (step1 -> step2 -> step3).
#[test]
fn list_shows_workflow_arrow_format() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Source code joins workflow steps with " \u{2192} " (U+2192 RIGHTWARDS ARROW)
    // — see list.rs line 243.
    assert!(
        text.contains("deps \u{2192} build \u{2192} test"),
        "Should show arrow separators joining workflow steps in 'deps -> build -> test' order, got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// Shows workflow descriptions.
#[test]
fn list_shows_workflow_descriptions() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Quick check"),
        "Should show workflow description 'Quick check', got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

// =====================================================================
// FLAGS
// =====================================================================

/// --steps-only hides workflows.
#[test]
fn list_steps_only_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--steps-only"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Steps:"), "Should show Steps section");
    assert!(text.contains("deps"), "Should show deps step");
    assert!(text.contains("build"), "Should show build step");
    assert!(text.contains("test"), "Should show test step");
    assert!(
        !text.contains("Workflows:"),
        "Should NOT show Workflows section with --steps-only"
    );

    assert_exit_code(&s, 0);
}

/// --workflows-only hides steps.
#[test]
fn list_workflows_only_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--workflows-only"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Workflows:"), "Should show workflows");
    assert!(text.contains("default"), "Should show default workflow");
    assert!(text.contains("quick"), "Should show quick workflow");
    assert!(
        !text.contains("Steps:"),
        "Should NOT show Steps section with --workflows-only"
    );

    assert_exit_code(&s, 0);
}

/// --json outputs structured data -- parse and verify structure, then
/// snapshot the sorted document for regression detection.
#[test]
fn list_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    assert!(
        parsed["environment"].is_string(),
        "JSON should have environment key"
    );
    assert!(
        parsed["steps"].is_array(),
        "JSON should have steps array"
    );
    assert!(
        parsed["workflows"].is_array(),
        "JSON should have workflows array"
    );

    // Verify step names are present
    let step_names: Vec<&str> = parsed["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(step_names.contains(&"deps"), "JSON steps should contain deps");
    assert!(
        step_names.contains(&"build"),
        "JSON steps should contain build"
    );
    assert!(step_names.contains(&"test"), "JSON steps should contain test");

    // Verify workflow names
    let wf_names: Vec<&str> = parsed["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert!(
        wf_names.contains(&"default"),
        "JSON workflows should contain default"
    );
    assert!(
        wf_names.contains(&"quick"),
        "JSON workflows should contain quick"
    );

    // Snapshot the sorted JSON document so any schema drift (new fields,
    // renamed fields, unintended serialization changes) is caught.
    let sorted = sort_list_json(parsed);
    insta::assert_json_snapshot!(
        "list_json_full",
        sorted,
        { ".environment" => "[environment]" }
    );

    assert_exit_code(&s, 0);
}

/// --json includes step details (command, description, dependencies).
#[test]
fn list_json_includes_step_details() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    let steps = parsed["steps"].as_array().unwrap();
    // Find the deps step and verify its fields
    let deps = steps
        .iter()
        .find(|s| s["name"] == "deps")
        .expect("Should have deps step");
    assert_eq!(deps["command"], "cargo --version", "deps should have command");
    assert_eq!(
        deps["description"], "Install all project dependencies",
        "deps should have description"
    );

    // Find the build step and verify depends_on
    let build = steps
        .iter()
        .find(|s| s["name"] == "build")
        .expect("Should have build step");
    assert_eq!(build["depends_on"][0], "deps", "build should depend on deps");

    assert_exit_code(&s, 0);
}

/// --json includes workflow description.
#[test]
fn list_json_includes_workflow_description() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    let workflows = parsed["workflows"].as_array().unwrap();
    let quick = workflows
        .iter()
        .find(|w| w["name"] == "quick")
        .expect("Should have quick workflow");
    assert_eq!(
        quick["description"], "Quick check",
        "quick workflow should have description"
    );

    assert_exit_code(&s, 0);
}

/// --json full output snapshot for regression detection (single-step
/// config is deterministic so no sorting is required).
#[test]
fn list_json_snapshot() {
    let temp = setup_project(SINGLE_STEP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    insta::assert_json_snapshot!("list_json_single_step", parsed);

    assert_exit_code(&s, 0);
}

/// --steps-only + --json combined -- no workflows key; snapshot the
/// sorted document to catch schema drift.
#[test]
fn list_steps_only_json() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--steps-only", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    assert!(parsed["steps"].is_array(), "Should have steps array");
    assert!(
        parsed.get("workflows").is_none(),
        "Should NOT have workflows key with --steps-only"
    );

    let sorted = sort_list_json(parsed);
    insta::assert_json_snapshot!(
        "list_json_steps_only",
        sorted,
        { ".environment" => "[environment]" }
    );

    assert_exit_code(&s, 0);
}

/// --workflows-only + --json combined -- no steps key; snapshot the
/// sorted document to catch schema drift.
#[test]
fn list_workflows_only_json() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--workflows-only", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    assert!(
        parsed.get("steps").is_none(),
        "Should NOT have steps key with --workflows-only"
    );
    assert!(
        parsed["workflows"].is_array(),
        "Should have workflows array"
    );

    let sorted = sort_list_json(parsed);
    insta::assert_json_snapshot!(
        "list_json_workflows_only",
        sorted,
        { ".environment" => "[environment]" }
    );

    assert_exit_code(&s, 0);
}

// =====================================================================
// ENVIRONMENT
// =====================================================================

/// --env flag shows environment info.
#[test]
fn list_env_flag() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    // Source prints "  Environment: {env_name} ({source})" — see list.rs lines 176-181.
    // Look for the literal label followed by the resolved env name on the same
    // line, so we don't accidentally match the "staging_only" step name.
    let env_line = text.lines().find(|l| l.contains("Environment:")).unwrap_or_else(|| {
        panic!(
            "Should show 'Environment:' label, got: {}",
            &text[..text.len().min(500)]
        )
    });
    assert!(
        env_line.contains("staging"),
        "Environment line should contain 'staging', got: {env_line}"
    );
    // The source annotates the resolver source in parentheses — the --env flag
    // should make this "(--env flag)".
    assert!(
        env_line.contains("--env flag"),
        "Environment line should indicate '--env flag' as the source, got: {env_line}"
    );

    assert_exit_code(&s, 0);
}

/// In wrong environment, shows skipped steps with exact format.
#[test]
fn list_env_shows_skipped_steps() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    // Source uses format "(skipped in {})" for env-skipped steps (line 195 of list.rs)
    assert!(
        text.contains("(skipped in development)"),
        "Should show '(skipped in development)' for env-restricted steps, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("staging_only"),
        "Should show staging_only step name, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("prod_only"),
        "Should show prod_only step name, got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// In matching environment, matching step is not skipped; non-matching
/// step still IS marked skipped.
#[test]
fn list_env_no_skip_when_matching() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("staging_only"),
        "Should show staging_only step, got: {}",
        &text[..text.len().min(500)]
    );
    // staging_only should NOT be marked as skipped in staging env — the
    // exact "(skipped in staging)" marker is what the source emits for a
    // skipped step, so we only look for THAT literal marker to avoid
    // false positives from the step names themselves. Since prod_only IS
    // skipped in staging, the literal MUST still appear at least once;
    // the guarantee we want is "staging_only's OWN line is not marked
    // skipped", which we verify by inspecting that specific line.
    let staging_only_line = text
        .lines()
        .find(|l| l.contains("staging_only"))
        .unwrap_or_else(|| {
            panic!(
                "Should have a line for staging_only step, got: {}",
                &text[..text.len().min(500)]
            )
        });
    assert!(
        !staging_only_line.contains("(skipped in staging)"),
        "staging_only's line should not contain '(skipped in staging)' when env is staging, got line: {staging_only_line}"
    );
    // In staging, the staging_only step's command should appear (it's not
    // skipped), which is how we can tell list rendered the full step entry.
    assert!(
        text.contains("git log --oneline -1"),
        "staging_only's command should be shown when running in staging env, got: {}",
        &text[..text.len().min(500)]
    );
    // prod_only is still restricted to production, so it MUST be marked as
    // skipped in staging with the exact "(skipped in staging)" label.
    assert!(
        text.contains("(skipped in staging)"),
        "prod_only should be marked as '(skipped in staging)' when env is staging, got: {}",
        &text[..text.len().min(500)]
    );

    assert_exit_code(&s, 0);
}

/// --env + --json marks skipped steps correctly and the sorted JSON
/// document snapshot catches any schema drift in the `skipped` field.
#[test]
fn list_env_json_marks_skipped() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--json", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    let parsed: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("Should produce valid JSON: {e}\nGot: {text}"));

    let steps = parsed["steps"].as_array().unwrap();
    let staging_only = steps
        .iter()
        .find(|s| s["name"] == "staging_only")
        .expect("Should have staging_only step");
    assert_eq!(
        staging_only["skipped"], true,
        "staging_only should be skipped in development"
    );

    let setup = steps
        .iter()
        .find(|s| s["name"] == "setup")
        .expect("Should have setup step");
    // setup has no only_environments, so should not be skipped
    assert!(
        setup.get("skipped").is_none() || setup["skipped"] == false,
        "setup should not be skipped"
    );

    let sorted = sort_list_json(parsed);
    insta::assert_json_snapshot!("list_json_env_development_skipped", sorted);

    assert_exit_code(&s, 0);
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description -- verified via snapshot.
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

/// No config file -- should show exact error message and exit code 2.
#[test]
fn list_no_config_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Normalize tempdir paths out of any error output before snapshotting
    // so the snapshot is stable across runs. The snapshot itself is the
    // regression guard for the exact user-facing message.  The isolated
    // HOME lives at `<temp>/.test_home`, so a single normalization of
    // `temp.path()` covers both the project dir and the isolated HOME.
    let normalized = normalize_tempdir(&text, temp.path());
    assert!(
        normalized.contains("No configuration found. Run 'bivvy init' first."),
        "Should show exact error message \"No configuration found. Run 'bivvy init' first.\", got: {}",
        &normalized[..normalized.len().min(300)]
    );
    insta::assert_snapshot!("list_no_config_error", normalized);

    assert_exit_code(&s, 2);
}

/// Empty config file -- should still succeed with empty steps/workflows.
/// Full output is deterministic (no entries to iterate) so we snapshot it.
#[test]
fn list_empty_config() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("list_empty_config_output", text);

    assert_exit_code(&s, 0);
}

/// Malformed YAML -- should produce a specific parse error and exit code 1.
/// The exact error message is snapshot-guarded so regressions in the user
/// facing wording are caught.
#[test]
fn list_malformed_yaml() {
    let temp = setup_project("{{{{ not valid yaml");
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Normalize tempdir paths before snapshotting so the snapshot is
    // stable across runs.  The isolated HOME lives at `<temp>/.test_home`
    // so normalizing `temp.path()` covers it too.
    let normalized = normalize_tempdir(&text, temp.path());

    // main.rs wraps errors as "Error: {e}" (src/main.rs line 126) and
    // ConfigParseError displays as "Failed to parse config at {path}: {message}"
    // (src/error.rs line 23).
    assert!(
        normalized.contains("Error:"),
        "Malformed YAML should produce 'Error:' prefix from main.rs, got: {}",
        &normalized[..normalized.len().min(500)]
    );
    assert!(
        normalized.contains("Failed to parse config"),
        "Malformed YAML should surface the ConfigParseError display text \
         'Failed to parse config', got: {}",
        &normalized[..normalized.len().min(500)]
    );
    assert!(
        normalized.contains("config.yml"),
        "Error message should reference the config file path, got: {}",
        &normalized[..normalized.len().min(500)]
    );

    insta::assert_snapshot!("list_malformed_yaml_error", normalized);

    assert_exit_code(&s, 1);
}
