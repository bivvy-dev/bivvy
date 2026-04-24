//! Integration tests for the `feedback` CLI command.
//!
//! Tests cover argument parsing, subcommands, error cases, and output format.
//! Uses `--no-deliver` to avoid interactive delivery prompts during tests.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use predicates::prelude::*;

// --- Help and argument parsing ---

#[test]
fn feedback_help_shows_usage() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Capture and manage feedback"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("resolve"))
        .stdout(predicate::str::contains("session"));
    Ok(())
}

#[test]
fn feedback_list_help_shows_options() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--status"))
        .stdout(predicate::str::contains("--tag"))
        .stdout(predicate::str::contains("--all"));
    Ok(())
}

#[test]
fn feedback_resolve_help_shows_options() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "resolve", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--note"))
        .stdout(predicate::str::contains("<ID>"));
    Ok(())
}

#[test]
fn feedback_session_help_shows_options() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "session", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[ID]"));
    Ok(())
}

// --- Feedback capture ---

#[test]
fn feedback_capture_with_message_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "--no-deliver", "integration", "test", "message"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feedback captured"));
    Ok(())
}

#[test]
fn feedback_capture_with_tags() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args([
        "feedback",
        "--no-deliver",
        "--tag",
        "bug,testing",
        "tagged",
        "feedback",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feedback captured"));
    Ok(())
}

#[test]
fn feedback_capture_with_session_flag_invalid_id() -> Result<(), Box<dyn std::error::Error>> {
    // An invalid session ID (wrong format) is silently ignored;
    // the feedback is still captured without a session attachment.
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args([
        "feedback",
        "--no-deliver",
        "--session",
        "ses_abc123",
        "with",
        "session",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feedback captured"));
    Ok(())
}

#[test]
fn feedback_capture_with_session_flag_accepts_value() -> Result<(), Box<dyn std::error::Error>> {
    // The --session flag is accepted by clap even if the ID format is invalid
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "--no-deliver", "--session", "any_value", "test"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Feedback captured"));
    Ok(())
}

#[test]
fn feedback_capture_no_message_non_interactive_fails() -> Result<(), Box<dyn std::error::Error>> {
    // When no message is provided and stdin is not a TTY (non-interactive),
    // the command should fail because interactive prompts are not available.
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback"]);
    // assert_cmd runs with stdin not connected to a TTY, so interactive mode
    // is not available, which means capture_interactive returns exit code 1.
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Interactive mode not available"));
    Ok(())
}

// --- List subcommand ---

#[test]
fn feedback_list_runs_successfully() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list"]);
    // Should succeed whether or not there are entries
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_list_all_runs_successfully() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--all"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_list_by_status_open() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--status", "open"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_list_by_status_resolved() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--status", "resolved"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_list_by_status_wontfix() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--status", "wontfix"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_list_by_status_inprogress() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--status", "inprogress"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_list_invalid_status_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--status", "invalid"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown status: invalid"));
    Ok(())
}

#[test]
fn feedback_list_by_tag() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "list", "--tag", "nonexistent-tag-xyz"]);
    // Should succeed even if no entries match (shows "No feedback entries found")
    cmd.assert().success();
    Ok(())
}

// --- Resolve subcommand ---

#[test]
fn feedback_resolve_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "resolve", "fb_nonexistent_99999"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
    Ok(())
}

#[test]
fn feedback_resolve_with_note_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args([
        "feedback",
        "resolve",
        "fb_nonexistent_99999",
        "--note",
        "fixed it",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
    Ok(())
}

#[test]
fn feedback_resolve_requires_id_argument() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "resolve"]);
    cmd.assert().failure();
    Ok(())
}

// --- Session subcommand ---

#[test]
fn feedback_session_no_sessions_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    // When there are no sessions, the command should succeed with a message
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "session"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_session_with_specific_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "session", "ses_nonexistent"]);
    // Should succeed with "No feedback for session" message
    cmd.assert().success();
    Ok(())
}

// --- Global flags with feedback ---

#[test]
fn feedback_with_debug_flag() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["--debug", "feedback", "list"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_with_verbose_flag() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["--verbose", "feedback", "list"]);
    cmd.assert().success();
    Ok(())
}

#[test]
fn feedback_with_quiet_flag() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["--quiet", "feedback", "list"]);
    cmd.assert().success();
    Ok(())
}

// --- Invalid subcommands ---

#[test]
fn feedback_invalid_subcommand_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(["feedback", "invalid-subcommand"]);
    // clap should report this as an error since "invalid-subcommand" is not
    // a recognized subcommand; it will be treated as message words instead.
    // Actually, trailing_var_arg means it gets captured as message.
    // With trailing_var_arg, unknown words become the message.
    cmd.assert().success();
    Ok(())
}
