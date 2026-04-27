//! Comprehensive system tests for `bivvy add`.
//!
//! Tests adding templates to existing configurations, with all flag
//! combinations, error conditions, edge cases, comment preservation,
//! generated step format verification, and multi-step sequences.
#![cfg(unix)]

mod system;

use std::fs;
use std::process::Command;
use system::helpers::*;

/// Normalize a config file to a stable representation for snapshotting.
///
/// Strips trailing whitespace from each line and ensures a single trailing
/// newline, so snapshots are resilient to incidental whitespace churn.
fn normalize_config(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

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

/// Config with dependencies and check already present.
const COMPLEX_CONFIG: &str = r#"
app_name: "ComplexProject"
steps:
  install:
    command: "npm install"
    check:
      type: presence
      target: "node_modules/.package-lock.json"
  build:
    command: "npm run build"
    depends_on: [install]
    check:
      type: presence
      target: "dist/index.js"
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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    // Snapshot the full resulting YAML to catch regressions in the
    // generated step block format, workflow list, and structural layout.
    insta::assert_snapshot!("add_template_creates_step_config", normalize_config(&config));
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
    s.expect("Added 'npm-install' step using template 'npm-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'my_build' step using template 'cargo-build'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'ci' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    // Ensure the "Added to 'default' workflow" message is NOT emitted when
    // --no-workflow is used: read to EOF and check the full transcript.
    let tail = read_to_eof(&mut s);
    assert!(
        !tail.contains("Added to 'default' workflow"),
        "Should not print workflow-addition message with --no-workflow, got: {tail}"
    );
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("template: bundle-install"));
    // Workflow should be unchanged — still just [existing]
    assert!(
        config.contains("steps: [existing]"),
        "Workflow should remain unchanged with --no-workflow"
    );
    insta::assert_snapshot!("add_no_workflow_config", normalize_config(&config));
}

