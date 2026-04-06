//! System tests for `bivvy completions` — all interactive, PTY-based.
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
fn completions_bash() {
    let mut s = spawn_bivvy(&["completions", "bash"]);

    s.expect("bivvy")
        .expect("Bash completions should reference bivvy");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn completions_zsh() {
    let mut s = spawn_bivvy(&["completions", "zsh"]);

    s.expect("bivvy")
        .expect("Zsh completions should reference bivvy");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn completions_fish() {
    let mut s = spawn_bivvy(&["completions", "fish"]);

    s.expect("bivvy")
        .expect("Fish completions should reference bivvy");
    s.expect(expectrl::Eof).unwrap();
}

#[test]
fn completions_help() {
    let mut s = spawn_bivvy(&["completions", "--help"]);

    s.expect("Generate shell completions")
        .expect("Should show help");
    s.expect(expectrl::Eof).unwrap();
}
