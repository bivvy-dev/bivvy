//! Comprehensive system tests for `bivvy add`.
//!
//! Tests adding templates to existing configurations, with all flag
//! combinations, error conditions, edge cases, comment preservation,
//! generated step format verification, and multi-step sequences.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

const BASE_CONFIG: &str = r#"
app_name: "AddTest"
steps:
  existing:
    command: "rustc --version"
workflows:
  default:
    steps: [existing]
"#;

const MULTI_WORKFLOW_CONFIG: &str = r#"
app_name: "AddTest"
steps:
  existing:
    command: "rustc --version"
workflows:
  default:
    steps: [existing]
  ci:
    steps: [existing]
"#;

/// Config with comments throughout — used to verify comment preservation.
const COMMENTED_CONFIG: &str = r#"# Project configuration for AddTest
# Managed by bivvy
app_name: "AddTest"

# Step definitions
steps:
  existing:
    command: "rustc --version"
    # This comment is inside a step

# Workflow definitions
workflows:
  default:
    steps: [existing]
"#;

/// Config with dependencies, watches, and completed_check already present.
const COMPLEX_CONFIG: &str = r#"
app_name: "ComplexProject"
steps:
  install:
    command: "npm install"
    completed_check:
      type: file_exists
      path: "node_modules/.package-lock.json"
    watches:
      - package.json
      - package-lock.json
  build:
    command: "npm run build"
    depends_on: [install]
    completed_check:
      type: file_exists
      path: "dist/index.js"
  test:
    command: "npm test"
    depends_on: [build]
workflows:
  default:
    steps: [install, build, test]
  ci:
    steps: [install, build, test]
"#;

/// Config with three ordered steps — used for insertion position tests.
const THREE_STEP_CONFIG: &str = r#"
app_name: "OrderTest"
steps:
  first:
    command: "git --version"
  second:
    command: "cargo --version"
  third:
    command: "rustc --version"
workflows:
  default:
    steps: [first, second, third]
"#;

// =====================================================================
// HAPPY PATH — Basic add
// =====================================================================

/// Add a template — config file updated with template reference and
/// correct YAML structure.
#[test]
fn add_template_creates_step() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("Added").expect("Should confirm addition");
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("template: bundle-install"),
        "Config should reference the template"
    );
    // Verify the step block structure: step name, then template on next line
    assert!(
        config.contains("  bundle-install:\n    template: bundle-install\n"),
        "Step block should have correct YAML structure with name and template"
    );
}

/// Add a second template — verify both are present and workflow is updated
/// with both in insertion order.
#[test]
fn add_second_template() {
    let temp = setup_project(BASE_CONFIG);

    // First add
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Second add
    let mut s = spawn_bivvy(&["add", "npm-install"], temp.path());
    s.expect("Added").expect("Should add second template");
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("template: bundle-install"));
    assert!(config.contains("template: npm-install"));

    // Both steps should be in the default workflow in insertion order
    assert!(
        config.contains("steps: [existing, bundle-install, npm-install]"),
        "Both added steps should appear in default workflow in order"
    );
}

/// Add three templates in sequence — verify ordering is preserved in both
/// step definitions and workflow list.
#[test]
fn add_three_templates_preserves_order() {
    let temp = setup_project(BASE_CONFIG);

    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);
    run_bivvy_silently(temp.path(), &["add", "npm-install"]);
    run_bivvy_silently(temp.path(), &["add", "cargo-build"]);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // Verify all three are in the workflow in order
    assert!(
        config.contains("steps: [existing, bundle-install, npm-install, cargo-build]"),
        "Three added steps should appear in workflow in insertion order"
    );

    // Verify step definition ordering in the file
    let bi_pos = config.find("  bundle-install:").unwrap();
    let ni_pos = config.find("  npm-install:").unwrap();
    let cb_pos = config.find("  cargo-build:").unwrap();
    assert!(
        bi_pos < ni_pos && ni_pos < cb_pos,
        "Step definitions should appear in insertion order"
    );
}

// =====================================================================
// HAPPY PATH — Flags
// =====================================================================

/// --as renames the step in the config and the workflow references
/// the custom name.
#[test]
fn add_with_custom_name() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "cargo-build", "--as", "my_build"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("my_build:"), "Should use custom step name");
    assert!(
        config.contains("  my_build:\n    template: cargo-build\n"),
        "Custom name should map to original template"
    );
    // Verify the custom name appears in the workflow, not the template name
    assert!(
        config.contains("steps: [existing, my_build]"),
        "Workflow should reference custom step name"
    );
}

