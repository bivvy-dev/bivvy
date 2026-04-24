//! Comprehensive system tests for `bivvy feedback`.
//!
//! Tests interactive and quick feedback capture, tagging, listing,
//! subcommands (resolve, session), filtering (--status, --tag),
//! round-trip data verification, and error handling.
//!
//! Every spawn routes through the shared helpers in
//! `tests/system/helpers.rs`, which pin `HOME` and all four `XDG_*`
//! base-directory variables to an isolated tempdir.  The feedback
//! store lives under `dirs::data_local_dir()`, which honors
//! `XDG_DATA_HOME` on Linux and `$HOME/Library/Application Support` on
//! macOS — the shared `apply_home_isolation` covers both paths so no
//! bespoke environment plumbing is needed here.
//!
//! Tests that assert on an empty starting store own a per-test
//! `TempDir` and spawn via `spawn_bivvy_with_home` (rather than
//! `spawn_bivvy_global`, which uses a process-lifetime shared home and
//! would leak state between tests).
#![cfg(unix)]

mod system;

use system::helpers::*;
use tempfile::TempDir;

// =====================================================================
// HAPPY PATH — Interactive capture
// =====================================================================

/// Interactive mode prompts for category and message, and persists the entry.
#[test]
fn feedback_interactive_capture() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "--no-deliver"], home.path());

    // Accept default category
    expect_or_dump(&mut s, "What kind of feedback?", "Should prompt for category");
    s.send_line("").unwrap();

    // Enter feedback message
    expect_or_dump(
        &mut s,
        "Describe your feedback:",
        "Should prompt for feedback message",
    );
    s.send_line("PTY system test feedback").unwrap();

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: the entered message is actually stored.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("PTY system test feedback"),
        "List should contain the interactively-captured message, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("1 feedback entries:"),
        "List should show exactly one entry after one interactive capture, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// Interactive mode — use arrow keys to select a non-default category.
#[test]
fn feedback_interactive_arrow_select_category() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "--no-deliver"], home.path());

    expect_or_dump(&mut s, "What kind of feedback?", "Should prompt for category");
    // Arrow down to pick a different category, then Enter
    send_keys(&s, ARROW_DOWN);
    std::thread::sleep(std::time::Duration::from_millis(100));
    s.send_line("").unwrap();

    expect_or_dump(
        &mut s,
        "Describe your feedback:",
        "Should prompt for feedback message",
    );
    s.send_line("Arrow-key selected category").unwrap();

    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: the message is stored after arrow-key selection,
    // and the selected "ux" category was merged into the tag list.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Arrow-key selected category"),
        "List should contain the message captured after arrow-key selection, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("ux"),
        "One arrow-down should select the 'ux' category and persist it as a tag, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — Quick capture
// =====================================================================

