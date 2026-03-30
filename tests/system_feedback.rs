//! System tests for `bivvy feedback` — all interactive, PTY-based.
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

// ---------------------------------------------------------------------------
// Interactive feedback capture
// ---------------------------------------------------------------------------

#[test]
fn feedback_interactive_capture() {
    let mut s = spawn_bivvy(&["feedback", "--no-deliver"]);

    // Interactive mode should prompt for category and message
    // Accept default category
    if s.expect("category").is_ok() || s.expect("kind").is_ok() {
        s.send_line("").unwrap();
    }

    // Enter feedback message
    if s.expect("feedback").is_ok() || s.expect("message").is_ok() {
        s.send_line("PTY system test feedback").unwrap();
    }

    s.expect("Feedback captured").ok();
    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// Quick capture (message as argument)
// ---------------------------------------------------------------------------

#[test]
fn feedback_quick_capture_with_message() {
    let mut s = spawn_bivvy(&["feedback", "--no-deliver", "Quick test feedback"]);

    s.expect("Feedback captured")
        .expect("Should confirm capture");
    s.expect(expectrl::Eof).ok();
}

#[test]
fn feedback_with_tags() {
    let mut s = spawn_bivvy(&[
        "feedback",
        "--no-deliver",
        "--tag",
        "bug,testing",
        "Tagged feedback message",
    ]);

    s.expect("Feedback captured").unwrap();
    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// List subcommand
// ---------------------------------------------------------------------------

#[test]
fn feedback_list() {
    // Capture something first
    let mut s = spawn_bivvy(&["feedback", "--no-deliver", "List test entry"]);
    s.expect(expectrl::Eof).ok();

    // Then list
    let mut s = spawn_bivvy(&["feedback", "list", "--all"]);
    s.expect(expectrl::Eof).ok();
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

#[test]
fn feedback_help() {
    let mut s = spawn_bivvy(&["feedback", "--help"]);

    s.expect("Capture and manage feedback")
        .expect("Should show help");
    s.expect(expectrl::Eof).ok();
}
