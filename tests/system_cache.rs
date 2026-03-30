//! System tests for `bivvy cache` — all interactive, PTY-based.
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
fn cache_list() {
    let mut s = spawn_bivvy(&["cache", "list"]);

    // May show entries or be empty — either way should succeed
    s.expect(expectrl::Eof).ok();
}

#[test]
fn cache_stats() {
    let mut s = spawn_bivvy(&["cache", "stats"]);

    s.expect("Cache Statistics")
        .expect("Should show stats header");
    s.expect("Total entries:").expect("Should show entry count");
    s.expect("Total size:").expect("Should show total size");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn cache_clear_expired() {
    let mut s = spawn_bivvy(&["cache", "clear", "--expired"]);

    s.expect("Cleared").expect("Should confirm clearing");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn cache_clear_force() {
    let mut s = spawn_bivvy(&["cache", "clear", "--force"]);

    s.expect(expectrl::Eof).ok();
}

#[test]
fn cache_help() {
    let mut s = spawn_bivvy(&["cache", "--help"]);

    s.expect("Manage template cache")
        .expect("Should show cache help");
    s.expect(expectrl::Eof).ok();
}
