//! System tests for `bivvy init` — all interactive, PTY-based.
#![cfg(unix)]

mod system;

use expectrl::WaitStatus;
use std::fs;
use system::helpers::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: spawn bivvy with HOME isolated to the temp directory
// ---------------------------------------------------------------------------

fn spawn_init(args: &[&str], dir: &std::path::Path) -> expectrl::Session {
    let home = dir.to_str().unwrap();
    spawn_bivvy_with_env(args, dir, &[("HOME", home)])
}

// ---------------------------------------------------------------------------
// Interactive init flow
// ---------------------------------------------------------------------------

#[test]
fn init_interactive_shows_detected_technologies() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )
    .unwrap();

    let mut s = spawn_init(&["init"], temp.path());

    // Verify detection output
    expect_or_dump(&mut s, "Detected technologies:", "Should show detection header");
    expect_or_dump(
        &mut s,
        "Rust - Cargo.toml found",
        "Should detect Rust from Cargo.toml",
    );

    // Accept default selections with Enter
    s.send_line("").unwrap();

    // Should create config
    expect_or_dump(&mut s, "Created .bivvy/config.yml", "Should confirm config creation");

    // Decline run
    expect_or_dump(&mut s, "Run setup now?", "Should offer to run setup");
    s.send_line("").unwrap(); // Default is "No"

    let text = read_to_eof(&mut s);
    // After declining, should show hint about bivvy run
    assert!(
        text.contains("bivvy run"),
        "Should show hint about bivvy run after declining, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: config file exists and contains expected content
    let config_path = temp.path().join(".bivvy/config.yml");
    assert!(config_path.exists(), "Config file should be created");
    let config = fs::read_to_string(&config_path).unwrap();
    assert!(
        config.contains("app_name"),
        "Config should contain app_name, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("cargo-build"),
        "Config should contain cargo-build step for Rust project, got: {}",
        &config[..config.len().min(500)]
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_interactive_rust_project_offers_run() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )
    .unwrap();

    let mut s = spawn_init(&["init"], temp.path());

    // Accept detected templates
    expect_or_dump(&mut s, "Select steps to include", "Should show step selection prompt");
    s.send_line("").unwrap();

    expect_or_dump(&mut s, "Created .bivvy/config.yml", "Should confirm config creation");

    // After config creation, should ask "Run setup now?"
    expect_or_dump(&mut s, "Run setup now?", "Should offer to run setup");

    // Decline — press Enter for default "No"
    s.send_line("").unwrap();

    // Phase 2 outcome: declining should print the post-init hint pointing
    // users at `bivvy run` and `bivvy templates`.
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy run"),
        "Declined phase 2 should show bivvy run hint, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("bivvy templates"),
        "Declined phase 2 should show bivvy templates hint, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: config file exists and contains Rust template
    let config_path = temp.path().join(".bivvy/config.yml");
    assert!(config_path.exists(), "Config file should be created");
    let config = fs::read_to_string(&config_path).unwrap();
    assert!(
        config.contains("cargo-build"),
        "Config should include cargo-build for Rust project, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("template: cargo-build"),
        "Config should reference cargo-build template, got: {}",
        &config[..config.len().min(500)]
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_interactive_node_project() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )
    .unwrap();

    let mut s = spawn_init(&["init"], temp.path());

    expect_or_dump(&mut s, "Detected technologies:", "Should show detection header");
    expect_or_dump(
        &mut s,
        "Node.js - package.json found",
        "Should detect Node.js from package.json",
    );

    // Accept defaults
    s.send_line("").unwrap();
    expect_or_dump(&mut s, "Created .bivvy/config.yml", "Should confirm config creation");

    // Decline run
    expect_or_dump(&mut s, "Run setup now?", "Should offer to run setup");
    s.send_line("").unwrap(); // Default is "No"

    // Phase 2 outcome: declining should show the post-init hint.
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("bivvy run"),
        "Declined phase 2 should show bivvy run hint, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: config exists and contains Node-specific content
    let config_path = temp.path().join(".bivvy/config.yml");
    assert!(config_path.exists(), "Config file should be created");
    let config = fs::read_to_string(&config_path).unwrap();
    assert!(
        config.contains("app_name"),
        "Config should contain app_name, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("npm-install"),
        "Config should contain npm-install step for Node.js project, got: {}",
        &config[..config.len().min(500)]
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

#[test]
fn init_minimal_flag_skips_prompts() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--minimal"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Should confirm config creation, got: {}",
        &text[..text.len().min(500)]
    );
    // Minimal should not show interactive selection prompt
    assert!(
        !text.contains("Select steps to include"),
        "Minimal mode should skip step selection prompt, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config file should be created"
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_force_overwrites_existing() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: OldConfig").unwrap();

    let mut s = spawn_init(&["init", "--force", "--minimal"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Should confirm config creation after force overwrite, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: old config replaced with new content
    let config = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(
        !config.contains("OldConfig"),
        "Old config content should be replaced, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("app_name"),
        "New config should contain app_name, got: {}",
        &config[..config.len().min(500)]
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_refuses_existing_config() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: Existing").unwrap();

    let mut s = spawn_init(&["init"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Configuration already exists. Use --force to overwrite."),
        "Should refuse overwrite without --force, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify exit code is 1 (failure)
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "Expected exit code 1 for existing config refusal"
    );
}

#[test]
fn init_verbose_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--minimal", "--verbose"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Verbose mode should still show config creation message, got: {}",
        &text[..text.len().min(500)]
    );
    // Verbose init shows the bivvy header with version and "· init" subtitle.
    assert!(
        text.contains("bivvy v"),
        "Verbose mode should show version header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("· init"),
        "Verbose mode should show '· init' header subtitle, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config file should be created"
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_quiet_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--minimal", "--quiet"], temp.path());

    let text = read_to_eof(&mut s);
    // Quiet mode should still show essential status messages
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Quiet mode should still show config creation confirmation, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect
    assert!(
        temp.path().join(".bivvy/config.yml").exists(),
        "Config file should be created"
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

// ---------------------------------------------------------------------------
// --template flag
// ---------------------------------------------------------------------------

#[test]
fn init_template_flag_creates_config_with_template() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--template", "cargo-build"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Using template: cargo-build"),
        "Should show which template is being used, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Should confirm config creation, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Workflow: default"),
        "Should print default workflow summary, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Steps: 1 (cargo-build)"),
        "Should print step summary listing cargo-build, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: config has the template
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("template: cargo-build"),
        "Config should reference cargo-build template, got: {}",
        &config[..config.len().min(500)]
    );
    // Workflow section should list cargo-build
    assert!(
        config.contains("steps: [cargo-build]"),
        "Config workflow should list cargo-build, got: {}",
        &config[..config.len().min(500)]
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_template_flag_unknown_template_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--template", "nonexistent-template"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No template or category found matching 'nonexistent-template'"),
        "Should show error for unknown template, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Run 'bivvy templates' to see available templates."),
        "Should suggest running 'bivvy templates', got: {}",
        &text[..text.len().min(500)]
    );

    // Config should NOT be created
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Config file should not be created for unknown template"
    );

    // Verify exit code is 1 (failure)
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "Expected exit code 1 for unknown template"
    );
}

