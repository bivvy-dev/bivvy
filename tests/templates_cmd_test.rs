//! Integration tests for the template system via CLI and registry API.
//!
//! Tests template listing, category filtering, project-level overrides,
//! and template resolution through CLI commands.
// The cargo_bin function is marked deprecated in favor of cargo_bin! macro,
// but both work correctly. Suppressing until assert_cmd stabilizes the new API.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use bivvy::registry::{Registry, TemplateSource};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    temp
}

// --- Registry: listing available templates ---

#[test]
fn registry_lists_all_builtin_templates() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let names = registry.all_template_names();

    // Should include well-known builtins
    assert!(names.contains(&"brew-bundle".to_string()));
    assert!(names.contains(&"bundle-install".to_string()));
    assert!(names.contains(&"npm-install".to_string()));
    assert!(names.contains(&"cargo-build".to_string()));

    // Names should be sorted
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);

    Ok(())
}

#[test]
fn registry_all_templates_have_required_fields() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let names = registry.all_template_names();

    for name in &names {
        let template = registry.get(name).unwrap_or_else(|| {
            panic!("Template '{}' listed but not resolvable", name);
        });
        assert!(!template.name.is_empty(), "Template has empty name");
        assert!(
            !template.description.is_empty(),
            "Template '{}' has empty description",
            name
        );
        assert!(
            !template.category.is_empty(),
            "Template '{}' has empty category",
            name
        );
    }

    Ok(())
}

// --- Category filtering ---

#[test]
fn registry_templates_have_valid_categories() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let names = registry.all_template_names();

    let mut categories: Vec<String> = names
        .iter()
        .filter_map(|name| registry.get(name))
        .map(|t| t.category.clone())
        .collect();
    categories.sort();
    categories.dedup();

    // Should have multiple categories
    assert!(
        categories.len() > 1,
        "Expected multiple categories, got: {:?}",
        categories
    );

    Ok(())
}

#[test]
fn registry_filter_by_category() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let names = registry.all_template_names();

    // Get the category of "brew" template
    let brew_bundle = registry.get("brew-bundle").unwrap();
    let brew_category = &brew_bundle.category;

    // Filter to that category
    let filtered: Vec<&str> = names
        .iter()
        .filter_map(|name| registry.get(name))
        .filter(|t| &t.category == brew_category)
        .map(|t| t.name.as_str())
        .collect();

    assert!(
        filtered.contains(&"brew-bundle"),
        "brew-bundle should be in its own category"
    );

    // Filtered list should be shorter than full list
    assert!(
        filtered.len() <= names.len(),
        "Filtered list should not be larger than full list"
    );

    Ok(())
}

#[test]
fn registry_filter_nonexistent_category_returns_empty() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let names = registry.all_template_names();

    let filtered: Vec<&str> = names
        .iter()
        .filter_map(|name| registry.get(name))
        .filter(|t| t.category == "nonexistent-category-xyz")
        .map(|t| t.name.as_str())
        .collect();

    assert!(
        filtered.is_empty(),
        "Nonexistent category should yield empty results"
    );

    Ok(())
}

// --- Template details/info ---

#[test]
fn registry_template_info_brew() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let (template, source) = registry.resolve("brew-bundle")?;

    assert_eq!(template.name, "brew-bundle");
    assert_eq!(source, TemplateSource::Builtin);
    assert!(!template.description.is_empty());
    assert!(!template.category.is_empty());

    // brew should have a command defined
    assert!(
        template.step.command.is_some(),
        "brew template should define a command"
    );

    Ok(())
}

#[test]
fn registry_template_info_unknown_fails() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let result = registry.resolve("completely-nonexistent-template");

    assert!(result.is_err());

    Ok(())
}

// --- Project-level template overrides ---

#[test]
fn project_override_shadows_builtin() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;

    // Create local template that shadows "brew-bundle"
    let local_brew = r#"
name: brew-bundle
description: "Custom project brew"
category: custom
step:
  title: "Custom Brew"
  command: "echo custom brew"
"#;
    fs::write(templates_dir.join("brew-bundle.yml"), local_brew)?;

    let registry = Registry::new(Some(temp.path()))?;
    let (template, source) = registry.resolve("brew-bundle")?;

    assert_eq!(template.description, "Custom project brew");
    assert_eq!(source, TemplateSource::Project);

    Ok(())
}

#[test]
fn project_override_adds_new_template() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;

    let custom = r#"
name: my-custom-tool
description: "A project-specific tool"
category: project
step:
  command: "echo my tool"
"#;
    fs::write(templates_dir.join("my-custom-tool.yml"), custom)?;

    let registry = Registry::new(Some(temp.path()))?;

    // New template should be resolvable
    let (template, source) = registry.resolve("my-custom-tool")?;
    assert_eq!(template.name, "my-custom-tool");
    assert_eq!(source, TemplateSource::Project);

    // Should appear in all template names
    let names = registry.all_template_names();
    assert!(names.contains(&"my-custom-tool".to_string()));

    Ok(())
}

#[test]
fn project_override_visible_in_all_names() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;

    let custom = r#"
name: unique-project-template
description: "Unique project template"
category: project
step:
  command: "echo unique"
"#;
    fs::write(templates_dir.join("unique-project-template.yml"), custom)?;

    let registry = Registry::new(Some(temp.path()))?;
    let names = registry.all_template_names();

    assert!(names.contains(&"unique-project-template".to_string()));
    // Builtins should still be present
    assert!(names.contains(&"brew-bundle".to_string()));

    Ok(())
}

