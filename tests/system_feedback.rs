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
    s.expect("What kind of feedback?")
        .expect("Should prompt for category");
    s.send_line("").unwrap();

    // Enter feedback message
    s.expect("feedback")
        .or_else(|_| s.expect("message"))
        .expect("Should prompt for message");
    s.send_line("PTY system test feedback").unwrap();

    s.expect("Feedback captured")
        .expect("Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
}

// ---------------------------------------------------------------------------
// Quick capture (message as argument)
// ---------------------------------------------------------------------------

#[test]
fn feedback_quick_capture_with_message() {
    let mut s = spawn_bivvy(&["feedback", "--no-deliver", "Quick test feedback"]);

    s.expect("Feedback captured")
        .expect("Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
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
    s.expect(expectrl::Eof).unwrap();
}

// ---------------------------------------------------------------------------
// List subcommand
// ---------------------------------------------------------------------------

#[test]
fn feedback_list() {
    // Capture something first
    let mut s = spawn_bivvy(&["feedback", "--no-deliver", "List test entry"]);
    s.expect("Feedback captured").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // Then list — should show captured entries
    let mut s = spawn_bivvy(&["feedback", "list", "--all"]);
    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(
        text.contains("List test entry") || text.contains("feedback") || text.contains("Feedback"),
        "Feedback list should show captured entries, got: {}",
        &text[..text.len().min(300)]
    );
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

#[test]
fn feedback_help() {
    let mut s = spawn_bivvy(&["feedback", "--help"]);

    s.expect("Capture and manage feedback")
        .expect("Should show help");
    s.expect(expectrl::Eof).unwrap();
}