// ---------------------------------------------------------------------------
// --from flag
// ---------------------------------------------------------------------------

#[test]
fn init_from_flag_copies_config() {
    // Set up a source project with a config
    let source = TempDir::new().unwrap();
    let source_bivvy = source.path().join(".bivvy");
    fs::create_dir_all(&source_bivvy).unwrap();
    fs::write(
        source_bivvy.join("config.yml"),
        "app_name: SourceProject\nsteps:\n  build:\n    command: \"cargo build\"\n",
    )
    .unwrap();

    let target = TempDir::new().unwrap();
    let source_str = source.path().to_str().unwrap();
    let home = target.path().to_str().unwrap();
    let mut s = spawn_bivvy_with_env(
        &["init", "--from", source_str],
        target.path(),
        &[("HOME", home)],
    );

    let text = read_to_eof(&mut s);
    // Full message is: "Copied configuration from <source>/.bivvy/config.yml"
    // Note: long paths may be wrapped across PTY lines, so assert on the
    // message prefix and the filename portion independently.
    assert!(
        text.contains("Copied configuration from"),
        "Should confirm config was copied, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("config.yml"),
        "Copied message should reference the source config filename, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: config was copied
    let config = fs::read_to_string(target.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("SourceProject"),
        "Copied config should contain source project name, got: {}",
        &config[..config.len().min(500)]
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_from_flag_nonexistent_source_fails() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--from", "/nonexistent/path"], temp.path());

    let text = read_to_eof(&mut s);
    // Full message is: "No .bivvy/config.yml found at /nonexistent/path/.bivvy/config.yml"
    assert!(
        text.contains("No .bivvy/config.yml found at"),
        "Should show error for missing source config, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("nonexistent"),
        "Error should reference the provided --from path, got: {}",
        &text[..text.len().min(500)]
    );

    // Config should NOT be created
    assert!(
        !temp.path().join(".bivvy/config.yml").exists(),
        "Config file should not be created when source doesn't exist"
    );

    // Verify exit code is 1 (failure)
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 1),
        "Expected exit code 1 for missing source"
    );
}

// ---------------------------------------------------------------------------
// Side effects
// ---------------------------------------------------------------------------

#[test]
fn init_updates_gitignore() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join(".gitignore"), "node_modules\n").unwrap();

    let mut s = spawn_init(&["init", "--minimal"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Should confirm config creation, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Added .bivvy/config.local.yml to .gitignore"),
        "Should print gitignore update message, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify side effect: .gitignore updated
    let gitignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    assert!(
        gitignore.contains(".bivvy/config.local.yml"),
        "Should add .bivvy/config.local.yml to .gitignore, got: {}",
        &gitignore
    );
    // Verify existing gitignore entries are preserved
    assert!(
        gitignore.contains("node_modules"),
        "Should preserve existing gitignore entries, got: {}",
        &gitignore
    );

    // Verify exit code
    assert_exit_code(&s, 0);
}

#[test]
fn init_config_contains_settings_and_header() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_init(&["init", "--minimal"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Created .bivvy/config.yml"),
        "Should confirm config creation, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Workflow: default"),
        "Should print default workflow summary, got: {}",
        &text[..text.len().min(500)]
    );

    // Verify config structure
    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        config.contains("# Bivvy configuration for"),
        "Config should have header comment, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("# Docs: https://bivvy.dev/configuration"),
        "Config should have docs link, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("default_output: verbose"),
        "Config should have default output setting, got: {}",
        &config[..config.len().min(500)]
    );
    assert!(
        config.contains("settings:"),
        "Config should have settings section, got: {}",
        &config[..config.len().min(500)]
    );

    assert_exit_code(&s, 0);
}
