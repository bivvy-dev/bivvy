//! Comprehensive system tests for `bivvy templates`.
//!
//! Tests listing available templates, category filtering, output format,
//! template descriptions, custom project-local templates, and all
//! template categories from the registry.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;

// =====================================================================
// HAPPY PATH
// =====================================================================

/// Lists all built-in templates with counts.
#[test]
fn templates_lists_all() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(text.contains("cargo-build"), "Should list cargo-build template, got: {}", &text[..text.len().min(500)]);
    assert!(text.contains("bundle-install"), "Should list bundle-install template, got: {}", &text[..text.len().min(500)]);
    assert!(text.contains("npm-install"), "Should list npm-install template, got: {}", &text[..text.len().min(500)]);
    assert!(text.contains("templates available"), "Should show template count line, got: {}", &text[..text.len().min(500)]);

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates command should exit 0");
}

/// Templates output shows the "Available Templates" header.
#[test]
fn templates_shows_header() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Available Templates"),
        "Should show 'Available Templates' header, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates command should exit 0");
}

/// Templates output shows the hint to use `bivvy add`.
#[test]
fn templates_shows_add_hint() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy add"),
        "Should show 'bivvy add' hint, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates command should exit 0");
}

// =====================================================================
// FLAGS -- Category filtering
// =====================================================================

/// --category rust shows Rust templates like cargo-build.
#[test]
fn templates_category_rust() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "rust"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build"),
        "Should show cargo-build for --category rust, got: {}",
        &text[..text.len().min(500)]
    );
    // Should NOT show templates from other categories
    assert!(
        !text.contains("bundle-install"),
        "Should not show Ruby templates when filtering by rust"
    );
    assert!(
        !text.contains("npm-install"),
        "Should not show Node templates when filtering by rust"
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category rust should exit 0");
}

/// --category node shows Node templates like npm-install and yarn-install.
#[test]
fn templates_category_node() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "node"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("npm-install"),
        "Should show npm-install for --category node, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("yarn-install"),
        "Should show yarn-install for --category node, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category node should exit 0");
}

/// --category ruby shows Ruby templates like bundle-install.
#[test]
fn templates_category_ruby() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "ruby"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bundle-install"),
        "Should show bundle-install for --category ruby, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category ruby should exit 0");
}

/// --category python shows Python templates like pip-install.
#[test]
fn templates_category_python() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "python"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("pip-install"),
        "Should show pip-install for --category python, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category python should exit 0");
}

/// --category go shows Go templates like go-mod-download.
#[test]
fn templates_category_go() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "go"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("go-mod-download"),
        "Should show go-mod-download for --category go, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category go should exit 0");
}

/// --category system shows system templates like brew-bundle.
#[test]
fn templates_category_system() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "system"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("brew-bundle"),
        "Should show brew-bundle for --category system, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category system should exit 0");
}

/// --category java shows Java templates like maven-resolve.
#[test]
fn templates_category_java() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "java"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("maven-resolve"),
        "Should show maven-resolve for --category java, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category java should exit 0");
}

/// --category containers shows container templates like docker-compose-up.
#[test]
fn templates_category_containers() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "containers"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("docker-compose-up"),
        "Should show docker-compose-up for --category containers, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category containers should exit 0");
}

/// --category common shows common templates like env-copy and pre-commit-install.
#[test]
fn templates_category_common() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "common"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("env-copy"),
        "Should show env-copy for --category common, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("pre-commit-install"),
        "Should show pre-commit-install for --category common, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category common should exit 0");
}

/// --verbose shows extra information per template.
#[test]
fn templates_verbose_flag() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--verbose"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build"),
        "Verbose should still show template names, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Available Templates"),
        "Verbose should show header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("templates available"),
        "Verbose should show count, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --verbose should exit 0");
}

// =====================================================================
// CUSTOM PROJECT-LOCAL TEMPLATES
// =====================================================================

/// Custom project-local templates appear in the template list.
#[test]
fn templates_project_local_custom() {
    let config = r#"app_name: CustomTplTest
steps:
  build:
    command: "cargo --version"
workflows:
  default:
    steps: [build]
"#;
    let temp = setup_project(config);

    // Create a custom template in the project-local templates directory
    let tpl_dir = temp.path().join(".bivvy/templates/steps");
    fs::create_dir_all(&tpl_dir).unwrap();
    fs::write(
        tpl_dir.join("my-custom-step.yml"),
        r#"name: my-custom-step
description: "Custom project setup step"
category: custom
version: "1.0.0"
platforms: [macos, linux, windows]
step:
  command: "cargo --version"
  completed_check:
    type: command_succeeds
    command: "cargo --version"
"#,
    )
    .unwrap();

    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("my-custom-step"),
        "Should show custom project-local template 'my-custom-step', got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates with custom template should exit 0");
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description for the templates subcommand.
#[test]
fn templates_help() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--help"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("template") || text.contains("Template"),
        "Help should describe the templates command, got: {}",
        &text[..text.len().min(500)]
    );
    // Help output should include usage information
    assert!(
        text.contains("Usage") || text.contains("usage") || text.contains("USAGE"),
        "Help should include usage info, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --help should exit 0");
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Nonexistent category shows zero templates.
#[test]
fn templates_nonexistent_category() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", "nonexistent"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("0 templates available"),
        "Should show '0 templates available' for nonexistent category, got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category nonexistent should exit 0");
}

/// Empty category string shows zero templates or all templates.
#[test]
fn templates_empty_category() {
    let temp = setup_project("app_name: test\nsteps:\n  a:\n    command: \"cargo --version\"\nworkflows:\n  default:\n    steps: [a]\n");
    let mut s = spawn_bivvy(&["templates", "--category", ""], temp.path());

    let text = read_to_eof(&mut s);
    // An empty string category won't match any real category name,
    // so it should show 0 templates available
    assert!(
        text.contains("0 templates available"),
        "Empty category should show '0 templates available', got: {}",
        &text[..text.len().min(500)]
    );

    let status = s.get_process().wait().unwrap();
    assert!(status.success(), "templates --category '' should exit 0");
}
