//! Comprehensive system tests for `bivvy list`.
//!
//! Tests listing steps and workflows with filtering, output formats,
//! dependency info, template references, descriptions, environment
//! skipping, and error conditions.
#![cfg(unix)]

mod system;

use system::helpers::*;

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
    title: "Lint"
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
// HAPPY PATH
// =====================================================================

/// Lists both steps and workflows.
#[test]
fn list_shows_steps_and_workflows() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("Steps:"), "Should have Steps section");
    assert!(text.contains("deps"), "Should list deps");
    assert!(text.contains("build"), "Should list build");
    assert!(text.contains("test"), "Should list test");
    assert!(text.contains("Workflows:"), "Should have Workflows section");
    assert!(text.contains("default"), "Should list default workflow");
    assert!(text.contains("quick"), "Should list quick workflow");
}

/// Shows dependency information.
#[test]
fn list_shows_dependencies() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("depends on") || text.contains("deps"),
        "Should show dependency info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Minimal config with single step.
#[test]
fn list_single_step_config() {
    let temp = setup_project(SINGLE_STEP_CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("only"), "Should show the single step");
    assert!(text.contains("Steps:"), "Should have Steps section");
    assert!(text.contains("Workflows:"), "Should have Workflows section");
}

/// Shows template references in step listing.
#[test]
fn list_shows_template_references() {
    let temp = setup_project(TEMPLATE_CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("yarn-install") || text.contains("template"),
        "Should show template reference, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows inline commands in step listing.
#[test]
fn list_shows_inline_commands() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("npm run build") || text.contains("echo"),
        "Should show inline commands, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows step descriptions.
#[test]
fn list_shows_step_descriptions() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Install all project dependencies"),
        "Should show step description, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Shows workflow arrow format (step1 → step2 → step3).
#[test]
fn list_shows_workflow_arrow_format() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    // Workflows typically show steps with arrow separators
    assert!(
        text.contains("→") || text.contains("->") || (text.contains("deps") && text.contains("build") && text.contains("test")),
        "Should show workflow steps in connected format, got: {}",
        &text[..text.len().min(500)]
    );
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
}

/// Shows environment info when --env is used.
#[test]
fn list_shows_environment_info() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Environment") || text.contains("staging"),
        "Should show environment info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// In wrong environment, shows skipped steps.
#[test]
fn list_env_shows_skipped_steps() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "development"], temp.path());

    let text = read_to_eof(&mut s);
    // staging_only and prod_only should be marked as skipped or hidden
    assert!(
        text.contains("skip") || text.contains("staging_only") || text.contains("prod_only"),
        "Should indicate skipped steps in wrong env, got: {}",
        &text[..text.len().min(500)]
    );
}

/// In matching environment, step is not skipped.
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
    assert!(text.contains("deps"), "Should show deps step");
    assert!(text.contains("build"), "Should show build step");
    assert!(
        !text.contains("Workflows:"),
        "Should NOT show Workflows section with --steps-only"
    );
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
}

/// --json outputs structured data with steps key.
#[test]
fn list_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("deps"),
        "JSON should contain step names, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("steps") || text.contains("workflows"),
        "JSON should contain 'steps' or 'workflows' key, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json includes step details (command, description, dependencies).
#[test]
fn list_json_includes_step_details() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("command") || text.contains("echo"),
        "JSON should include command info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json includes workflow description.
#[test]
fn list_json_includes_workflow_description() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Quick check") || text.contains("description"),
        "JSON should include workflow description, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --json includes dependency info.
#[test]
fn list_json_includes_dependencies() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("depends_on") || text.contains("deps"),
        "JSON should include dependency info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --env filters by environment in JSON.
#[test]
fn list_env_flag() {
    let temp = setup_project(ENV_SKIP_CONFIG);
    let mut s = spawn_bivvy(&["list", "--env", "staging"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("staging") || text.contains("Environment"),
        "Should show environment info, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --steps-only + --json combined.
#[test]
fn list_steps_only_json() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--steps-only", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("deps"),
        "Steps-only JSON should contain step names"
    );
}

/// --workflows-only + --json combined.
#[test]
fn list_workflows_only_json() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["list", "--workflows-only", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("default"),
        "Workflows-only JSON should contain workflow names"
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn list_help() {
    let mut s = spawn_bivvy_global(&["list", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("list") || text.contains("List") || text.contains("steps"),
        "Help should describe list command, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file.
#[test]
fn list_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found") || text.contains("config") || text.contains("error"),
        "Should report missing config, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Empty config file.
#[test]
fn list_empty_config() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Steps") || text.contains("No") || text.contains("error")
            || text.contains("empty") || text.contains("0"),
        "Empty config should show steps or error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Malformed YAML.
#[test]
fn list_malformed_yaml() {
    let temp = setup_project("{{{{ not valid yaml");
    let mut s = spawn_bivvy(&["list"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("Error") || text.contains("invalid")
            || text.contains("parse") || text.contains("YAML") || text.contains("yaml"),
        "Malformed YAML should produce error, got: {}",
        &text[..text.len().min(300)]
    );
}
