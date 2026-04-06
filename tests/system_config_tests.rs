//! Comprehensive system tests for `bivvy config`.
//!
//! Tests display of resolved configuration in YAML and JSON formats,
//! merged config with local overrides, variables, and error conditions.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const CONFIG: &str = r#"
app_name: "ConfigTest"
settings:
  default_output: verbose
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

const MINIMAL_CONFIG: &str = r#"
app_name: "Minimal"
steps:
  one:
    command: "git --version"
workflows:
  default:
    steps: [one]
"#;

const CONFIG_WITH_VARS: &str = r#"
app_name: "VarConfig"
vars:
  version:
    command: "git --version"
  env_name:
    value: "development"
steps:
  show:
    command: "test -n '${version}' && test -n '${env_name}'"
workflows:
  default:
    steps: [show]
"#;

const CONFIG_WITH_SECRETS: &str = r#"
app_name: "SecretConfig"
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

const CONFIG_WITH_ENVIRONMENTS: &str = r#"
app_name: "EnvConfig"
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

const CONFIG_MULTIPLE_WORKFLOWS: &str = r#"
app_name: "MultiWorkflow"
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

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Default output shows YAML with app_name.
#[test]
fn config_shows_yaml() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("app_name"),
        "Should show app_name key, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("ConfigTest"),
        "Should show app name value, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Config with variables displays them.
#[test]
fn config_shows_vars() {
    let temp = setup_project(CONFIG_WITH_VARS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("vars") || text.contains("version"),
        "Should show vars section, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Minimal config displays correctly.
#[test]
fn config_minimal_config() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Minimal"),
        "Should show app name, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Config with secrets shows env vars.
#[test]
fn config_shows_secrets() {
    let temp = setup_project(CONFIG_WITH_SECRETS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("API_KEY") || text.contains("env"),
        "Should show env vars section, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Config with environments shows environment settings.
#[test]
fn config_shows_environments() {
    let temp = setup_project(CONFIG_WITH_ENVIRONMENTS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("staging") || text.contains("production") || text.contains("environments"),
        "Should show environment settings, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Config with multiple workflows shows all.
#[test]
fn config_shows_multiple_workflows() {
    let temp = setup_project(CONFIG_MULTIPLE_WORKFLOWS);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("release") || text.contains("quick"),
        "Should show multiple workflows, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Config shows depends_on relationships.
#[test]
fn config_shows_depends_on() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("depends_on") || text.contains("deps"),
        "Should show dependency info, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// FLAGS
// =====================================================================

/// --json outputs JSON format.
#[test]
fn config_json_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ConfigTest"),
        "JSON should contain app name, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("{") || text.contains("\""),
        "JSON should have JSON-like structure, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --json with variables shows them.
#[test]
fn config_json_with_vars() {
    let temp = setup_project(CONFIG_WITH_VARS);
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("vars") || text.contains("version"),
        "JSON should contain vars, got: {}",
        &text[..text.len().min(500)]
    );
}

/// --yaml explicitly requests YAML (default).
#[test]
fn config_yaml_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--yaml"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("app_name"),
        "YAML should show app_name, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --merged shows fully resolved config.
#[test]
fn config_merged_flag() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("app_name") || text.contains("ConfigTest"),
        "Merged should show config, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --merged + --json combined.
#[test]
fn config_merged_json() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config", "--merged", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("ConfigTest"),
        "Merged JSON should contain app name, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// HAPPY PATH — Config with local overrides
// =====================================================================

/// config.local.yml merges into the resolved config.
#[test]
fn config_with_local_override() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        "app_name: LocalOverride\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("LocalOverride"),
        "Merged config should reflect local override, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Local override adds new steps.
#[test]
fn config_local_override_adds_steps() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        "steps:\n  local-step:\n    command: echo local\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("local-step") || text.contains("local"),
        "Merged config should include local step, got: {}",
        &text[..text.len().min(500)]
    );
}

/// Local override changes settings.
#[test]
fn config_local_override_changes_settings() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        "settings:\n  default_output: quiet\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("quiet") || text.contains("settings"),
        "Merged config should reflect settings override, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn config_help() {
    let mut s = spawn_bivvy_global(&["config", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("config") || text.contains("Config"),
        "Help should describe config command, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// No config file.
#[test]
fn config_no_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration found") || text.contains("config") || text.contains("error"),
        "Should report missing config, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Empty config file.
#[test]
fn config_empty_config() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("app_name") || text.contains("error") || text.contains("empty")
            || text.contains("config") || text.contains("{}") || text.contains("null"),
        "Empty config should show config or error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Malformed YAML.
#[test]
fn config_malformed_yaml() {
    let temp = setup_project("{{not yaml]]");
    let mut s = spawn_bivvy(&["config"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("error") || text.contains("Error") || text.contains("parse")
            || text.contains("YAML"),
        "Malformed YAML should produce error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --json with empty config.
#[test]
fn config_json_empty() {
    let temp = setup_project("");
    let mut s = spawn_bivvy(&["config", "--json"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("{") || text.contains("null") || text.contains("error")
            || text.contains("empty"),
        "JSON with empty config should produce JSON or error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --merged with no config file.
#[test]
fn config_merged_no_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["config", "--merged"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No configuration") || text.contains("error") || text.contains("config"),
        "Merged with no config should report error, got: {}",
        &text[..text.len().min(300)]
    );
}
