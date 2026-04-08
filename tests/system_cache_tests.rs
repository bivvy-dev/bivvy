//! Comprehensive system tests for `bivvy cache`.
//!
//! Tests cache management subcommands: `list`, `stats`, `clear`, and their
//! flag variants.  Each test is isolated by setting `HOME` and
//! `XDG_CACHE_HOME` to a temp directory so the cache is deterministic.
//!
//! When a test needs a non-empty cache, it seeds entries directly through
//! `bivvy::cache::CacheStore` pointing at the same isolated directory the
//! spawned binary will read from.
#![cfg(unix)]

mod system;

use assert_cmd::cargo::cargo_bin;
use bivvy::cache::CacheStore;
use expectrl::Session;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use system::helpers::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Compute the cache directory that a child `bivvy` process will use when
/// spawned with the given `HOME` and `XDG_CACHE_HOME` set to `home`.
///
/// This mirrors `bivvy::cache::default_cache_dir()` on both macOS and
/// Linux so tests can seed entries the binary will actually read.
fn isolated_cache_dir(home: &Path) -> PathBuf {
    // On macOS `dirs::cache_dir()` returns `$HOME/Library/Caches`; on Linux
    // it reads `$XDG_CACHE_HOME` first, falling back to `$HOME/.cache`.
    // We set both env vars in `spawn_cache_isolated`, and since
    // `XDG_CACHE_HOME` takes precedence on Linux, we use it as the base
    // there.  On macOS `XDG_CACHE_HOME` is ignored, so we use
    // `Library/Caches` directly.
    #[cfg(target_os = "macos")]
    {
        home.join("Library")
            .join("Caches")
            .join("bivvy")
            .join("templates")
    }
    #[cfg(not(target_os = "macos"))]
    {
        home.join("bivvy").join("templates")
    }
}

/// Build a `CacheStore` pointing at the isolated cache dir for `home`.
fn isolated_store(home: &Path) -> CacheStore {
    let dir = isolated_cache_dir(home);
    std::fs::create_dir_all(&dir).unwrap();
    CacheStore::new(dir)
}

/// Spawn `bivvy` in a PTY with `HOME` and `XDG_CACHE_HOME` overridden.
///
/// This isolates the cache from the user's real `~/.cache/bivvy/` (or
/// `~/Library/Caches/bivvy/` on macOS), giving each test a
/// deterministically empty cache — or a deterministically seeded one
/// when combined with [`isolated_store`].
fn spawn_cache_isolated(args: &[&str], home: &Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.env("HOME", home);
    // On Linux, `dirs::cache_dir()` honours XDG_CACHE_HOME first. Point it
    // directly at the temp home so Linux tests are as isolated as macOS.
    cmd.env("XDG_CACHE_HOME", home);
    // Clear anything that could leak the real user's cache.
    cmd.env_remove("XDG_DATA_HOME");
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
}

/// Run `bivvy cache <subcommand>` non-interactively under the isolated
/// environment and return `(stdout, stderr, exit_code)`.
///
/// Used for help / error tests where we want deterministic captured
/// output suitable for snapshotting.
fn run_cache_isolated(args: &[&str], home: &Path) -> (String, String, i32) {
    let bin = cargo_bin("bivvy");
    let output = Command::new(bin)
        .args(args)
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home)
        .env_remove("XDG_DATA_HOME")
        .output()
        .expect("Failed to run bivvy");
    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

// =====================================================================
// HAPPY PATH — cache stats (empty)
// =====================================================================

/// `cache stats` on an empty isolated cache prints the header, every
/// documented field with zero values, a location line, and exits 0.
#[test]
fn cache_stats_empty_shows_all_fields_and_exits_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "stats"], home.path());

    // Full strings as emitted by `show_stats` in src/cli/commands/cache.rs.
    s.expect("Cache Statistics:").unwrap();
    s.expect("  Total entries: 0").unwrap();
    s.expect("  Fresh: 0").unwrap();
    s.expect("  Expired: 0").unwrap();
    s.expect("  Total size: 0 bytes").unwrap();
    s.expect("  Location: ").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — cache stats (seeded)
// =====================================================================