// --- CLI: template usage through run command ---

#[test]
fn cli_run_with_template_step() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: TemplateTest
steps:
  deps:
    template: brew-bundle
workflows:
  default:
    steps: [deps]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run", "--dry-run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry-run mode"));
    Ok(())
}

#[test]
fn cli_run_with_project_template_override() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;

    // Create config that references a project-local template
    let config = r#"
app_name: OverrideTest
steps:
  setup:
    template: custom-setup
workflows:
  default:
    steps: [setup]
"#;
    fs::write(bivvy_dir.join("config.yml"), config)?;

    // Create the project-local template
    let templates_dir = bivvy_dir.join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;
    let custom = r#"
name: custom-setup
description: "Custom setup step"
category: project
step:
  command: "echo custom setup running"
"#;
    fs::write(templates_dir.join("custom-setup.yml"), custom)?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("custom setup running"));
    Ok(())
}

#[test]
fn cli_lint_with_valid_template_reference() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: LintTemplateTest
steps:
  deps:
    template: brew-bundle
workflows:
  default:
    steps: [deps]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Configuration is valid!"));
    Ok(())
}

#[test]
fn cli_lint_with_unknown_template_fails() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: LintBadTemplate
steps:
  deps:
    template: nonexistent-template-xyz
workflows:
  default:
    steps: [deps]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("lint");
    cmd.assert().failure();
    Ok(())
}

#[test]
fn cli_list_shows_template_steps() -> Result<(), Box<dyn std::error::Error>> {
    let config = r#"
app_name: ListTemplateTest
steps:
  deps:
    template: brew-bundle
  build:
    command: echo build
workflows:
  default:
    steps: [deps, build]
"#;
    let temp = setup_project(config);
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("deps"))
        .stdout(predicate::str::contains("build"));
    Ok(())
}

// --- CLI: init --minimal produces valid config ---

#[test]
fn cli_init_minimal_produces_config() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    // Verify config file was created with expected structure
    let config = std::fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(config.contains("app_name:"));
    assert!(config.contains("steps:"));
    assert!(config.contains("workflows:"));

    Ok(())
}

// --- Multiple project templates ---

#[test]
fn project_multiple_local_templates() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;

    let tool_a = r#"
name: tool-a
description: "Tool A"
category: tools
step:
  command: "echo tool-a"
"#;
    let tool_b = r#"
name: tool-b
description: "Tool B"
category: tools
step:
  command: "echo tool-b"
"#;
    fs::write(templates_dir.join("tool-a.yml"), tool_a)?;
    fs::write(templates_dir.join("tool-b.yml"), tool_b)?;

    let registry = Registry::new(Some(temp.path()))?;
    let names = registry.all_template_names();

    assert!(names.contains(&"tool-a".to_string()));
    assert!(names.contains(&"tool-b".to_string()));

    // Both resolve as Project source
    let (_, source_a) = registry.resolve("tool-a")?;
    let (_, source_b) = registry.resolve("tool-b")?;
    assert_eq!(source_a, TemplateSource::Project);
    assert_eq!(source_b, TemplateSource::Project);

    Ok(())
}

#[test]
fn project_template_category_filtering_with_local() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;

    let custom = r#"
name: custom-lint
description: "Custom linter"
category: quality
step:
  command: "echo lint"
"#;
    fs::write(templates_dir.join("custom-lint.yml"), custom)?;

    let registry = Registry::new(Some(temp.path()))?;
    let names = registry.all_template_names();

    // Filter to the "quality" category
    let filtered: Vec<&str> = names
        .iter()
        .filter_map(|name| registry.get(name))
        .filter(|t| t.category == "quality")
        .map(|t| t.name.as_str())
        .collect();

    assert!(
        filtered.contains(&"custom-lint"),
        "custom-lint should be in quality category"
    );

    Ok(())
}

// --- Edge cases ---

#[test]
fn registry_no_project_root_only_builtins() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::new(None)?;
    let names = registry.all_template_names();

    // Should have builtins
    assert!(!names.is_empty());

    // All should resolve as Builtin
    for name in &names {
        let (_, source) = registry.resolve(name)?;
        assert_eq!(
            source,
            TemplateSource::Builtin,
            "Template '{}' should be Builtin with no project root",
            name
        );
    }

    Ok(())
}

#[test]
fn registry_empty_project_templates_dir() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let templates_dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;

    // Empty templates dir - should still load builtins
    let registry = Registry::new(Some(temp.path()))?;
    let names = registry.all_template_names();
    assert!(!names.is_empty());
    assert!(names.contains(&"brew-bundle".to_string()));

    Ok(())
}

#[test]
fn cli_run_project_template_overrides_builtin() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;

    // Create config referencing "brew-bundle" (builtin name)
    let config = r#"
app_name: OverrideBuiltinTest
steps:
  deps:
    template: brew-bundle
workflows:
  default:
    steps: [deps]
"#;
    fs::write(bivvy_dir.join("config.yml"), config)?;

    // Create project-local brew-bundle that overrides the builtin
    let templates_dir = bivvy_dir.join("templates").join("steps");
    fs::create_dir_all(&templates_dir)?;
    let custom_brew = r#"
name: brew-bundle
description: "Project-specific brew"
category: custom
step:
  command: "echo project-brew-override"
"#;
    fs::write(templates_dir.join("brew-bundle.yml"), custom_brew)?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["run"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("project-brew-override"));

    Ok(())
}
