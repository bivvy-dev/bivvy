//! System tests for `bivvy templates` — PTY-based.
//!
//! These tests exercise the `bivvy templates` command as a subprocess in a
//! PTY, asserting on documented behavior: flags, output format, exit codes,
//! category filters, custom project-local templates, and help output.
//!
//! Tests are isolated from the user environment: each test gets its own
//! tempdir for both the project root and `HOME`, so user-installed templates
//! do not interfere.
#![cfg(unix)]

mod system;

use expectrl::WaitStatus;
use std::fs;
use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────

/// Minimal but realistic project config used for tests that just need a
/// project root. Uses `cargo --version` as a real command (per the
/// "real-commands" testing norm) even though the `templates` command does
/// not actually execute steps.
const MINIMAL_CONFIG: &str = r#"app_name: TemplatesTest
steps:
  check:
    title: "Check cargo"
    command: "cargo --version"
    completed_check:
      type: command_succeeds
      command: "cargo --version"
workflows:
  default:
    steps: [check]
"#;

// ─────────────────────────────────────────────────────────────────────
// Happy path — default listing
// ─────────────────────────────────────────────────────────────────────

/// `bivvy templates` lists built-in templates from multiple ecosystems,
/// shows the header, the add-hint, and the count line, then exits 0.
#[test]
fn templates_lists_builtin_templates_from_all_ecosystems() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates"], temp.path());

    let text = read_to_eof(&mut s);

    // Header (documented in docs/commands/templates.md)
    assert!(
        text.contains("Available Templates"),
        "Should show 'Available Templates' header. Got:\n{text}"
    );

    // Templates from multiple documented ecosystems
    assert!(
        text.contains("cargo-build"),
        "Should list cargo-build (rust). Got:\n{text}"
    );
    assert!(
        text.contains("bundle-install"),
        "Should list bundle-install (ruby). Got:\n{text}"
    );
    assert!(
        text.contains("npm-install"),
        "Should list npm-install (node). Got:\n{text}"
    );

    // Count line
    assert!(
        text.contains("templates available"),
        "Should show 'N templates available' line. Got:\n{text}"
    );

    // Add hint (documented behavior)
    assert!(
        text.contains("bivvy add"),
        "Should show 'bivvy add' hint. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// --category flag — documented categories
// ─────────────────────────────────────────────────────────────────────

/// `--category rust` shows rust templates and EXCLUDES templates from
/// other ecosystems.
#[test]
fn templates_category_rust_filters_to_rust_only() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--category", "rust"], temp.path());

    let text = read_to_eof(&mut s);

    assert!(
        text.contains("cargo-build"),
        "Should show cargo-build for --category rust. Got:\n{text}"
    );
    // Exclusion: other ecosystems must not appear
    assert!(
        !text.contains("bundle-install"),
        "Should NOT show ruby templates under --category rust. Got:\n{text}"
    );
    assert!(
        !text.contains("npm-install"),
        "Should NOT show node templates under --category rust. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

/// `--category node` shows node templates.
#[test]
fn templates_category_node_filters_to_node_only() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--category", "node"], temp.path());

    let text = read_to_eof(&mut s);

    assert!(
        text.contains("npm-install"),
        "Should show npm-install for --category node. Got:\n{text}"
    );
    assert!(
        !text.contains("cargo-build"),
        "Should NOT show rust templates under --category node. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

/// `--category ruby` shows ruby templates.
#[test]
fn templates_category_ruby_filters_to_ruby_only() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--category", "ruby"], temp.path());

    let text = read_to_eof(&mut s);

    assert!(
        text.contains("bundle-install"),
        "Should show bundle-install for --category ruby. Got:\n{text}"
    );
    assert!(
        !text.contains("cargo-build"),
        "Should NOT show rust templates under --category ruby. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

/// `--category python` shows python templates.
#[test]
fn templates_category_python_filters_to_python_only() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--category", "python"], temp.path());

    let text = read_to_eof(&mut s);

    assert!(
        text.contains("pip-install"),
        "Should show pip-install for --category python. Got:\n{text}"
    );
    assert!(
        !text.contains("cargo-build"),
        "Should NOT show rust templates under --category python. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// Sad path — invalid / empty categories
// ─────────────────────────────────────────────────────────────────────

/// A nonexistent category name yields "0 templates available" and exit 0.
#[test]
fn templates_nonexistent_category_shows_zero_and_exits_success() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(
        &["templates", "--category", "definitely-not-a-category"],
        temp.path(),
    );

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("0 templates available"),
        "Should show '0 templates available' for unknown category. Got:\n{text}"
    );
    // No well-known template names should slip through
    assert!(
        !text.contains("cargo-build"),
        "Should not show any templates for unknown category. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

/// An empty `--category` argument shows "0 templates available" and exit 0.
#[test]
fn templates_empty_category_shows_zero_and_exits_success() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--category", ""], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("0 templates available"),
        "Empty category should show '0 templates available'. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// --verbose flag
// ─────────────────────────────────────────────────────────────────────

/// `--verbose` still shows the full listing with header, templates and
/// count, and exits 0.
#[test]
fn templates_verbose_flag_shows_full_listing() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--verbose"], temp.path());

    let text = read_to_eof(&mut s);

    assert!(
        text.contains("Available Templates"),
        "Verbose output should still show header. Got:\n{text}"
    );
    assert!(
        text.contains("cargo-build"),
        "Verbose output should list templates. Got:\n{text}"
    );
    assert!(
        text.contains("templates available"),
        "Verbose output should show count line. Got:\n{text}"
    );
    assert!(
        text.contains("bivvy add"),
        "Verbose output should show add hint. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// --help
// ─────────────────────────────────────────────────────────────────────

/// `bivvy templates --help` prints usage and description, exits 0.
#[test]
fn templates_help_shows_usage_and_exits_success() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--help"], temp.path());

    let text = read_to_eof(&mut s);

    assert!(
        text.to_lowercase().contains("usage"),
        "Help output should include 'Usage' section. Got:\n{text}"
    );
    assert!(
        text.contains("--category"),
        "Help output should document --category flag. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// Custom project-local templates
// ─────────────────────────────────────────────────────────────────────

/// A project-local template under `.bivvy/templates/steps/` appears in the
/// listing alongside built-ins.
#[test]
fn templates_lists_project_local_custom_template() {
    let temp = setup_project(MINIMAL_CONFIG);

    let tpl_dir = temp.path().join(".bivvy/templates/steps");
    fs::create_dir_all(&tpl_dir).unwrap();
    fs::write(
        tpl_dir.join("my-custom-step.yml"),
        r#"name: my-custom-step
description: "Custom project-local setup step"
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
        "Should list project-local custom template. Got:\n{text}"
    );
    assert!(
        text.contains("Custom project-local setup step"),
        "Should show description of custom template. Got:\n{text}"
    );
    // Built-ins must still appear
    assert!(
        text.contains("cargo-build"),
        "Built-in templates should still appear alongside custom ones. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

/// When `--category` is given, the custom category only appears if it
/// matches the filter. With a built-in filter, custom templates are
/// hidden.
#[test]
fn templates_custom_templates_hidden_by_builtin_category_filter() {
    let temp = setup_project(MINIMAL_CONFIG);

    let tpl_dir = temp.path().join(".bivvy/templates/steps");
    fs::create_dir_all(&tpl_dir).unwrap();
    fs::write(
        tpl_dir.join("project-only.yml"),
        r#"name: project-only
description: "Project-only template"
category: custom
version: "1.0.0"
platforms: [macos, linux, windows]
step:
  command: "cargo --version"
"#,
    )
    .unwrap();

    let mut s = spawn_bivvy(&["templates", "--category", "rust"], temp.path());
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("cargo-build"),
        "Rust filter should show built-in rust templates. Got:\n{text}"
    );
    assert!(
        !text.contains("project-only"),
        "Custom templates should not appear when filtering by a built-in category. Got:\n{text}"
    );

    assert_exit_code(&s, 0);
}

// ─────────────────────────────────────────────────────────────────────
// Exit code for top-level failures
// ─────────────────────────────────────────────────────────────────────

/// An unknown flag should fail fast with a non-zero exit code.
#[test]
fn templates_unknown_flag_exits_nonzero() {
    let temp = setup_project(MINIMAL_CONFIG);
    let mut s = spawn_bivvy(&["templates", "--no-such-flag"], temp.path());

    // Drain output before checking exit code.
    let _ = read_to_eof(&mut s);

    let status = s.get_process().wait().unwrap();
    match status {
        WaitStatus::Exited(_, code) => {
            assert_ne!(
                code, 0,
                "Unknown flag should produce a non-zero exit code, got {code}"
            );
        }
        other => panic!("Expected Exited status, got {other:?}"),
    }
}
