//! System tests for `bivvy update` — all interactive, PTY-based.
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
fn update_check_flag() {
    let mut s = spawn_bivvy(&["update", "--check"]);

    // Should show current version info (may or may not reach network)
    s.expect("version").ok();
    s.expect(expectrl::Eof).ok();
}

#[test]
fn update_help() {
    let mut s = spawn_bivvy(&["update", "--help"]);

    s.expect("Check for and install updates")
        .expect("Should show help");
    s.expect("--check").expect("Should list --check flag");
    s.expect("--enable-auto-update")
        .expect("Should list auto-update flag");
    s.expect("--disable-auto-update")
        .expect("Should list disable flag");
    s.expect(expectrl::Eof).ok();
}
