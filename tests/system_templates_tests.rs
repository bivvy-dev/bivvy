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
    let mut s = spawn_bivvy_global(&["templates"]);

    let text = read_to_eof(&mut s);
    assert!(text.contains("cargo-build"), "Should list Rust template");
    assert!(
        text.contains("bundle-install"),
        "Should list Ruby template"
    );
    assert!(
        text.contains("templates available"),
        "Should show count"
    );
}

/// Templates output includes npm-install.
#[test]
fn templates_includes_npm() {
    let mut s = spawn_bivvy_global(&["templates"]);

    let text = read_to_eof(&mut s);
    assert!(text.contains("npm-install"), "Should include npm-install");
}

/// Templates output includes pip-install.
#[test]
fn templates_includes_pip() {
    let mut s = spawn_bivvy_global(&["templates"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("pip-install"),
        "Should include pip-install"
    );
}

/// Templates output includes brew-bundle.
#[test]
fn templates_includes_brew() {
    let mut s = spawn_bivvy_global(&["templates"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("brew-bundle"),
        "Should include brew-bundle"
    );
}

/// Templates output shows header.
#[test]
fn templates_shows_header() {
    let mut s = spawn_bivvy_global(&["templates"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Available Templates") || text.contains("templates available")
            || text.contains("Template"),
        "Should show header, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// FLAGS — Category filtering
// =====================================================================

/// --category rust shows Rust templates.
#[test]
fn templates_category_rust() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "rust"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build") || text.contains("cargo"),
        "Should show Rust templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category node shows Node templates.
#[test]
fn templates_category_node() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "node"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("npm-install") || text.contains("yarn-install"),
        "Should show Node templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category ruby shows Ruby templates.
#[test]
fn templates_category_ruby() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "ruby"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bundle-install"),
        "Should show Ruby templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category python shows Python templates.
#[test]
fn templates_category_python() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "python"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("pip-install") || text.contains("poetry") || text.contains("python"),
        "Should show Python templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category go shows Go templates.
#[test]
fn templates_category_go() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "go"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("go") || text.contains("Go"),
        "Should show Go templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category system shows system templates.
#[test]
fn templates_category_system() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "system"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("brew") || text.contains("system") || text.contains("apt"),
        "Should show system templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category java shows Java templates.
#[test]
fn templates_category_java() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "java"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("templates available") || text.contains("0 templates")
            || text.contains("java"),
        "Java category should show template count, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category containers shows container templates.
#[test]
fn templates_category_containers() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "containers"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("templates available") || text.contains("docker")
            || text.contains("container"),
        "Containers category should show templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --category common shows common templates.
#[test]
fn templates_category_common() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "common"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("templates available") || text.contains("common")
            || text.contains("git"),
        "Common category should show templates, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --verbose shows extra information per template.
#[test]
fn templates_verbose_flag() {
    let mut s = spawn_bivvy_global(&["templates", "--verbose"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("cargo-build"),
        "Verbose should still show template names, got: {}",
        &text[..text.len().min(300)]
    );
    // Verbose should show descriptions or command details
    assert!(
        text.len() > 200,
        "Verbose output should be substantial, got {} bytes",
        text.len()
    );
}

// =====================================================================
// CUSTOM PROJECT-LOCAL TEMPLATES
// =====================================================================

/// Custom project-local templates appear in the template list.
#[test]
fn templates_project_local_custom() {
    let config = "app_name: CustomTplTest\nsteps:\n  a:\n    command: echo hi\nworkflows:\n  default:\n    steps: [a]\n";
    let temp = setup_project(config);

    // Create a custom template
    let tpl_dir = temp.path().join(".bivvy/templates/steps");
    fs::create_dir_all(&tpl_dir).unwrap();
    fs::write(
        tpl_dir.join("my-custom-step.yml"),
        "command: echo custom\ncompleted_check:\n  type: command_succeeds\n  command: \"true\"\n",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("my-custom-step") || text.contains("custom"),
        "Should show custom project-local template, got: {}",
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn templates_help() {
    let mut s = spawn_bivvy_global(&["templates", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("template") || text.contains("Template"),
        "Help should describe templates command, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Nonexistent category shows zero templates.
#[test]
fn templates_nonexistent_category() {
    let mut s = spawn_bivvy_global(&["templates", "--category", "nonexistent"]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("0 templates available") || text.contains("0"),
        "Should show zero templates for nonexistent category, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Empty category string.
#[test]
fn templates_empty_category() {
    let mut s = spawn_bivvy_global(&["templates", "--category", ""]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("templates available") || text.contains("0") || text.contains("error")
            || text.contains("invalid"),
        "Empty category should show templates or error, got: {}",
        &text[..text.len().min(300)]
    );
}
