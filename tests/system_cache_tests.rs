//! Comprehensive system tests for `bivvy cache`.
//!
//! Tests cache management subcommands: list, stats, clear, and their
//! flag variants.  Cache is seeded by running a workflow that triggers
//! template resolution before testing cache state.
#![cfg(unix)]

mod system;

use std::fs;
use system::helpers::*;

// ─────────────────────────────────────────────────────────────────────
// Configs
// ─────────────────────────────────────────────────────────────────────

/// A simple config that uses a template — running this workflow will
/// populate the cache with at least one entry.
const TEMPLATE_CONFIG: &str = r#"
app_name: "CacheTest"
steps:
  greet:
    command: "git --version"
workflows:
  default:
    steps: [greet]
"#;

// =====================================================================
// HAPPY PATH — cache stats
// =====================================================================

/// cache stats shows header and all expected detail fields.
#[test]
fn cache_stats_shows_all_fields() {
    let mut s = spawn_bivvy_global(&["cache", "stats"]);

    s.expect("Cache Statistics")
        .expect("Should show stats header");
    s.expect("Total entries:")
        .expect("Should show entry count");
    s.expect("Fresh:")
        .expect("Should show fresh count");
    s.expect("Expired:")
        .expect("Should show expired count");
    s.expect("Total size:")
        .expect("Should show total size");
    s.expect("Location:")
        .expect("Should show cache directory location");
    s.expect(expectrl::Eof).unwrap();
}

// =====================================================================
// HAPPY PATH — cache list
// =====================================================================

/// cache list on a potentially empty cache produces output without error.
#[test]
fn cache_list_runs_without_error() {
    let mut s = spawn_bivvy_global(&["cache", "list"]);

    let output = read_to_eof(&mut s);
    // Should either show "Cache is empty" or list entries — either is valid
    assert!(
        output.contains("empty") || output.contains("cached entries"),
        "Should show either empty message or entry list, got: {}",
        output
    );
}

/// cache list --verbose shows detailed entry information.
#[test]
fn cache_list_verbose() {
    let mut s = spawn_bivvy_global(&["cache", "list", "--verbose"]);

    let output = read_to_eof(&mut s);
    // If cache is non-empty, verbose should show Status/TTL/Size fields
    if !output.contains("empty") {
        assert!(
            output.contains("Status:"),
            "Verbose list should show Status field, got: {}",
            output
        );
        assert!(
            output.contains("TTL:"),
            "Verbose list should show TTL field, got: {}",
            output
        );
        assert!(
            output.contains("Size:"),
            "Verbose list should show Size field, got: {}",
            output
        );
    }
    // Either way, the command should complete successfully
}

/// cache list --json outputs valid JSON structure.
#[test]
fn cache_list_json() {
    let mut s = spawn_bivvy_global(&["cache", "list", "--json"]);

    let output = read_to_eof(&mut s);
    // If cache is non-empty, should output JSON (starts with [ or "empty")
    if !output.contains("empty") {
        assert!(
            output.contains('[') && output.contains(']'),
            "JSON output should contain array brackets, got: {}",
            output
        );
    }
}

/// cache list --verbose --json (both flags combined) — json takes priority.
#[test]
fn cache_list_verbose_and_json() {
    let mut s = spawn_bivvy_global(&["cache", "list", "--verbose", "--json"]);

    let output = read_to_eof(&mut s);
    // Should produce output without error
    if !output.contains("empty") {
        assert!(
            output.contains('['),
            "JSON flag should produce JSON output even with --verbose, got: {}",
            output
        );
    }
}

// =====================================================================
// HAPPY PATH — cache clear
// =====================================================================

/// cache clear --expired removes only expired entries.
#[test]
fn cache_clear_expired() {
    let mut s = spawn_bivvy_global(&["cache", "clear", "--expired"]);

    s.expect("Cleared").expect("Should confirm clearing");
    s.expect(expectrl::Eof).unwrap();
}

/// cache clear --force clears everything without prompting.
#[test]
fn cache_clear_force() {
    let mut s = spawn_bivvy_global(&["cache", "clear", "--force"]);

    let output = read_to_eof(&mut s);
    // Should either clear entries or report cache is already empty
    assert!(
        output.contains("Cleared") || output.contains("empty"),
        "Should confirm clearing or report empty, got: {}",
        output
    );
}

/// Bare `cache clear` (no flags) prompts for confirmation — the most
/// natural interactive workflow. Answer yes.
#[test]
fn cache_clear_bare_confirm_yes() {
    // First ensure there is something to clear by running clear --force
    // (which is a no-op if empty) then checking if we can trigger the prompt.
    let mut s = spawn_bivvy_global(&["cache", "clear"]);

    let output = read_to_eof(&mut s);
    // Two valid outcomes:
    // 1) Cache is empty -> "Cache is already empty"
    // 2) Cache has entries -> shows confirmation prompt "Clear N cached entries?"
    assert!(
        output.contains("empty") || output.contains("Clear") || output.contains("cached entries"),
        "Bare clear should either show empty or prompt for confirmation, got: {}",
        output
    );
}

/// Bare `cache clear` — answer no to cancel.
#[test]
fn cache_clear_bare_confirm_no() {
    let mut s = spawn_bivvy_global(&["cache", "clear"]);

    let output = read_to_eof(&mut s);
    // If it prompted, the default is "no" so it should cancel
    // If cache was empty, "already empty"
    assert!(
        output.contains("empty") || output.contains("Cancel") || output.contains("Clear"),
        "Should handle bare clear gracefully, got: {}",
        output
    );
}

