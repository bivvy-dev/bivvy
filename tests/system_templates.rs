//! System tests for `bivvy templates` — all interactive, PTY-based.
#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::process::Command;
use std::time::Duration;

fn spawn_bivvy(args: &[&str]) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(Duration::from_secs(15)));
    session
}

#[test]
fn templates_lists_builtin_templates() {
    let mut s = spawn_bivvy(&["templates"]);

    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(text.contains("cargo-build"), "Should list Rust template");
    assert!(text.contains("bundle-install"), "Should list Ruby template");
    assert!(
        text.contains("templates available"),
        "Should show template count"
    );
}

#[test]
fn templates_category_filter_rust() {
    let mut s = spawn_bivvy(&["templates", "--category", "rust"]);

    s.expect("cargo-build").expect("Should show Rust templates");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn templates_category_filter_node() {
    let mut s = spawn_bivvy(&["templates", "--category", "node"]);

    s.expect("npm-install").expect("Should show Node templates");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn templates_category_filter_ruby() {
    let mut s = spawn_bivvy(&["templates", "--category", "ruby"]);

    s.expect("bundle-install")
        .expect("Should show Ruby templates");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn templates_nonexistent_category_shows_zero() {
    let mut s = spawn_bivvy(&["templates", "--category", "nonexistent"]);

    s.expect("0 templates available")
        .expect("Should show zero templates");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn templates_verbose_flag() {
    let mut s = spawn_bivvy(&["templates", "--verbose"]);

    s.expect("cargo-build").unwrap();
    s.expect(expectrl::Eof).ok();
}