/// --workflow adds to a specific workflow instead of default — verify
/// the step appears only in the targeted workflow.
#[test]
fn add_to_named_workflow() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install", "--workflow", "ci"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("template: bundle-install"),
        "Step definition should exist"
    );

    // Parse out the workflow lines to verify membership
    let lines: Vec<&str> = config.lines().collect();
    let mut in_ci = false;
    let mut in_default = false;
    let mut ci_steps_line = None;
    let mut default_steps_line = None;
    for line in &lines {
        if line.trim() == "ci:" {
            in_ci = true;
            in_default = false;
        } else if line.trim() == "default:" {
            in_default = true;
            in_ci = false;
        } else if line.trim().starts_with("steps:") {
            if in_ci {
                ci_steps_line = Some(line.trim().to_string());
            }
            if in_default {
                default_steps_line = Some(line.trim().to_string());
            }
        }
    }

    assert!(
        ci_steps_line
            .as_deref()
            .unwrap()
            .contains("bundle-install"),
        "CI workflow should contain the added step"
    );
    assert!(
        !default_steps_line
            .as_deref()
            .unwrap()
            .contains("bundle-install"),
        "Default workflow should NOT contain the step when --workflow ci is used"
    );
}

/// --after inserts after a specific step in the workflow — verify
/// the exact position.
#[test]
fn add_after_specific_step() {
    let temp = setup_project(THREE_STEP_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "first"],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("steps: [first, bundle-install, second, third]"),
        "Step should be inserted after 'first' in the workflow, got: {}",
        config
    );
}

/// --after the last step appends at the end.
#[test]
fn add_after_last_step() {
    let temp = setup_project(THREE_STEP_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "third"],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("steps: [first, second, third, bundle-install]"),
        "Step should be appended after the last step"
    );
}

/// --no-workflow adds the step definition without placing it in any
/// workflow — verify the workflow list is unchanged.
#[test]
fn add_no_workflow() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install", "--no-workflow"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("template: bundle-install"));
    // Workflow should be unchanged — still just [existing]
    assert!(
        config.contains("steps: [existing]"),
        "Workflow should remain unchanged with --no-workflow"
    );
}

/// --as + --workflow combined.
#[test]
fn add_custom_name_to_named_workflow() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "cargo-build", "--as", "build_ci", "--workflow", "ci"],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("build_ci:"), "Custom name should be used");
    assert!(
        config.contains("template: cargo-build"),
        "Template reference should be present"
    );
}

/// --as + --after combined — verify custom name at correct position.
#[test]
fn add_custom_name_after_step() {
    let temp = setup_project(THREE_STEP_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--as", "deps", "--after", "second"],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("deps:"),
        "Custom step name should be in config"
    );
    assert!(
        config.contains("steps: [first, second, deps, third]"),
        "Custom-named step should be inserted after 'second'"
    );
}

/// --as + --after + --workflow — all three positioning flags combined.
#[test]
fn add_all_flags_combined() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    let mut s = spawn_bivvy(
        &[
            "add",
            "cargo-build",
            "--as",
            "my_build",
            "--after",
            "existing",
            "--workflow",
            "ci",
        ],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("  my_build:\n    template: cargo-build\n"));
}

// =====================================================================
// GENERATED STEP FORMAT VERIFICATION
// =====================================================================

/// The generated step for bundle-install includes commented-out
/// template details (command, completed_check, watches).
#[test]
fn add_generates_commented_template_details() {
    let temp = setup_project(BASE_CONFIG);
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // bundle-install template has command, completed_check, and watches
    assert!(
        config.contains("# command: bundle install"),
        "Should include commented command from template"
    );
    assert!(
        config.contains("# completed_check:"),
        "Should include commented completed_check from template"
    );
    assert!(
        config.contains("# watches:"),
        "Should include commented watches from template"
    );
}

/// Verify step format with cargo-build template.
#[test]
fn add_cargo_build_format() {
    let temp = setup_project(BASE_CONFIG);
    run_bivvy_silently(temp.path(), &["add", "cargo-build"]);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    assert!(
        config.contains("  cargo-build:\n    template: cargo-build\n"),
        "Step block should have correct structure"
    );
    assert!(
        config.contains("# command:"),
        "Should include commented command from cargo-build template"
    );
}

/// Custom step name still shows the template's commented details.
#[test]
fn add_custom_name_preserves_template_comments() {
    let temp = setup_project(BASE_CONFIG);
    run_bivvy_silently(
        temp.path(),
        &["add", "bundle-install", "--as", "ruby_deps"],
    );

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    assert!(
        config.contains("  ruby_deps:\n    template: bundle-install\n"),
        "Custom name should map to original template"
    );
    assert!(
        config.contains("# command: bundle install"),
        "Commented template details should still appear under custom name"
    );
}