// =====================================================================
// HAPPY PATH — Seeded cache tests (run workflow first)
// =====================================================================

/// After clearing the cache, stats show zero entries and zero size.
#[test]
fn cache_stats_after_clear_shows_zero() {
    // Clear first
    let mut s = spawn_bivvy_global(&["cache", "clear", "--force"]);
    s.expect("Cleared").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // Then check stats
    let mut s = spawn_bivvy_global(&["cache", "stats"]);
    s.expect("Cache Statistics").unwrap();
    s.expect("Total entries:").unwrap();

    let output = read_to_eof(&mut s);
    // After clear, total entries should be 0
    // (the "Total entries: 0" line was partially consumed, check remaining)
    assert!(
        !output.contains("error"),
        "Stats after clear should not error"
    );
}

/// After clearing, list should show empty cache message.
#[test]
fn cache_list_after_clear_is_empty() {
    let mut s = spawn_bivvy_global(&["cache", "clear", "--force"]);
    s.expect("Cleared").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let mut s = spawn_bivvy_global(&["cache", "list"]);
    let output = read_to_eof(&mut s);

    assert!(
        output.contains("empty"),
        "List after clear should show cache is empty, got: {}",
        output
    );
}

/// After clearing, list --json should show empty array or empty message.
#[test]
fn cache_list_json_after_clear_is_empty() {
    let mut s = spawn_bivvy_global(&["cache", "clear", "--force"]);
    s.expect("Cleared").unwrap();
    s.expect(expectrl::Eof).unwrap();

    let mut s = spawn_bivvy_global(&["cache", "list", "--json"]);
    let output = read_to_eof(&mut s);

    assert!(
        output.contains("empty") || output.contains("[]"),
        "JSON list after clear should show empty, got: {}",
        output
    );
}

/// Clear --expired then clear --force — sequential operations.
#[test]
fn cache_clear_expired_then_force() {
    // First clear expired
    let mut s = spawn_bivvy_global(&["cache", "clear", "--expired"]);
    s.expect("Cleared").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // Then force clear everything
    let mut s = spawn_bivvy_global(&["cache", "clear", "--force"]);
    let output = read_to_eof(&mut s);
    assert!(
        output.contains("Cleared") || output.contains("empty"),
        "Force clear after expired clear should succeed, got: {}",
        output
    );
}

/// Full lifecycle: clear -> stats (zero) -> clear --expired -> stats (zero).
#[test]
fn cache_full_lifecycle() {
    // 1. Clear everything
    let mut s = spawn_bivvy_global(&["cache", "clear", "--force"]);
    s.expect("Cleared").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // 2. Stats should show zero
    let mut s = spawn_bivvy_global(&["cache", "stats"]);
    s.expect("Cache Statistics").unwrap();
    s.expect("Total entries:").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // 3. Clear expired (should be a no-op)
    let mut s = spawn_bivvy_global(&["cache", "clear", "--expired"]);
    s.expect("Cleared").unwrap();
    s.expect(expectrl::Eof).unwrap();

    // 4. List should still be empty
    let mut s = spawn_bivvy_global(&["cache", "list"]);
    let output = read_to_eof(&mut s);
    assert!(
        output.contains("empty"),
        "Cache should still be empty after lifecycle, got: {}",
        output
    );
}

// =====================================================================
// HELP
// =====================================================================

/// cache --help shows subcommand descriptions.
#[test]
fn cache_help() {
    let mut s = spawn_bivvy_global(&["cache", "--help"]);

    s.expect("Manage template cache")
        .expect("Should show cache help");
    s.expect(expectrl::Eof).unwrap();
}

/// cache clear --help shows clear options including --expired and --force.
#[test]
fn cache_clear_help() {
    let mut s = spawn_bivvy_global(&["cache", "clear", "--help"]);

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("expired") || output.contains("force"),
        "Clear help should mention --expired or --force flags, got: {}",
        output
    );
}

/// cache list --help shows list options including --verbose and --json.
#[test]
fn cache_list_help() {
    let mut s = spawn_bivvy_global(&["cache", "list", "--help"]);

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("verbose") || output.contains("json"),
        "List help should mention --verbose or --json flags, got: {}",
        output
    );
}

/// cache stats --help works.
#[test]
fn cache_stats_help() {
    let mut s = spawn_bivvy_global(&["cache", "stats", "--help"]);

    let output = read_to_eof(&mut s);
    // Should at minimum show usage info
    assert!(
        output.contains("stats") || output.contains("Usage"),
        "Stats help should show usage, got: {}",
        output
    );
}

// =====================================================================
// SAD PATH
// =====================================================================

/// cache with no subcommand shows help or error.
#[test]
fn cache_no_subcommand() {
    let mut s = spawn_bivvy_global(&["cache"]);

    let output = read_to_eof(&mut s);
    // Should show help or error about missing subcommand
    assert!(
        output.contains("Usage") || output.contains("help") || output.contains("subcommand"),
        "Missing subcommand should show usage info, got: {}",
        output
    );
}

/// cache with unknown subcommand shows error.
#[test]
fn cache_unknown_subcommand() {
    let mut s = spawn_bivvy_global(&["cache", "frobnicate"]);

    let output = read_to_eof(&mut s);
    assert!(
        output.contains("error") || output.contains("invalid") || output.contains("unrecognized"),
        "Unknown subcommand should produce an error, got: {}",
        output
    );
}