/// --as + --workflow combined.
#[test]
fn add_custom_name_to_named_workflow() {
    let temp = setup_project(MULTI_WORKFLOW_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "cargo-build", "--as", "build_ci", "--workflow", "ci"],
        temp.path(),
    );

    s.expect("Added 'build_ci' step using template 'cargo-build'")
        .unwrap();
    s.expect("Added to 'ci' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'deps' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

    s.expect("Added 'my_build' step using template 'cargo-build'")
        .unwrap();
    s.expect("Added to 'ci' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(config.contains("  my_build:\n    template: cargo-build\n"));
    insta::assert_snapshot!("add_all_flags_combined_config", normalize_config(&config));
}

// =====================================================================
// GENERATED STEP FORMAT VERIFICATION
// =====================================================================

/// The generated step for bundle-install includes commented-out
/// template details (command, check).
#[test]
fn add_generates_commented_template_details() {
    let temp = setup_project(BASE_CONFIG);
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // bundle-install template has command and check
    assert!(
        config.contains("# command: bundle install"),
        "Should include commented command from template"
    );
    assert!(
        config.contains("# check:"),
        "Should include commented check from template"
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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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
// ADDING TO COMPLEX CONFIGS (dependencies, check)
// =====================================================================

/// Adding a template to a config with dependencies and
/// check fields — verifies no corruption of existing data.
#[test]
fn add_to_config_with_complex_steps() {
    let temp = setup_project(COMPLEX_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "install"],
        temp.path(),
    );

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // Original complex fields should still be present
    assert!(
        config.contains("depends_on: [install]"),
        "depends_on should be preserved"
    );
    assert!(
        config.contains("type: presence"),
        "check type should be preserved"
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

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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
    s.expect("Added 'build_ci' step using template 'cargo-build'")
        .unwrap();
    s.expect("Added to 'ci' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // bundle-install in default workflow
    assert!(config.contains("template: bundle-install"));
    // build_ci in ci workflow
    assert!(config.contains("  build_ci:\n    template: cargo-build\n"));
    // bundle-install must be in the default workflow (from the first add).
    assert!(
        config.contains("default:\n    steps: [existing, bundle-install]"),
        "bundle-install should remain in the default workflow"
    );
    insta::assert_snapshot!(
        "multi_step_add_with_different_flags_config",
        normalize_config(&config)
    );
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
    s.expect("Added 'npm-install' step using template 'npm-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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
    s.expect("Added 'npm-install' step using template 'npm-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

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

/// Add two steps to different workflows in sequence — verify each
/// workflow contains exactly the step that was added to it.
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

    // The default workflow should contain bundle-install but not cargo-build.
    assert!(
        config.contains("default:\n    steps: [existing, bundle-install]"),
        "Default workflow should contain bundle-install only, got: {config}"
    );
    // The ci workflow should contain cargo-build but not bundle-install.
    assert!(
        config.contains("ci:\n    steps: [existing, cargo-build]"),
        "CI workflow should contain cargo-build only, got: {config}"
    );
    insta::assert_snapshot!(
        "add_to_different_workflows_sequentially_config",
        normalize_config(&config)
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Unknown template name fails with useful error.
#[test]
fn add_unknown_template_fails() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(&["add", "nonexistent-template-xyz"], temp.path());

    s.expect("Error: Unknown template: nonexistent-template-xyz")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 1);

    // Config should be unchanged
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        !config.contains("nonexistent"),
        "Config should not be modified on failure"
    );
    assert_eq!(
        config.trim(),
        BASE_CONFIG.trim(),
        "Config should be byte-identical to the original on failure"
    );
}

/// Duplicate template add fails with clear message.
#[test]
fn add_duplicate_template_fails() {
    let temp = setup_project(BASE_CONFIG);

    // First add succeeds
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Capture the config state after the first successful add.
    let config_after_first =
        fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    // Second add of same template fails
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());
    s.expect("Step 'bundle-install' already exists in configuration. Use a different name with --as.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 1);

    // Config should be byte-identical — failed add must not mutate file.
    let config_after_second =
        fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(
        config_after_first, config_after_second,
        "Failed duplicate add should not modify the config file"
    );
}

/// Duplicate with --as to the same custom name also fails.
#[test]
fn add_duplicate_custom_name_fails() {
    let temp = setup_project(BASE_CONFIG);

    run_bivvy_silently(temp.path(), &["add", "bundle-install", "--as", "deps"]);

    // Try to add a different template with the same custom name
    let mut s = spawn_bivvy(&["add", "npm-install", "--as", "deps"], temp.path());
    s.expect("Step 'deps' already exists in configuration. Use a different name with --as.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 1);

    // The second add must not have introduced npm-install into the config.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        !config.contains("template: npm-install"),
        "Failed add should not introduce a new template reference, got: {config}"
    );
}

/// Adding a step named the same as an existing non-template step fails.
#[test]
fn add_conflicts_with_existing_step_name() {
    let temp = setup_project(BASE_CONFIG);
    // "existing" is already a step name in BASE_CONFIG
    let mut s = spawn_bivvy(&["add", "bundle-install", "--as", "existing"], temp.path());

    s.expect("Step 'existing' already exists in configuration. Use a different name with --as.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 1);

    // Config should be unchanged on failure.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(
        config.trim(),
        BASE_CONFIG.trim(),
        "Conflicting --as should not modify the config file"
    );
}

/// Add without any config file suggests `bivvy init`.
#[test]
fn add_without_config_fails() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["add", "bundle-install"], temp.path());

    s.expect("No configuration found. Run 'bivvy init' first.")
        .unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 2);

    // No config file should be created
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Should not create config file on failure"
    );
    // The .bivvy directory itself should not exist either.
    assert!(
        !temp.path().join(".bivvy").exists(),
        "Should not create .bivvy directory on failure"
    );
}

/// --workflow targeting a nonexistent workflow — the step definition is
/// still added and the command succeeds (workflow silently not modified).
#[test]
fn add_to_nonexistent_workflow() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--workflow", "ghost"],
        temp.path(),
    );

    // The step definition is added and the success message is printed.
    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    // The workflow line is printed even though the workflow does not exist.
    s.expect("Added to 'ghost' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    // Default workflow should still just have [existing]
    assert!(
        config.contains("steps: [existing]"),
        "Default workflow should be unchanged when targeting nonexistent workflow"
    );
    // The step definition should still be added even if the workflow doesn't exist
    assert!(
        config.contains("template: bundle-install"),
        "Step definition should be present even when target workflow doesn't exist"
    );
    // No "ghost:" workflow should have been created.
    assert!(
        !config.contains("ghost:"),
        "Nonexistent workflow should not be created"
    );
    insta::assert_snapshot!(
        "add_to_nonexistent_workflow_config",
        normalize_config(&config)
    );
}

