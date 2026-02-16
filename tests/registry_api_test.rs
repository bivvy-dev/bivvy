//! Integration tests for the registry public API.

use bivvy::config::StepConfig;
use bivvy::registry::{Registry, TemplateSource};
use bivvy::steps::ResolvedStep;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

#[test]
fn public_api_accessible() {
    let registry = Registry::new(None).unwrap();
    let _names = registry.all_template_names();
}

#[test]
fn resolve_builtin_template() {
    let registry = Registry::new(None).unwrap();
    let (template, source) = registry.resolve("brew").unwrap();

    assert_eq!(template.name, "brew");
    assert_eq!(source, TemplateSource::Builtin);
}

#[test]
fn full_template_workflow() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();

    // Create config using a template
    fs::write(
        bivvy_dir.join("config.yml"),
        r#"
steps:
  deps:
    template: brew
"#,
    )
    .unwrap();

    // Load registry
    let registry = Registry::new(Some(temp.path())).unwrap();

    // Resolve template
    let (template, source) = registry.resolve("brew").unwrap();
    assert_eq!(source, TemplateSource::Builtin);

    // Create resolved step
    let step_config = StepConfig {
        template: Some("brew".to_string()),
        ..Default::default()
    };

    let resolved =
        ResolvedStep::from_template("deps", template, &step_config, &HashMap::new(), None);

    assert!(!resolved.command.is_empty());
    assert_eq!(resolved.name, "deps");
}

#[test]
fn local_template_override() {
    let temp = TempDir::new().unwrap();
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir).unwrap();

    // Create local template that shadows brew
    let local_brew = r#"
name: brew
description: "Local brew override"
category: local
step:
  title: "Local Brew"
  command: "echo local brew"
"#;
    fs::write(templates_dir.join("brew.yml"), local_brew).unwrap();

    let registry = Registry::new(Some(temp.path())).unwrap();
    let (template, source) = registry.resolve("brew").unwrap();

    assert_eq!(template.description, "Local brew override");
    assert_eq!(source, TemplateSource::Project);
}

#[test]
fn template_input_validation_workflow() {
    let temp = TempDir::new().unwrap();
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir).unwrap();

    let template_with_inputs = r#"
name: database
description: "Database setup"
category: common
inputs:
  db_name:
    description: "Database name"
    type: string
    required: true
  environment:
    description: "Environment"
    type: enum
    values: [development, test, production]
    default: development
step:
  command: "echo setup ${db_name} for ${environment}"
"#;
    fs::write(templates_dir.join("database.yml"), template_with_inputs).unwrap();

    let registry = Registry::new(Some(temp.path())).unwrap();

    // Validate missing required input
    let errors = registry
        .validate_inputs("database", &HashMap::new())
        .unwrap();
    assert!(!errors.is_empty());

    // Validate with valid inputs
    let mut inputs = HashMap::new();
    inputs.insert(
        "db_name".to_string(),
        serde_yaml::Value::String("mydb".to_string()),
    );
    let errors = registry.validate_inputs("database", &inputs).unwrap();
    assert!(errors.is_empty());

    // Get effective inputs (with defaults)
    let effective = registry.effective_inputs("database", &inputs).unwrap();
    assert_eq!(effective.get("db_name").unwrap().as_str(), Some("mydb"));
    assert_eq!(
        effective.get("environment").unwrap().as_str(),
        Some("development")
    );
}
