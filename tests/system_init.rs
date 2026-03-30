//! System tests for `bivvy init` — all interactive, PTY-based.
#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn spawn_bivvy(args: &[&str], dir: &std::path::Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
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

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should show detection results");
    s.expect("Rust").expect("Should detect Rust");

    // Accept default selections with Enter
    s.send_line("").unwrap();

    // Should eventually create config
    s.expect("Created .bivvy/config.yml").ok();
    s.expect(expectrl::Eof).ok();

    assert!(temp.path().join(".bivvy/config.yml").exists());
}

#[test]
fn init_interactive_rust_project_offers_run() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"",
    )
    .unwrap();

    let mut s = spawn_bivvy(&["init"], temp.path());

    // Accept detected templates
    s.expect("Select steps").ok();
    s.send_line("").unwrap();

    // After config creation, should ask "Run setup now?"
    s.expect("Run setup now").ok();

    // Decline — press 'n'
    s.send("n").unwrap();

    s.expect(expectrl::Eof).ok();
    assert!(temp.path().join(".bivvy/config.yml").exists());
}

#[test]
fn init_interactive_node_project() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )
    .unwrap();

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("Detected technologies")
        .expect("Should show detection");

    // Accept defaults
    s.send_line("").unwrap();
    s.expect("Created .bivvy/config.yml").ok();

    // Decline run
    if s.expect("Run setup now").is_ok() {
        s.send("n").unwrap();
    }

    s.expect(expectrl::Eof).ok();
    assert!(temp.path().join(".bivvy/config.yml").exists());
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

#[test]
fn init_minimal_flag_skips_prompts() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml").unwrap();
    s.expect(expectrl::Eof).ok();
    assert!(temp.path().join(".bivvy/config.yml").exists());
}

#[test]
fn init_force_overwrites_existing() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: OldConfig").unwrap();

    let mut s = spawn_bivvy(&["init", "--force", "--minimal"], temp.path());

    s.expect("Created .bivvy/config.yml").unwrap();
    s.expect(expectrl::Eof).ok();

    let config = fs::read_to_string(bivvy_dir.join("config.yml")).unwrap();
    assert!(!config.contains("OldConfig"));
}

#[test]
fn init_refuses_existing_config() {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), "app_name: Existing").unwrap();

    let mut s = spawn_bivvy(&["init"], temp.path());

    s.expect("already exists")
        .expect("Should refuse overwrite without --force");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn init_verbose_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal", "--verbose"], temp.path());

    s.expect("Created").unwrap();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn init_quiet_flag() {
    let temp = TempDir::new().unwrap();
    let mut s = spawn_bivvy(&["init", "--minimal", "--quiet"], temp.path());

    s.expect(expectrl::Eof).ok();
    assert!(temp.path().join(".bivvy/config.yml").exists());
}