/// --after targeting a nonexistent step — the command succeeds and the
/// step is appended at the end of the workflow (documented fallback).
#[test]
fn add_after_nonexistent_step_appends() {
    let temp = setup_project(BASE_CONFIG);
    let mut s = spawn_bivvy(
        &["add", "bundle-install", "--after", "ghost-step"],
        temp.path(),
    );

    s.expect("Added 'bundle-install' step using template 'bundle-install'")
        .unwrap();
    s.expect("Added to 'default' workflow").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    // The step definition must be present.
    assert!(
        config.contains("template: bundle-install"),
        "Step definition should be added even when --after target doesn't exist"
    );
    // When --after target doesn't exist, the step should be appended at the end.
    assert!(
        config.contains("steps: [existing, bundle-install]"),
        "Step should be appended when --after target is not found"
    );
    insta::assert_snapshot!(
        "add_after_nonexistent_step_config",
        normalize_config(&config)
    );
}

/// No template argument provided — clap shows required argument error
/// and the process exits with clap's usage-error exit code (2).
#[test]
fn add_no_argument_shows_help() {
    let temp = setup_project(BASE_CONFIG);

    // Use a subprocess so we can reliably capture stderr + exit code.
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .arg("add")
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: the following required arguments were not provided"),
        "Clap should print the full 'required arguments were not provided' error on stderr, got: {stderr}"
    );
    // Clap's error message names the missing argument.
    assert!(
        stderr.contains("<TEMPLATE>"),
        "Clap error should name the missing TEMPLATE argument, got: {stderr}"
    );
    // Clap exits with code 2 on usage errors.
    assert_eq!(
        output.status.code(),
        Some(2),
        "Missing required arg should exit with clap's usage-error code 2"
    );

    // Config must not have been modified.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(config.trim(), BASE_CONFIG.trim());
}

/// Config with no steps section — should fail with a specific error and
/// exit code 1. The config file must not be modified.
#[test]
fn add_config_without_steps_section() {
    let config = "app_name: \"NoSteps\"\n";
    let temp = setup_project(config);

    // Use a subprocess for reliable stderr + exit code capture.
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("Added 'bundle-install'"),
        "Should not succeed when config has no steps section, got: {combined}"
    );
    assert!(
        combined.contains("Invalid configuration: No 'steps:' section found in config file"),
        "Should show the specific 'no steps section' error, got: {combined}"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Config without steps section should exit with code 1"
    );

    // Config file should be unchanged.
    let after = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(
        after, config,
        "Config file should not be modified on failure"
    );
}

/// Add to a config with malformed YAML fails gracefully with a specific
/// parse-error message and exit code 1. The malformed file is not modified.
#[test]
fn add_malformed_config_fails() {
    let malformed = "{{{{ not yaml";
    let temp = setup_project(malformed);

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("Added 'bundle-install'"),
        "Should not succeed with malformed YAML, got: {combined}"
    );
    assert!(
        combined.contains("Failed to parse config at"),
        "Should show the specific parse-error prefix for malformed YAML, got: {combined}"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "Malformed config should exit with code 1"
    );

    // The malformed file should not have been overwritten.
    let after = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(after, malformed, "Malformed config should not be modified");
}

// =====================================================================
// EXIT CODE VERIFICATION
// =====================================================================

