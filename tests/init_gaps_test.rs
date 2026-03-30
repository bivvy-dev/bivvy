//! Integration tests for `bivvy init` — gaps not covered by cli_test.rs.
//!
//! Covers: template injection during init, detection-based config generation,
//! --force flag, and generated config content.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// --- Template injection / detection-based config generation ---

#[test]
fn init_detects_ruby_project_and_includes_bundler() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created .bivvy/config.yml"));

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(
        config.contains("bundler"),
        "Config should reference bundler template for Ruby project"
    );
    assert!(
        config.contains("template: bundler"),
        "Config should use template: bundler"
    );
    Ok(())
}

#[test]
fn init_detects_node_project_and_includes_package_manager() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempDir::new()?;
    fs::write(
        temp.path().join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    // Should detect npm/yarn/pnpm depending on lockfiles; at minimum should have a step
    assert!(
        config.contains("npm") || config.contains("yarn") || config.contains("pnpm"),
        "Config should include a Node.js package manager step, got:\n{}",
        config
    );
    Ok(())
}

#[test]
fn init_detects_rust_project_and_includes_cargo() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(
        config.contains("cargo"),
        "Config should reference cargo template for Rust project"
    );
    Ok(())
}

#[test]
fn init_empty_project_creates_config_without_steps() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    // When no technologies are detected, config has no steps or workflows
    assert!(
        config.contains("app_name:"),
        "Config should include app_name"
    );
    assert!(
        config.contains("settings:"),
        "Config should include settings section"
    );
    assert!(
        !config.contains("\nsteps:\n"),
        "Config should not include steps section when nothing is detected"
    );
    assert!(
        !config.contains("\nworkflows:\n"),
        "Config should not include workflows section when nothing is detected"
    );
    Ok(())
}

#[test]
fn init_multi_language_project_detects_all() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'")?;
    fs::write(
        temp.path().join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(
        config.contains("bundler"),
        "Should detect Ruby/bundler in multi-lang project"
    );
    // Should also detect a JS package manager
    assert!(
        config.contains("npm") || config.contains("yarn") || config.contains("pnpm"),
        "Should detect Node.js package manager in multi-lang project, got:\n{}",
        config
    );
    Ok(())
}

// --- --force flag via CLI ---

#[test]
fn init_force_overwrites_existing_config() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir)?;
    fs::write(bivvy_dir.join("config.yml"), "app_name: OldApp")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--force", "--minimal"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created .bivvy/config.yml"));

    let config = fs::read_to_string(bivvy_dir.join("config.yml"))?;
    assert!(
        !config.contains("OldApp"),
        "Old config should have been overwritten"
    );
    Ok(())
}

// --- Generated config content ---

#[test]
fn init_generated_config_contains_settings() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    assert!(
        config.contains("settings:"),
        "Config should include settings section"
    );
    assert!(
        config.contains("default_output:"),
        "Config should include default_output setting"
    );
    assert!(
        config.contains("app_name:"),
        "Config should include app_name"
    );
    Ok(())
}

#[test]
fn init_generated_config_includes_template_comments() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join("Gemfile"), "source 'https://rubygems.org'")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let config = fs::read_to_string(temp.path().join(".bivvy/config.yml"))?;
    // When a template is known, the generated config should include
    // commented-out info from the template
    assert!(
        config.contains("# command:"),
        "Config should include commented command from template"
    );
    Ok(())
}

// --- Gitignore handling ---

#[test]
fn init_updates_gitignore_if_present() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    fs::write(temp.path().join(".gitignore"), "/tmp\n")?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    let gitignore = fs::read_to_string(temp.path().join(".gitignore"))?;
    assert!(
        gitignore.contains(".bivvy/config.local.yml"),
        "Should add local config to .gitignore"
    );
    // Should preserve existing entries
    assert!(
        gitignore.contains("/tmp"),
        "Should preserve existing entries"
    );
    Ok(())
}

#[test]
fn init_no_gitignore_does_not_create_one() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert().success();

    // init only updates an existing .gitignore, doesn't create one
    assert!(
        !temp.path().join(".gitignore").exists(),
        "Should not create .gitignore if it didn't exist"
    );
    Ok(())
}

#[test]
fn init_scanning_output_shown() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path());
    cmd.args(["init", "--minimal"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Scanning project"));
    Ok(())
}
