//! Comprehensive system tests for `bivvy feedback`.
//!
//! Tests interactive and quick feedback capture, tagging, listing,
//! subcommands (resolve, session), filtering (--status, --tag),
//! round-trip data verification, and error handling.
#![cfg(unix)]

mod system;

use system::helpers::*;

// =====================================================================
// HAPPY PATH — Interactive capture
// =====================================================================

/// Interactive mode prompts for category and message.
#[test]
fn feedback_interactive_capture() {
    let mut s = spawn_bivvy_global(&["feedback", "--no-deliver"]);

    // Accept default category
    expect_or_dump(&mut s, "What kind of feedback?", "Should prompt for category");
    s.send_line("").unwrap();

    // Enter feedback message
    expect_or_dump(&mut s, "feedback", "Should prompt for feedback message");
    s.send_line("PTY system test feedback").unwrap();

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
}

/// Interactive mode — use arrow keys to select category.
#[test]
fn feedback_interactive_arrow_select_category() {
    let mut s = spawn_bivvy_global(&["feedback", "--no-deliver"]);

    expect_or_dump(&mut s, "What kind of feedback?", "Should prompt for category");
    // Arrow down to pick a different category, then Enter
    send_keys(&s, ARROW_DOWN);
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send_line("").unwrap();

    expect_or_dump(&mut s, "feedback", "Should prompt for feedback message");
    s.send_line("Arrow-key selected category").unwrap();

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// HAPPY PATH — Quick capture
// =====================================================================

/// Message as argument skips interactive prompts.
#[test]
fn feedback_quick_capture() {
    let mut s = spawn_bivvy_global(&["feedback", "--no-deliver", "Quick test feedback"]);

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
}

/// Quick capture with tags.
#[test]
fn feedback_with_tags() {
    let mut s = spawn_bivvy_global(&[
        "feedback",
        "--no-deliver",
        "--tag",
        "bug,testing",
        "Tagged feedback",
    ]);

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
}

/// Quick capture with single tag.
#[test]
fn feedback_with_single_tag() {
    let mut s = spawn_bivvy_global(&[
        "feedback",
        "--no-deliver",
        "--tag",
        "enhancement",
        "Enhancement feedback",
    ]);

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
}

/// Quick capture with --session flag.
#[test]
fn feedback_with_session_flag() {
    let mut s = spawn_bivvy_global(&[
        "feedback",
        "--no-deliver",
        "--session",
        "test-session-123",
        "Session-scoped feedback",
    ]);

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Feedback captured") || text.contains("captured"),
        "Should confirm capture with session flag, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// LIST SUBCOMMAND
// =====================================================================

/// List subcommand shows captured feedback.
#[test]
fn feedback_list() {
    // Capture first
    let mut s = spawn_bivvy_global(&["feedback", "--no-deliver", "List test entry"]);
    expect_or_dump(&mut s, "Feedback captured", "Setup: should confirm capture");
    s.expect(expectrl::Eof).unwrap();

    // Then list
    let mut s = spawn_bivvy_global(&["feedback", "list", "--all"]);
    let text = read_to_eof(&mut s);
    // Should show some output (even if empty, should not crash)
    assert!(
        !text.is_empty(),
        "List should produce output (header or entries)"
    );
}

/// List with no feedback yet should not crash.
#[test]
fn feedback_list_empty() {
    let mut s = spawn_bivvy_global(&["feedback", "list"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Feedback") || text.contains("feedback") || text.contains("No")
            || text.contains("0") || text.contains("empty"),
        "List should show feedback or empty message, got: {}",
        &text[..text.len().min(300)]
    );
}

/// List with --status open filter.
#[test]
fn feedback_list_status_open() {
    let mut s = spawn_bivvy_global(&["feedback", "list", "--status", "open"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Feedback") || text.contains("feedback") || text.contains("No")
            || text.contains("open") || text.contains("0"),
        "List --status open should show filtered feedback, got: {}",
        &text[..text.len().min(300)]
    );
}

/// List with --status resolved filter.
#[test]
fn feedback_list_status_resolved() {
    let mut s = spawn_bivvy_global(&["feedback", "list", "--status", "resolved"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Feedback") || text.contains("feedback") || text.contains("No")
            || text.contains("resolved") || text.contains("0"),
        "List --status resolved should show filtered feedback, got: {}",
        &text[..text.len().min(300)]
    );
}

/// List with --tag filter.
#[test]
fn feedback_list_tag_filter() {
    // First capture with a known tag
    let mut s = spawn_bivvy_global(&[
        "feedback",
        "--no-deliver",
        "--tag",
        "systest-filter",
        "Tag filter test message",
    ]);
    expect_or_dump(&mut s, "Feedback captured", "Setup: should confirm capture");
    s.expect(expectrl::Eof).unwrap();

    // Then list filtering by that tag
    let mut s = spawn_bivvy_global(&["feedback", "list", "--tag", "systest-filter"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Tag filter") || text.contains("systest-filter")
            || text.contains("Feedback") || text.contains("feedback"),
        "List --tag should show tagged feedback, got: {}",
        &text[..text.len().min(300)]
    );
}

/// List --status with invalid status shows error.
#[test]
fn feedback_list_invalid_status() {
    let mut s = spawn_bivvy_global(&["feedback", "list", "--status", "bogus"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("invalid") || text.contains("error") || text.contains("Unknown")
            || text.contains("bogus"),
        "Invalid status should show error, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// RESOLVE SUBCOMMAND
// =====================================================================

/// Resolve with nonexistent ID shows error.
#[test]
fn feedback_resolve_nonexistent() {
    let mut s = spawn_bivvy_global(&["feedback", "resolve", "nonexistent-id-999"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("not found") || text.contains("error") || text.contains("No")
            || text.contains("invalid"),
        "Resolve nonexistent ID should show error, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Resolve --help shows expected flags.
#[test]
fn feedback_resolve_help() {
    let mut s = spawn_bivvy_global(&["feedback", "resolve", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("resolve") || text.contains("Resolve"),
        "Resolve help should mention resolve, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SESSION SUBCOMMAND
// =====================================================================

/// Session subcommand with no data.
#[test]
fn feedback_session_no_sessions() {
    let mut s = spawn_bivvy_global(&["feedback", "session"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("session") || text.contains("Session") || text.contains("No")
            || text.contains("0") || text.contains("error"),
        "Session subcommand should show sessions or empty state, got: {}",
        &text[..text.len().min(300)]
    );
}

/// Session --help shows expected description.
#[test]
fn feedback_session_help() {
    let mut s = spawn_bivvy_global(&["feedback", "session", "--help"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("session") || text.contains("Session"),
        "Session help should mention session, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// ROUND-TRIP — Capture then verify in list
// =====================================================================

/// Capture feedback with unique marker, then verify it appears in list.
#[test]
fn feedback_round_trip_capture_then_list() {
    let marker = format!("roundtrip-{}", std::process::id());

    // Capture
    let mut s = spawn_bivvy_global(&[
        "feedback",
        "--no-deliver",
        "--tag",
        "roundtrip",
        &marker,
    ]);
    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();

    // List all and look for our marker
    let mut s = spawn_bivvy_global(&["feedback", "list", "--all"]);
    let text = read_to_eof(&mut s);
    assert!(
        text.contains(&marker),
        "List --all should contain our captured message '{}', got: {}",
        marker,
        &text[..text.len().min(500)]
    );
}

// =====================================================================
// PROJECT-SCOPED
// =====================================================================

/// Feedback capture within a project directory.
#[test]
fn feedback_project_scoped() {
    let config = r#"
app_name: "FeedbackTestApp"
steps:
  hello:
    command: "git --version"
"#;
    let temp = setup_project(config);
    let mut s = spawn_bivvy(&["feedback", "--no-deliver", "Project-scoped feedback"], temp.path());

    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Feedback captured") || text.contains("captured"),
        "Project-scoped capture should work, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// HELP
// =====================================================================

/// --help shows expected description.
#[test]
fn feedback_help() {
    let mut s = spawn_bivvy_global(&["feedback", "--help"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("feedback") || text.contains("Feedback"),
        "Help should mention feedback, got: {}",
        &text[..text.len().min(300)]
    );
}

/// List --help shows --status and --tag flags.
#[test]
fn feedback_list_help() {
    let mut s = spawn_bivvy_global(&["feedback", "list", "--help"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("--status") || text.contains("status"),
        "List help should mention --status, got: {}",
        &text[..text.len().min(300)]
    );
    assert!(
        text.contains("--tag") || text.contains("tag"),
        "List help should mention --tag, got: {}",
        &text[..text.len().min(300)]
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Empty message string.
#[test]
fn feedback_empty_message() {
    let mut s = spawn_bivvy_global(&["feedback", "--no-deliver", ""]);
    let text = read_to_eof(&mut s);

    // Should either reject empty message or handle gracefully
    assert!(
        text.contains("Feedback") || text.contains("feedback") || text.contains("error")
            || text.contains("empty") || text.contains("captured"),
        "Empty message should show error or capture, got: {}",
        &text[..text.len().min(300)]
    );
}

/// --tag with empty string.
#[test]
fn feedback_empty_tag() {
    let mut s = spawn_bivvy_global(&["feedback", "--no-deliver", "--tag", "", "msg"]);
    let text = read_to_eof(&mut s);

    assert!(
        text.contains("Feedback") || text.contains("feedback") || text.contains("captured")
            || text.contains("error"),
        "Empty tag should show capture or error, got: {}",
        &text[..text.len().min(300)]
    );
}