/// Successful `bivvy add` exits with code 0 and prints the success message
/// and the `Added to 'default' workflow` line on stdout.
#[test]
fn add_success_exit_code_zero() {
    let temp = setup_project(BASE_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(0),
        "Successful add should exit with code 0"
    );

    // The success path writes to stdout — verify content, not just exit code.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Added 'bundle-install' step using template 'bundle-install'"),
        "Should print success message on stdout, got: {combined}"
    );
    assert!(
        combined.contains("Added to 'default' workflow"),
        "Should print workflow-addition line on stdout, got: {combined}"
    );
    // And the config file should actually have been modified.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("template: bundle-install"),
        "Config file should be updated on success"
    );
}

/// `bivvy add` with no config exits with code 2 and prints the
/// `Run 'bivvy init' first.` hint on stderr.
#[test]
fn add_no_config_exit_code_two() {
    let temp = tempfile::TempDir::new().unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(2),
        "Add with no config should exit with code 2"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("No configuration found. Run 'bivvy init' first."),
        "Should print init hint on failure, got: {combined}"
    );
    // No config directory should be created as a side effect.
    assert!(
        !temp.path().join(".bivvy").exists(),
        "No .bivvy directory should be created on failure"
    );
}

/// `bivvy add` with duplicate step exits with code 1 and prints the
/// `already exists` message.
#[test]
fn add_duplicate_step_exit_code_one() {
    let temp = setup_project(BASE_CONFIG);
    run_bivvy_silently(temp.path(), &["add", "bundle-install"]);

    // Snapshot the config after the first add so we can confirm the second
    // add does not modify it.
    let before = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();

    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Add with duplicate step should exit with code 1"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains(
            "Step 'bundle-install' already exists in configuration. Use a different name with --as."
        ),
        "Should print the full duplicate-step error message, got: {combined}"
    );

    // Config must not have been mutated by the failed second add.
    let after = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(
        before, after,
        "Failed duplicate add must not modify the config file"
    );
}

/// `bivvy add` with unknown template exits with code 1 and prints the
/// `Unknown template` error.
#[test]
fn add_unknown_template_exit_code_one() {
    let temp = setup_project(BASE_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "nonexistent-template-xyz"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");
    assert_eq!(
        output.status.code(),
        Some(1),
        "Add with unknown template should exit with code 1"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Unknown template: nonexistent-template-xyz"),
        "Should print the full unknown-template error message, got: {combined}"
    );

    // Config must not have been modified by the failed add.
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert_eq!(
        config.trim(),
        BASE_CONFIG.trim(),
        "Failed unknown-template add must not modify the config file"
    );
}

/// A successful `bivvy add` emits the documented `after_add` hint that
/// points the user at `bivvy run --only=<step>`.  This is a documented,
/// user-facing behavior so it must be exercised by a system test.
#[test]
fn add_emits_after_add_hint() {
    let temp = setup_project(BASE_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    assert_eq!(
        output.status.code(),
        Some(0),
        "Successful add should exit with code 0"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    // The hint references the new step by name and points at `bivvy run --only=`.
    assert!(
        combined.contains("bivvy run --only=bundle-install"),
        "Should emit the after_add hint pointing at `bivvy run --only=bundle-install`, got: {combined}"
    );
    assert!(
        combined.contains("bivvy list"),
        "after_add hint should also reference `bivvy list`, got: {combined}"
    );
}

/// A successful `bivvy add --as NAME` emits the `after_add` hint using the
/// custom step name, not the original template name.
#[test]
fn add_emits_after_add_hint_with_custom_name() {
    let temp = setup_project(BASE_CONFIG);
    let bin = assert_cmd::cargo::cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(["add", "bundle-install", "--as", "ruby_deps"])
        .current_dir(temp.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to run bivvy");

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("bivvy run --only=ruby_deps"),
        "after_add hint should use the custom step name, got: {combined}"
    );
    // Should not reference the raw template name in the hint.
    assert!(
        !combined.contains("bivvy run --only=bundle-install"),
        "after_add hint should not reference the template name when --as is used, got: {combined}"
    );
}
