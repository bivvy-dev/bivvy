//! Integration tests for config module public API.

use bivvy::config::{
    load_merged_config, resolve_string, validate, BivvyConfig, InterpolationContext, OutputMode,
};
use std::fs;
use tempfile::TempDir;

#[test]
fn public_api_is_accessible() {
    // Verify types are exported correctly
    let _config = BivvyConfig::default();
    let _ctx = InterpolationContext::new();
    let _mode = OutputMode::Verbose;
}

#[test]
fn full_config_workflow() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();

    fs::write(
        bivvy_dir.join("config.yml"),
        r#"
app_name: TestApp
steps:
  test:
    command: "echo ${message}"
workflows:
  default:
    steps: [test]
"#,
    )
    .unwrap();

    let config = load_merged_config(temp.path()).unwrap();
    validate(&config).unwrap();

    let mut ctx = InterpolationContext::new();
    ctx.prompts
        .insert("message".to_string(), "hello".to_string());

    let command = resolve_string(config.steps["test"].command.as_ref().unwrap(), &ctx).unwrap();

    assert_eq!(command, "echo hello");
}

#[test]
fn config_merge_workflow() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();

    // Base config
    fs::write(
        bivvy_dir.join("config.yml"),
        r#"
app_name: BaseApp
settings:
  default_output: verbose
steps:
  deps:
    command: "yarn install"
"#,
    )
    .unwrap();

    // Local override
    fs::write(
        bivvy_dir.join("config.local.yml"),
        r#"
settings:
  default_output: quiet
steps:
  deps:
    command: "yarn install --frozen-lockfile"
"#,
    )
    .unwrap();

    let config = load_merged_config(temp.path()).unwrap();

    // App name from base
    assert_eq!(config.app_name, Some("BaseApp".to_string()));

    // Setting overridden by local
    assert_eq!(config.settings.default_output, OutputMode::Quiet);

    // Step command overridden by local
    assert_eq!(
        config.steps["deps"].command,
        Some("yarn install --frozen-lockfile".to_string())
    );
}

#[test]
fn interpolation_context_workflow() {
    let ctx = InterpolationContext::new()
        .with_project("myapp", std::path::Path::new("/projects/myapp"))
        .with_env(std::collections::HashMap::from([(
            "RAILS_ENV".to_string(),
            "development".to_string(),
        )]));

    // Resolve with builtins
    let result = resolve_string("Project: ${project_name}", &ctx).unwrap();
    assert_eq!(result, "Project: myapp");

    // Resolve with env
    let result = resolve_string("Environment: ${RAILS_ENV}", &ctx).unwrap();
    assert_eq!(result, "Environment: development");
}