/// Message as argument skips interactive prompts.
/// Verifies the captured message round-trips into the store via `list --all`.
#[test]
fn feedback_quick_capture_with_message() {
    let home = TempDir::new().unwrap();

    let mut s = spawn_bivvy_with_home(
        &["feedback", "--no-deliver", "Quick test feedback"],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: the entry is actually stored and visible in `list --all`.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("1 feedback entries:"),
        "List should show entry count header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("Quick test feedback"),
        "List should contain the captured quick message, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// Quick capture with multiple tags — verify each tag is persisted and visible.
#[test]
fn feedback_with_tags() {
    let home = TempDir::new().unwrap();

    let mut s = spawn_bivvy_with_home(
        &[
            "feedback",
            "--no-deliver",
            "--tag",
            "bug,testing",
            "Tagged feedback message",
        ],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: both tags are persisted on the stored entry.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Tagged feedback message"),
        "List should contain the captured message, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("bug") && text.contains("testing"),
        "List should show both tags 'bug' and 'testing', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// Quick capture with a single tag — verify tag persistence via `list --tag`.
#[test]
fn feedback_with_single_tag() {
    let home = TempDir::new().unwrap();

    let mut s = spawn_bivvy_with_home(
        &[
            "feedback",
            "--no-deliver",
            "--tag",
            "enhancement",
            "Enhancement feedback",
        ],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: the entry can be found when filtering by that exact tag.
    let mut s = spawn_bivvy_with_home(
        &["feedback", "list", "--tag", "enhancement"],
        home.path(),
    );
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Enhancement feedback"),
        "List --tag enhancement should contain the captured message, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("enhancement"),
        "List --tag enhancement should show the tag label, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// Quick capture with --session flag using a well-formed session ID —
/// the success message should include the session suffix and the entry
/// should round-trip into the store with the session attached.
///
/// Note: `SessionId::parse` expects the `sess_<millis>_<hex>` format;
/// invalid IDs are silently dropped, so this test passes a valid one.
#[test]
fn feedback_with_session_flag() {
    let home = TempDir::new().unwrap();
    // Matches the `sess_<unix-ms>_<16-hex>` (8-byte random) format
    // enforced by `SessionId::parse` in `src/session/id.rs`.
    let session_id = "sess_1700000000000_abcdef0123456789";

    let mut s = spawn_bivvy_with_home(
        &[
            "feedback",
            "--no-deliver",
            "--session",
            session_id,
            "Session-scoped feedback",
        ],
        home.path(),
    );
    // The success line is `Feedback captured (session <id>)` when a
    // session is attached, per `capture_feedback` in feedback.rs.
    expect_or_dump(
        &mut s,
        &format!("Feedback captured (session {})", session_id),
        "Should confirm capture with session suffix",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: the captured entry is persisted, visible in
    // `list --all`, and annotated with the session suffix used at capture.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Session-scoped feedback"),
        "List should contain the session-scoped message, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains(session_id),
        "List should show the attached session id '{}', got: {}",
        session_id,
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// LIST SUBCOMMAND
// =====================================================================

/// List subcommand shows captured feedback.
#[test]
fn feedback_list() {
    let home = TempDir::new().unwrap();

    // Capture first
    let mut s = spawn_bivvy_with_home(&["feedback", "--no-deliver", "List test entry"], home.path());
    expect_or_dump(&mut s, "Feedback captured", "Setup: should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Then list
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("1 feedback entries:"),
        "List should show entry count header, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("List test entry"),
        "List should contain captured message, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// List with no feedback yet should show empty message.
#[test]
fn feedback_list_empty() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "list"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No feedback entries found"),
        "Empty list should show 'No feedback entries found', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// List with --status open filter on an empty store shows the empty message.
#[test]
fn feedback_list_status_open_empty() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--status", "open"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No feedback entries found"),
        "List --status open with no data should show 'No feedback entries found', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// `--status open` returns freshly captured (still-open) entries, while
/// `--status resolved` hides them. This verifies the filter actually
/// differentiates by status rather than just tolerating the flag.
#[test]
fn feedback_list_status_filter_differentiates() {
    let home = TempDir::new().unwrap();

    // Seed one open entry.
    let mut s = spawn_bivvy_with_home(
        &["feedback", "--no-deliver", "Status filter test entry"],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Setup: should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // --status open should show the new entry.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--status", "open"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Status filter test entry"),
        "List --status open should include the open entry, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("1 feedback entries:"),
        "List --status open should show exactly one open entry, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);

    // --status resolved should NOT show it (the entry is still open).
    let mut s = spawn_bivvy_with_home(
        &["feedback", "list", "--status", "resolved"],
        home.path(),
    );
    let text = read_to_eof(&mut s);
    assert!(
        !text.contains("Status filter test entry"),
        "List --status resolved should not include open entries, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("No feedback entries found"),
        "List --status resolved with only open entries should be empty, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// List with --tag filter shows matching entries.
#[test]
fn feedback_list_tag_filter() {
    let home = TempDir::new().unwrap();

    // First capture with a known tag
    let mut s = spawn_bivvy_with_home(
        &[
            "feedback",
            "--no-deliver",
            "--tag",
            "systest-filter",
            "Tag filter test message",
        ],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Setup: should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Then list filtering by that tag
    let mut s = spawn_bivvy_with_home(
        &["feedback", "list", "--tag", "systest-filter"],
        home.path(),
    );
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Tag filter test message"),
        "List --tag should show the captured message, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("systest-filter"),
        "List --tag should show the tag, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// List --status with invalid status shows error.
#[test]
fn feedback_list_invalid_status() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--status", "bogus"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Unknown status: bogus"),
        "Invalid status should show 'Unknown status: bogus', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 1);
}

// =====================================================================
// RESOLVE SUBCOMMAND
// =====================================================================

/// Resolve a real feedback entry: capture, discover its ID from the
/// list output, resolve it with a note, and verify the status icon
/// transitions from `[ ]` to `[x]` on subsequent `list --all`.
#[test]
fn feedback_resolve_happy_path() {
    let home = TempDir::new().unwrap();

    // Capture an entry we'll later resolve.
    let mut s = spawn_bivvy_with_home(
        &["feedback", "--no-deliver", "Entry to resolve"],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Setup: should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Discover the entry's ID by parsing `list --all` output. Entries are
    // printed as "<icon> <id> <message>..." and IDs are `fb_<hex>`.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let listing = read_to_eof(&mut s);
    assert_exit_code(&s, 0);
    assert!(
        listing.contains("[ ] fb_"),
        "Newly captured entry should show open status icon '[ ]' and an fb_ ID, got: {}",
        &listing[..listing.len().min(500)]
    );
    let id = listing
        .split_whitespace()
        .find(|tok| tok.starts_with("fb_"))
        .expect("Expected an fb_<hex> ID in list output")
        .to_string();

    // Resolve with a note.
    let mut s = spawn_bivvy_with_home(
        &["feedback", "resolve", &id, "--note", "Fixed in test"],
        home.path(),
    );
    let text = read_to_eof(&mut s);
    assert!(
        text.contains(&format!("Resolved {}", id)),
        "Resolve should print 'Resolved <id>', got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);

    // Verify the status transitioned: default list (open-only) should now
    // be empty, and `list --all` should show the resolved icon `[x]`.
    let mut s = spawn_bivvy_with_home(&["feedback", "list"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No feedback entries found"),
        "Default list (open-only) should be empty after resolve, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);

    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("[x]") && text.contains(&id),
        "list --all after resolve should show '[x] <id>' for the resolved entry, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

/// Resolve with nonexistent ID shows error.
#[test]
fn feedback_resolve_nonexistent() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "resolve", "nonexistent-id-999"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Feedback nonexistent-id-999 not found"),
        "Resolve nonexistent ID should show 'Feedback nonexistent-id-999 not found', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 1);
}

/// Resolve --help shows expected content (snapshot).
#[test]
fn feedback_resolve_help() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "resolve", "--help"], home.path());
    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("feedback_resolve_help", text);
    assert_exit_code(&s, 0);
}

// =====================================================================
// SESSION SUBCOMMAND
// =====================================================================

/// Session subcommand with no sessions captured shows the documented message.
#[test]
fn feedback_session_no_sessions() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "session"], home.path());
    let text = read_to_eof(&mut s);
    // With a fresh HOME there are no sessions at all, so this must be the
    // "no sessions found" path, not the "no feedback for session X" path.
    assert!(
        text.contains("No sessions found"),
        "Session subcommand with no data should show 'No sessions found', got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// Session --help shows expected content (snapshot).
#[test]
fn feedback_session_help() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "session", "--help"], home.path());
    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("feedback_session_help", text);
    assert_exit_code(&s, 0);
}

// =====================================================================
// ROUND-TRIP — Capture then verify in list
// =====================================================================

/// Capture feedback with unique marker, then verify it appears in list.
#[test]
fn feedback_round_trip_capture_then_list() {
    let home = TempDir::new().unwrap();
    let marker = format!("roundtrip-{}", std::process::id());

    // Capture
    let mut s = spawn_bivvy_with_home(
        &["feedback", "--no-deliver", "--tag", "roundtrip", &marker],
        home.path(),
    );
    expect_or_dump(&mut s, "Feedback captured", "Should confirm capture");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // List all and look for our marker
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains(&marker),
        "List --all should contain our captured message '{}', got: {}",
        marker,
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// PROJECT-SCOPED
// =====================================================================

/// Feedback capture within a project directory persists the entry.
///
/// Uses a realistic config that exercises a real tool (`rustc --version`) as a
/// completed check rather than `echo`/`true`/other shell builtins.
#[test]
fn feedback_project_scoped() {
    let config = r#"
app_name: "FeedbackTestApp"
steps:
  check_rust:
    command: "rustc --version"
    completed_check:
      type: command_succeeds
      command: "rustc --version"
workflows:
  default:
    steps: [check_rust]
"#;
    let temp = setup_project(config);
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_env(
        &["feedback", "--no-deliver", "Project-scoped feedback"],
        temp.path(),
        &[("HOME", home.path().to_str().unwrap())],
    );

    expect_or_dump(
        &mut s,
        "Feedback captured",
        "Project-scoped capture should confirm with 'Feedback captured'",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect: the project-scoped capture is persisted and visible
    // from within the project directory.
    let mut s = spawn_bivvy_with_env(
        &["feedback", "list", "--all"],
        temp.path(),
        &[("HOME", home.path().to_str().unwrap())],
    );
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("Project-scoped feedback"),
        "Project-scoped list should contain the captured message, got: {}",
        &text[..text.len().min(500)]
    );
    assert!(
        text.contains("1 feedback entries:"),
        "Project-scoped list should show exactly one entry, got: {}",
        &text[..text.len().min(500)]
    );
    assert_exit_code(&s, 0);
}

// =====================================================================
// HELP (snapshots)
// =====================================================================

/// --help shows expected description (snapshot).
#[test]
fn feedback_help() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "--help"], home.path());
    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("feedback_help", text);
    assert_exit_code(&s, 0);
}

/// List --help shows --status, --tag, and --all flags (snapshot).
#[test]
fn feedback_list_help() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--help"], home.path());
    let text = read_to_eof(&mut s);
    insta::assert_snapshot!("feedback_list_help", text);
    assert_exit_code(&s, 0);
}

// =====================================================================
// SAD PATH
// =====================================================================

/// Empty message string in interactive mode should warn and exit non-zero.
///
/// `capture_interactive` in `src/cli/commands/feedback.rs` emits the
/// "No feedback provided" warning and returns exit code 1 when the user
/// submits an empty feedback message.
#[test]
fn feedback_empty_message_interactive() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["feedback", "--no-deliver"], home.path());

    // Accept default category
    expect_or_dump(&mut s, "What kind of feedback?", "Should prompt for category");
    s.send_line("").unwrap();

    // Send empty message
    expect_or_dump(
        &mut s,
        "Describe your feedback:",
        "Should prompt for feedback message",
    );
    s.send_line("").unwrap();

    expect_or_dump(
        &mut s,
        "No feedback provided",
        "Empty message should show 'No feedback provided' warning",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 1);

    // Verify side effect: no entry was stored despite the attempt.
    let mut s = spawn_bivvy_with_home(&["feedback", "list", "--all"], home.path());
    let text = read_to_eof(&mut s);
    assert!(
        text.contains("No feedback entries found"),
        "Empty-message capture should not persist an entry, got: {}",
        &text[..text.len().min(300)]
    );
    assert_exit_code(&s, 0);
}

/// --tag with empty string still captures feedback.
#[test]
fn feedback_empty_tag() {
    let home = TempDir::new().unwrap();
    let mut s =
        spawn_bivvy_with_home(&["feedback", "--no-deliver", "--tag", "", "msg"], home.path());

    expect_or_dump(
        &mut s,
        "Feedback captured",
        "Empty tag should still capture feedback successfully",
    );
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}