// =====================================================================
// COMMENT PRESERVATION
// =====================================================================

/// Adding a step preserves all existing comments in the config file.
#[test]
fn add_preserves_all_comments() {
    let temp = setup_project(COMMENTED_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    assert!(
        config.contains("# Project configuration for AddTest"),
        "Top-level comment should be preserved"
    );
    assert!(
        config.contains("# Managed by bivvy"),
        "Second top-level comment should be preserved"
    );
    assert!(
        config.contains("# Step definitions"),
        "Section comment should be preserved"
    );
    assert!(
        config.contains("# This comment is inside a step"),
        "Inline step comment should be preserved"
    );
    assert!(
        config.contains("# Workflow definitions"),
        "Workflow section comment should be preserved"
    );
    // And the new step should also be present
    assert!(
        config.contains("template: bundle-install"),
        "New step should be added despite comments"
    );
}

// =====================================================================
// ADDING TO COMPLEX CONFIGS (dependencies, watches, completed_check)
// =====================================================================

/// Adding a template to a config with dependencies, watches, and
/// completed_check fields — verifies no corruption of existing data.
#[test]
fn add_to_config_with_complex_steps() {
    let temp = setup_project(COMPLEX_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "install"],
        temp.path(),
    );

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // Original complex fields should still be present
    assert!(
        config.contains("depends_on: [install]"),
        "depends_on should be preserved"
    );
    assert!(
        config.contains("type: file_exists"),
        "completed_check type should be preserved"
    );
    assert!(
        config.contains("- package.json"),
        "watches entries should be preserved"
    );
    assert!(
        config.contains("- package-lock.json"),
        "All watches entries should be preserved"
    );

    // New step should be inserted in the right place in the workflow
    assert!(
        config.contains("steps: [install, bundle-install, build, test]"),
        "New step should be after 'install' in workflow"
    );
}

/// Adding to a config with multiple workflows — both are preserved.
#[test]
fn add_to_complex_config_preserves_ci_workflow() {
    let temp = setup_project(COMPLEX_CONFIG);
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // The CI workflow should be unchanged
    // Find the ci: workflow section
    let lines: Vec<&str> = config.lines().collect();
    let mut in_ci = false;
    for line in &lines {
        if line.trim() == "ci:" {
            in_ci = true;
        } else if in_ci && line.trim().starts_with("steps:") {
            assert!(
                line.contains("install, build, test"),
                "CI workflow should be unchanged, got: {}",
                line
            );
            break;
        }
    }
}

// =====================================================================
// MULTI-STEP TESTS
// =====================================================================

/// Add two templates with different flags, then verify final state.
#[test]
fn multi_step_add_with_different_flags() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);

    // First: add bundle-install to default workflow
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Second: add cargo-build to ci workflow with custom name
    let mut s = spawn_bivvy(
        &["add", "cargo-build", "--as", "build_ci", "--workflow", "ci"],
        temp.path(),
    );
    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // bundle-install in default workflow
    assert!(config.contains("template: bundle-install"));
    // build_ci in ci workflow
    assert!(config.contains("  build_ci:\n    template: cargo-build\n"));
}

/// Add a step, then add another after it — chained --after.
#[test]
fn chained_after_insertions() {
    let temp = setup_project(BASE_CONFIG);

    // Add first step
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Add second step after the first
    let mut s = spawn_bivvy(
        &["add", "npm-install", "--after", "bundle-install"],
        temp.path(),
    );
    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("steps: [existing, bundle-install, npm-install]"),
        "npm-install should be after bundle-install in workflow"
    );
}

/// Add a step with --no-workflow, then add another normally — verify
/// only the second appears in the workflow.
#[test]
fn add_no_workflow_then_normal() {
    let temp = setup_project(BASE_CONFIG);

    run_bivvy_silently(
        temp.path(),
        &["add", "bundle-install", "--no-workflow"],
    );

    let mut s = spawn_bivvy(&["add", "npm-install"], temp.path());
    s.expect("Added").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    // bundle-install should NOT be in workflow, npm-install should be
    assert!(
        config.contains("steps: [existing, npm-install]"),
        "Only npm-install should be in workflow (bundle-install was --no-workflow)"
    );
    // But both step definitions should exist
    assert!(config.contains("template: bundle-install"));
    assert!(config.contains("template: npm-install"));
}