/// `cache stats` on a cache seeded with one fresh and one expired entry
/// reports both correctly, reports the combined byte total, and exits 0.
#[test]
fn cache_stats_with_fresh_and_expired_entries() {
    let home = TempDir::new().unwrap();

    // Seed: one fresh (TTL 3600), one expired (TTL 0).
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "rust-fresh", "12345", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "rust-old", "abc", 0)
        .unwrap();

    let mut s = spawn_cache_isolated(&["cache", "stats"], home.path());

    s.expect("Cache Statistics:").unwrap();
    s.expect("  Total entries: 2").unwrap();
    s.expect("  Fresh: 1").unwrap();
    s.expect("  Expired: 1").unwrap();
    // 5 + 3 = 8 bytes.
    s.expect("  Total size: 8 bytes").unwrap();
    s.expect("  Location: ").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — cache list (empty)
// =====================================================================

/// `cache list` on an empty cache prints the empty message and exits 0.
#[test]
fn cache_list_empty_shows_message_and_exits_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "list"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --verbose` on empty cache prints the empty message.
#[test]
fn cache_list_verbose_empty_exits_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "list", "--verbose"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --json` on empty cache prints the empty message.
#[test]
fn cache_list_json_empty_exits_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "list", "--json"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --verbose --json` on empty cache prints the empty message.
#[test]
fn cache_list_verbose_and_json_empty_exits_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "list", "--verbose", "--json"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — cache list (seeded)
// =====================================================================

/// `cache list` on a seeded cache prints one entry-count header line per
/// template and the fresh/expired status for each.
#[test]
fn cache_list_shows_seeded_entries_with_status() {
    let home = TempDir::new().unwrap();
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "cargo-setup", "content", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "git-hooks", "content", 0)
        .unwrap();

    let mut s = spawn_cache_isolated(&["cache", "list"], home.path());

    s.expect("2 cached entries:").unwrap();
    // Full non-verbose line format from src/cli/commands/cache.rs:
    //     "  {template_name} [{status}] {ttl}"
    // cargo-setup was seeded with 3600 s TTL so it is fresh; git-hooks with
    // 0 s TTL so it is expired and its ttl_str is literally "expired".
    s.expect("  cargo-setup [fresh] ").unwrap();
    s.expect("  git-hooks [expired] expired").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --verbose` on a seeded cache prints every documented
/// verbose field (Status, TTL, Size) for each entry.
#[test]
fn cache_list_verbose_shows_all_fields_for_seeded_entry() {
    let home = TempDir::new().unwrap();
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "rust-toolchain", "hello", 3600)
        .unwrap();

    let mut s = spawn_cache_isolated(&["cache", "list", "--verbose"], home.path());

    s.expect("1 cached entries:").unwrap();
    // Verbose format from src/cli/commands/cache.rs:
    //     "  {template_name} ({source_id})"
    //     "    Status: {status}"
    //     "    TTL: {ttl_str}"
    //     "    Size: {size_bytes} bytes"
    s.expect("  rust-toolchain (http:https://example.com)").unwrap();
    s.expect("    Status: fresh").unwrap();
    s.expect("    TTL: ").unwrap();
    s.expect("    Size: 5 bytes").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --json` on a seeded cache emits parseable JSON describing
/// the entries.  We parse it to catch structural regressions instead of
/// string matching.
#[test]
fn cache_list_json_output_is_parseable_and_correct() {
    let home = TempDir::new().unwrap();
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "alpha", "aaa", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "beta", "bbbbb", 3600)
        .unwrap();

    // Use non-PTY capture so we get clean stdout for JSON parsing.
    let (stdout, _stderr, code) =
        run_cache_isolated(&["cache", "list", "--json"], home.path());
    assert_eq!(code, 0, "cache list --json should exit 0, stdout: {stdout}");

    // Locate the JSON array in stdout (ui.message prefixes may add no
    // content but trailing newline is fine).
    let start = stdout.find('[').expect("JSON array not found in stdout");
    let end = stdout.rfind(']').expect("JSON array end not found in stdout");
    let json_slice = &stdout[start..=end];
    let parsed: serde_json::Value =
        serde_json::from_str(json_slice).expect("list --json must emit valid JSON");

    let arr = parsed.as_array().expect("JSON root must be an array");
    assert_eq!(arr.len(), 2, "expected exactly 2 entries");

    // Every entry must carry these documented fields.
    for entry in arr {
        assert!(entry.get("source_id").is_some(), "missing source_id");
        assert!(entry.get("template_name").is_some(), "missing template_name");
        assert!(entry.get("metadata").is_some(), "missing metadata block");
    }

    // Template names must match what we seeded.
    let names: Vec<&str> = arr
        .iter()
        .map(|e| e["template_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

// =====================================================================
// HAPPY PATH — cache clear
// =====================================================================

/// `cache clear --expired` on an empty cache reports 0 cleared entries
/// and exits 0.
#[test]
fn cache_clear_expired_on_empty_cache_reports_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "clear", "--expired"], home.path());

    s.expect("Cleared 0 expired entries").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache clear --expired` on a seeded cache removes only expired entries
/// and leaves fresh ones intact.
#[test]
fn cache_clear_expired_removes_only_expired() {
    let home = TempDir::new().unwrap();
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "keep-me", "fresh", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "drop-me", "stale", 0)
        .unwrap();

    let mut s = spawn_cache_isolated(&["cache", "clear", "--expired"], home.path());
    s.expect("Cleared 1 expired entries").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Verify side effect directly through the seeded store.
    let remaining = store.list().unwrap();
    assert_eq!(remaining.len(), 1, "only fresh entry should remain");
    assert_eq!(remaining[0].template_name, "keep-me");
}

/// `cache clear --force` on an empty cache reports already-empty and
/// exits 0.
#[test]
fn cache_clear_force_on_empty_cache_reports_already_empty() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "clear", "--force"], home.path());

    s.expect("Cache is already empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// Bare `cache clear` on an empty cache reports already-empty without
/// prompting and exits 0.
#[test]
fn cache_clear_bare_on_empty_cache_does_not_prompt() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_cache_isolated(&["cache", "clear"], home.path());

    s.expect("Cache is already empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache clear --force` on a seeded cache clears every entry without
/// prompting, reports the count, and exits 0.  The side effect is
/// verified by reading the store back.
#[test]
fn cache_clear_force_wipes_seeded_cache() {
    let home = TempDir::new().unwrap();
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "one", "x", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "two", "y", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "three", "z", 3600)
        .unwrap();

    let mut s = spawn_cache_isolated(&["cache", "clear", "--force"], home.path());
    s.expect("Cleared 3 entries").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Side effect: store should be empty.
    let remaining = store.list().unwrap();
    assert!(
        remaining.is_empty(),
        "cache clear --force should leave store empty, found {remaining:?}"
    );
}

// =====================================================================
// HAPPY PATH — sequential / multi-phase
// =====================================================================

/// After clearing expired on an empty cache, stats still report zero.
/// This verifies both phases independently (clear, then stats).
#[test]
fn cache_stats_after_clear_expired_shows_zero() {
    let home = TempDir::new().unwrap();

    // Phase 1: clear --expired
    let mut s = spawn_cache_isolated(&["cache", "clear", "--expired"], home.path());
    s.expect("Cleared 0 expired entries").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Phase 2: stats should still show zeros
    let mut s = spawn_cache_isolated(&["cache", "stats"], home.path());
    s.expect("Cache Statistics:").unwrap();
    s.expect("  Total entries: 0").unwrap();
    s.expect("  Fresh: 0").unwrap();
    s.expect("  Expired: 0").unwrap();
    s.expect("  Total size: 0 bytes").unwrap();
    s.expect("  Location: ").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// Full lifecycle against a seeded cache: `stats` shows the seeded count,
/// `clear --force` wipes it, and `list` then reports empty.
#[test]
fn cache_full_lifecycle_on_seeded_cache() {
    let home = TempDir::new().unwrap();
    let store = isolated_store(home.path());
    store
        .store("http:https://example.com", "lifecycle-a", "xx", 3600)
        .unwrap();
    store
        .store("http:https://example.com", "lifecycle-b", "yyyy", 3600)
        .unwrap();

    // 1. Stats: 2 entries, 6 bytes
    let mut s = spawn_cache_isolated(&["cache", "stats"], home.path());
    s.expect("Cache Statistics:").unwrap();
    s.expect("  Total entries: 2").unwrap();
    s.expect("  Fresh: 2").unwrap();
    s.expect("  Expired: 0").unwrap();
    s.expect("  Total size: 6 bytes").unwrap();
    s.expect("  Location: ").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // 2. Force clear
    let mut s = spawn_cache_isolated(&["cache", "clear", "--force"], home.path());
    s.expect("Cleared 2 entries").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // 3. List should report empty
    let mut s = spawn_cache_isolated(&["cache", "list"], home.path());
    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // 4. Side effect verified via the store.
    assert!(store.list().unwrap().is_empty());
}

// =====================================================================
// HELP — snapshot-based regression protection
// =====================================================================
//
// `system_global_tests.rs` already snapshots `bivvy cache --help`.  Here
// we cover each subcommand's help independently so renaming a flag or
// changing a description is caught.  NOTE: these snapshots are written
// but not generated by this audit — run `cargo insta test` to accept.

/// Snapshot of `bivvy cache list --help`.
#[test]
fn cache_list_help_snapshot() {
    let home = TempDir::new().unwrap();
    let (stdout, _stderr, code) =
        run_cache_isolated(&["cache", "list", "--help"], home.path());
    assert_eq!(code, 0, "cache list --help should exit 0");
    insta::assert_snapshot!("cache_list_help", stdout);
}

/// Snapshot of `bivvy cache clear --help`.
#[test]
fn cache_clear_help_snapshot() {
    let home = TempDir::new().unwrap();
    let (stdout, _stderr, code) =
        run_cache_isolated(&["cache", "clear", "--help"], home.path());
    assert_eq!(code, 0, "cache clear --help should exit 0");
    insta::assert_snapshot!("cache_clear_help", stdout);
}

/// Snapshot of `bivvy cache stats --help`.
#[test]
fn cache_stats_help_snapshot() {
    let home = TempDir::new().unwrap();
    let (stdout, _stderr, code) =
        run_cache_isolated(&["cache", "stats", "--help"], home.path());
    assert_eq!(code, 0, "cache stats --help should exit 0");
    insta::assert_snapshot!("cache_stats_help", stdout);
}

// =====================================================================
// SAD PATH — error conditions
// =====================================================================

/// `bivvy cache` with no subcommand is an arg-parsing error: clap
/// prints a Usage message to stderr and exits with code 2.  The full
/// stderr is snapshotted to catch wording and flag-list regressions.
#[test]
fn cache_no_subcommand_is_usage_error_with_exit_2() {
    let home = TempDir::new().unwrap();
    let (stdout, stderr, code) = run_cache_isolated(&["cache"], home.path());

    assert_eq!(
        code, 2,
        "missing subcommand should exit 2 (clap usage error), got {code}\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "clap usage errors should write to stderr only, got stdout: {stdout}"
    );
    insta::assert_snapshot!("cache_no_subcommand_stderr", stderr);
}

/// `bivvy cache frobnicate` (unknown subcommand) is a clap error:
/// "unrecognized subcommand" on stderr, exit code 2.
#[test]
fn cache_unknown_subcommand_is_usage_error_with_exit_2() {
    let home = TempDir::new().unwrap();
    let (stdout, stderr, code) = run_cache_isolated(&["cache", "frobnicate"], home.path());

    assert_eq!(
        code, 2,
        "unknown subcommand should exit 2 (clap usage error), got {code}\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "clap usage errors should write to stderr only, got stdout: {stdout}"
    );
    insta::assert_snapshot!("cache_unknown_subcommand_stderr", stderr);
}

/// `bivvy cache list --nope` must be rejected by clap with exit code 2,
/// not silently accepted.  Full stderr is snapshotted so that wording
/// and suggestion-list regressions are caught.
#[test]
fn cache_list_unknown_flag_is_usage_error_with_exit_2() {
    let home = TempDir::new().unwrap();
    let (stdout, stderr, code) =
        run_cache_isolated(&["cache", "list", "--nope"], home.path());

    assert_eq!(
        code, 2,
        "unknown flag should exit 2 (clap usage error), got {code}\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "clap usage errors should write to stderr only, got stdout: {stdout}"
    );
    insta::assert_snapshot!("cache_list_unknown_flag_stderr", stderr);
}