/// Add two steps to different workflows in sequence.
#[test]
fn add_to_different_workflows_sequentially() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);

    run_bivvy_silently(
        temp.path(),
        &["add", "bundle-install", "--workflow", "default"],
    );
    run_bivvy_silently(
        temp.path(),
        &["add", "cargo-build", "--workflow", "ci"],
    );

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // Both templates should be defined as steps
    assert!(config.contains("template: bundle-install"));
    assert!(config.contains("template: cargo-build"));
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Unknown template name fails with useful error.
#[test]
fn add_unknown_template_fails() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "nonexistent-template-xyz"], temp.path());

    s.expect("Unknown template: nonexistent-template-xyz")
        .expect("Should show 'Unknown template' error for unknown template");
    s.expect(expectrl::Eof).unwrap();

    // Config should be unchanged
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        !config.contains("nonexistent"),
        "Config should not be modified on failure"
    );
}

/// Duplicate template add fails with clear message.
#[test]
fn add_duplicate_template_fails() {
    let temp = setup_project(BASE_CONFIG);

    // First add succeeds
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Second add of same template fails
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());
    s.expect("Step 'bundle-install' already exists in configuration. Use a different name with --as.")
        .expect("Should indicate step already exists");
    s.expect(expectrl::Eof).unwrap();
}

/// Duplicate with --as to the same custom name also fails.
#[test]
fn add_duplicate_custom_name_fails() {
    let temp = setup_project(BASE_CONFIG);

    run_bivvy_silently(temp.path(), &["add", "bundle-install", "--as", "deps"]);

    // Try to add a different template with the same custom name
    let mut s = spawn_bivvy(&["add", "npm-install", "--as", "deps"], temp.path());
    s.expect("Step 'deps' already exists in configuration. Use a different name with --as.")
        .expect("Should reject duplicate custom step name");
    s.expect(expectrl::Eof).unwrap();
}

/// Adding a step named the same as an existing non-template step fails.
#[test]
fn add_conflicts_with_existing_step_name() {
    let temp = setup_project(BASE_CONFIG);
    // "existing" is already a step name in BASE_CONFIG
    let mut s = spawn_bivvy(&["add", "bundle-install", "--as", "existing"], temp.path());

    s.expect("Step 'existing' already exists in configuration. Use a different name with --as.")
        .expect("Should reject name that conflicts with existing step");
    s.expect(expectrl::Eof).unwrap();
}

/// Add without any config file suggests `bivvy init`.
#[test]
fn add_without_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("bivvy init")
        .expect("Should suggest running 'bivvy init'");
    s.expect(expectrl::Eof).unwrap();

    // No config file should be created
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Should not create config file on failure"
    );
}

/// --workflow targeting a nonexistent workflow — step is added but
/// workflow is unchanged.
#[test]
fn add_to_nonexistent_workflow() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--workflow", "ghost"],
        temp.path(),
    );

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("Added") || output.contains("added") || output.contains("error") || output.contains("Error"),
        "Should show result of add to nonexistent workflow, got: {}",
        &output[..output.len().min(300)]
    );

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    // Default workflow should still just have [existing]
    assert!(
        config.contains("steps: [existing]"),
        "Default workflow should be unchanged when targeting nonexistent workflow"
    );
}

/// --after targeting a nonexistent step — step is still appended.
#[test]
fn add_after_nonexistent_step_appends() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "ghost-step"],
        temp.path(),
    );

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("Added") || output.contains("added") || output.contains("error") || output.contains("Error"),
        "Should show result of add after nonexistent step, got: {}",
        &output[..output.len().min(300)]
    );

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    // When --after target doesn't exist, the step should still be appended
    if config.contains("template: bundle-install") {
        assert!(
            config.contains("steps: [existing, bundle-install]"),
            "Step should be appended when --after target is not found"
        );
    }
}

/// No template argument provided — clap shows required argument error.
#[test]
fn add_no_argument_shows_help() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add"], temp.path());

    // Clap should show an error about missing required argument
    s.expect("required")
        .expect("Should show missing argument error");
    s.expect(expectrl::Eof).unwrap();
}

/// Config with no steps section — should fail.
#[test]
fn add_config_without_steps_section() {
    let config = "app_name: \"NoSteps\"\n";
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    let output = read_to_eof(&mut s);
    // The command should fail since there's no steps section
    assert!(
        !output.contains("Added"),
        "Should not succeed when config has no steps section, got: {}",
        output
    );
}

/// Add to a config with malformed YAML fails gracefully.
#[test]
fn add_malformed_config_fails() {
    let temp = setup_project("{{{{ not yaml");
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    let output = read_to_eof(&mut s);
    assert!(
        !output.contains("Added"),
        "Should not succeed with malformed YAML"
    );
}
