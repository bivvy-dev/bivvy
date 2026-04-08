//! System tests for `bivvy cache`.
//!
//! Verifies cache management subcommands (`list`, `stats`, `clear`) and their
//! flag variants with full content assertions, exit-code checks, and
//! deterministic state.
//!
//! Every test isolates HOME (and XDG_CACHE_HOME) to a per-test `TempDir`, so
//! the cache is a known state at the start of each test.  Populated-cache
//! tests seed the cache directly via `bivvy::cache::CacheStore` before
//! spawning the CLI, which lets us assert on real entries instead of
//! branching on "maybe the cache has stuff in it".
#![cfg(unix)]

mod system;

use bivvy::cache::CacheStore;
use predicates::prelude::PredicateBooleanExt;
use std::path::{Path, PathBuf};
use system::helpers::*;
use tempfile::TempDir;

// =====================================================================
// Helpers
// =====================================================================
//
// Spawns route through the shared `spawn_bivvy_with_home` /
// `bivvy_assert_cmd_with_home` helpers, which pin `HOME` and all four
// `XDG_*` base-dir variables to a caller-supplied tempdir via the
// shared `apply_home_isolation*` utilities.  Cache tests need a
// per-test home (rather than the process-lifetime home used by
// `spawn_bivvy_global`) because they seed the `CacheStore` directly
// via the library API before spawning, which requires knowing the
// exact path bivvy will resolve.

/// Compute the cache directory that `bivvy` will use when `HOME` is
/// set to `home`.  Mirrors `dirs::cache_dir()` behaviour on both Linux
/// (via `XDG_CACHE_HOME`, which the shared helpers pin to
/// `<home>/.cache`) and macOS (which always uses
/// `$HOME/Library/Caches`) so we can seed the store directly.
fn cache_dir_for_home(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Caches/bivvy/templates")
    }
    #[cfg(not(target_os = "macos"))]
    {
        home.join(".cache/bivvy/templates")
    }
}

/// Seed an isolated cache with a small set of fresh entries, simulating
/// what a real template fetch would produce.
fn seed_cache_with_fresh_entries(home: &Path) {
    let cache_dir = cache_dir_for_home(home);
    std::fs::create_dir_all(&cache_dir).unwrap();
    let store = CacheStore::new(&cache_dir);

    // Two fresh entries representing realistic templates fetched from a
    // git source (the kind `bivvy` actually caches).
    store
        .store(
            "git:github.com/bivvy-dev/templates",
            "rust-toolchain",
            "rust_version: \"1.93\"\n",
            3600,
        )
        .unwrap();
    store
        .store(
            "git:github.com/bivvy-dev/templates",
            "cargo-install",
            "command: \"cargo install --locked\"\n",
            3600,
        )
        .unwrap();
}

/// Seed an isolated cache with one expired entry and one fresh entry.
fn seed_cache_with_mixed_entries(home: &Path) {
    let cache_dir = cache_dir_for_home(home);
    std::fs::create_dir_all(&cache_dir).unwrap();
    let store = CacheStore::new(&cache_dir);

    // Expired entry (TTL = 0 → immediately expired).
    store
        .store(
            "git:github.com/bivvy-dev/templates",
            "node-setup",
            "command: \"node --version\"\n",
            0,
        )
        .unwrap();
    // Fresh entry.
    store
        .store(
            "git:github.com/bivvy-dev/templates",
            "python-venv",
            "command: \"python3 -m venv .venv\"\n",
            3600,
        )
        .unwrap();
}

// =====================================================================
// HAPPY PATH — cache stats (empty cache)
// =====================================================================

/// `cache stats` on an empty cache shows the full header and all fields
/// with zero values, and exits 0.
#[test]
fn cache_stats_empty_shows_all_fields_and_zeros() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "stats"], home.path());

    s.expect("Cache Statistics:").unwrap();
    s.expect("Total entries: 0").unwrap();
    s.expect("Fresh: 0").unwrap();
    s.expect("Expired: 0").unwrap();
    s.expect("Total size: 0 bytes").unwrap();
    s.expect("Location:").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — cache stats (populated cache)
// =====================================================================

/// `cache stats` on a cache populated with two fresh entries reports the
/// exact counts and a non-zero size.
#[test]
fn cache_stats_populated_reports_counts_and_size() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cache Statistics:"))
        .stdout(predicates::str::contains("Total entries: 2"))
        .stdout(predicates::str::contains("Fresh: 2"))
        .stdout(predicates::str::contains("Expired: 0"));
}

/// `cache stats` on a cache with one expired and one fresh entry reports
/// the split correctly.
#[test]
fn cache_stats_mixed_reports_fresh_and_expired_counts() {
    let home = TempDir::new().unwrap();
    seed_cache_with_mixed_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Total entries: 2"))
        .stdout(predicates::str::contains("Fresh: 1"))
        .stdout(predicates::str::contains("Expired: 1"));
}

// =====================================================================
// HAPPY PATH — cache list (empty)
// =====================================================================

/// `cache list` on an empty cache shows exactly "Cache is empty" and
/// exits 0.
#[test]
fn cache_list_empty_shows_empty_message() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "list"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --verbose` on an empty cache still shows "Cache is empty".
#[test]
fn cache_list_verbose_empty_shows_empty_message() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "list", "--verbose"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache list --json` on an empty cache shows "Cache is empty" and
/// exits 0 (matches documented behaviour: empty cache is a user message,
/// not an empty JSON array).
#[test]
fn cache_list_json_empty_shows_empty_message() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "list", "--json"], home.path());

    s.expect("Cache is empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// =====================================================================
// HAPPY PATH — cache list (populated)
// =====================================================================

/// `cache list` on a populated cache shows the count header and every
/// seeded template name in the default (non-verbose) format.
#[test]
fn cache_list_populated_shows_entries() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("2 cached entries:"))
        .stdout(predicates::str::contains("rust-toolchain"))
        .stdout(predicates::str::contains("cargo-install"))
        .stdout(predicates::str::contains("[fresh]"));
}

/// `cache list --verbose` on a populated cache shows the Status, TTL,
/// and Size fields for each entry.
#[test]
fn cache_list_verbose_populated_shows_detail_fields() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list", "--verbose"])
        .assert()
        .success()
        .stdout(predicates::str::contains("rust-toolchain"))
        .stdout(predicates::str::contains("cargo-install"))
        .stdout(predicates::str::contains("Status: fresh"))
        .stdout(predicates::str::contains("TTL:"))
        .stdout(predicates::str::contains("Size:"))
        .stdout(predicates::str::contains("bytes"));
}

/// `cache list --verbose` on a mixed cache shows both "Status: fresh"
/// and "Status: expired".
#[test]
fn cache_list_verbose_mixed_shows_both_statuses() {
    let home = TempDir::new().unwrap();
    seed_cache_with_mixed_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list", "--verbose"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Status: fresh"))
        .stdout(predicates::str::contains("Status: expired"));
}

/// `cache list --json` on a populated cache produces parseable JSON
/// whose structure matches what downstream tooling would expect.  We
/// parse and snapshot a normalised view so timestamp jitter doesn't
/// invalidate the snapshot.
#[test]
fn cache_list_json_populated_is_parseable_and_structured() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    let output = bivvy_assert_cmd_with_home(home.path()).args(["cache", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    // Parse the JSON output.
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("cache list --json should emit valid JSON");

    // Must be an array of exactly the two seeded entries.
    let arr = parsed.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 2, "Expected 2 cached entries, got {}", arr.len());

    let names: Vec<&str> = arr
        .iter()
        .map(|e| e["template_name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"rust-toolchain"),
        "Expected rust-toolchain in output, got {names:?}"
    );
    assert!(
        names.contains(&"cargo-install"),
        "Expected cargo-install in output, got {names:?}"
    );

    // Every entry has the documented top-level fields.
    for entry in arr {
        assert!(entry.get("source_id").is_some(), "missing source_id");
        assert!(entry.get("template_name").is_some(), "missing template_name");
        assert!(entry.get("metadata").is_some(), "missing metadata");
    }

    // Snapshot just the stable keys so future schema changes are caught.
    let normalised: Vec<serde_json::Value> = arr
        .iter()
        .map(|e| {
            serde_json::json!({
                "source_id": e["source_id"],
                "template_name": e["template_name"],
                "size_bytes": e["metadata"]["size_bytes"],
            })
        })
        .collect();
    // Sort for stable ordering (list() sorts by cached_at which may tie).
    let mut normalised = normalised;
    normalised.sort_by(|a, b| {
        a["template_name"]
            .as_str()
            .cmp(&b["template_name"].as_str())
    });
    insta::assert_json_snapshot!("cache_list_json_populated", normalised);
}

// =====================================================================
// HAPPY PATH — cache clear
// =====================================================================

/// `cache clear --expired` on an empty cache reports the exact "Cleared
/// 0 expired entries" message and exits 0.
#[test]
fn cache_clear_expired_empty_reports_zero() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "clear", "--expired"], home.path());

    s.expect("Cleared 0 expired entries").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache clear --expired` on a mixed cache removes only the expired
/// entry and leaves the fresh one behind.
#[test]
fn cache_clear_expired_mixed_removes_only_expired() {
    let home = TempDir::new().unwrap();
    seed_cache_with_mixed_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "clear", "--expired"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cleared 1 expired entries"));

    // Verify the side effect: the fresh entry is still there.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Total entries: 1"))
        .stdout(predicates::str::contains("Fresh: 1"))
        .stdout(predicates::str::contains("Expired: 0"));
}

/// `cache clear --force` on an empty cache reports "Cache is already
/// empty" without prompting and exits 0.
#[test]
fn cache_clear_force_empty_reports_already_empty() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "clear", "--force"], home.path());

    s.expect("Cache is already empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

/// `cache clear --force` on a populated cache removes everything without
/// prompting and reports the exact cleared count.
#[test]
fn cache_clear_force_populated_clears_all() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    bivvy_assert_cmd_with_home(home.path()).args(["cache", "clear", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cleared 2 entries"));

    // Verify the side effect: cache is now empty.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cache is empty"));
}

/// Bare `cache clear` on an empty cache reports "Cache is already
/// empty" without prompting (nothing to confirm).
#[test]
fn cache_clear_bare_empty_reports_already_empty() {
    let home = TempDir::new().unwrap();
    let mut s = spawn_bivvy_with_home(&["cache", "clear"], home.path());

    s.expect("Cache is already empty").unwrap();
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);
}

// =====================================================================
// INTERACTIVE — cache clear prompt
// =====================================================================

/// Bare `cache clear` on a populated cache prompts the user, and
/// declining (n) cancels without clearing.
#[test]
fn cache_clear_bare_populated_decline_cancels() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    let mut s = spawn_bivvy_with_home(&["cache", "clear"], home.path());
    wait_and_answer(&s, "Clear 2 cached entries?", KEY_N, "Decline clear prompt");
    wait_for(&s, "Cancelled", "Should print Cancelled after declining");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Side effect: cache was NOT cleared.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Total entries: 2"))
        .stdout(predicates::str::contains("Fresh: 2"));
}

/// Bare `cache clear` on a populated cache prompts the user, and
/// accepting (y) clears everything.
#[test]
fn cache_clear_bare_populated_accept_clears() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    let mut s = spawn_bivvy_with_home(&["cache", "clear"], home.path());
    wait_and_answer(&s, "Clear 2 cached entries?", KEY_Y, "Accept clear prompt");
    wait_for(&s, "Cleared 2 entries", "Should print Cleared 2 entries");
    s.expect(expectrl::Eof).unwrap();
    assert_exit_code(&s, 0);

    // Side effect: cache IS cleared.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cache is empty"));
    // And stats confirm zero entries.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Total entries: 0"));
}

// =====================================================================
// LIFECYCLE — multi-step sequences
// =====================================================================

/// Full lifecycle: seed → list (populated) → clear --force → stats
/// (zero) → clear --expired (no-op) → list (empty).  Each phase has its
/// own content and exit-code assertion.
#[test]
fn cache_full_lifecycle_seed_clear_verify() {
    let home = TempDir::new().unwrap();
    seed_cache_with_fresh_entries(home.path());

    // Phase 1: list shows 2 seeded entries.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("2 cached entries:"));

    // Phase 2: force clear removes both.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "clear", "--force"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cleared 2 entries"));

    // Phase 3: stats show zero.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Total entries: 0"))
        .stdout(predicates::str::contains("Fresh: 0"))
        .stdout(predicates::str::contains("Expired: 0"));

    // Phase 4: clear --expired on empty cache is a no-op.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "clear", "--expired"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cleared 0 expired entries"));

    // Phase 5: list still empty.
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Cache is empty"));
}

// =====================================================================
// HELP — documented flags & usage
// =====================================================================

/// `cache --help` output contains the documented subcommand description
/// and lists all three subcommands (list/clear/stats).  Snapshot for
/// regression protection.
#[test]
fn cache_help_snapshot() {
    let home = TempDir::new().unwrap();
    let output = bivvy_assert_cmd_with_home(home.path()).args(["cache", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();

    assert!(
        text.contains("Manage template cache"),
        "Help should contain the cache description, got: {text}"
    );
    assert!(text.contains("list"), "Help should list `list` subcommand");
    assert!(text.contains("clear"), "Help should list `clear` subcommand");
    assert!(text.contains("stats"), "Help should list `stats` subcommand");

    insta::assert_snapshot!("cache_help", text);
}

/// `cache clear --help` documents both `--expired` and `--force` flags.
#[test]
fn cache_clear_help_snapshot() {
    let home = TempDir::new().unwrap();
    let output = bivvy_assert_cmd_with_home(home.path()).args(["cache", "clear", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();

    assert!(
        text.contains("--expired"),
        "Clear help should mention --expired flag, got: {text}"
    );
    assert!(
        text.contains("--force"),
        "Clear help should mention --force flag, got: {text}"
    );

    insta::assert_snapshot!("cache_clear_help", text);
}

/// `cache list --help` documents both `--verbose` and `--json` flags.
#[test]
fn cache_list_help_snapshot() {
    let home = TempDir::new().unwrap();
    let output = bivvy_assert_cmd_with_home(home.path()).args(["cache", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();

    assert!(
        text.contains("--verbose"),
        "List help should mention --verbose flag, got: {text}"
    );
    assert!(
        text.contains("--json"),
        "List help should mention --json flag, got: {text}"
    );

    insta::assert_snapshot!("cache_list_help", text);
}

/// `cache stats --help` shows usage and subcommand name.
#[test]
fn cache_stats_help_snapshot() {
    let home = TempDir::new().unwrap();
    let output = bivvy_assert_cmd_with_home(home.path()).args(["cache", "stats", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();

    assert!(
        text.contains("Usage"),
        "Stats help should show Usage section, got: {text}"
    );
    assert!(
        text.contains("stats"),
        "Stats help should mention 'stats', got: {text}"
    );

    insta::assert_snapshot!("cache_stats_help", text);
}

// =====================================================================
// SAD PATH — invalid invocations
// =====================================================================

/// `cache` with no subcommand exits 2 (clap usage error) and mentions a
/// missing subcommand in stderr.
#[test]
fn cache_no_subcommand_errors_with_exit_2() {
    let home = TempDir::new().unwrap();
    bivvy_assert_cmd_with_home(home.path()).args(["cache"])
        .assert()
        .failure()
        .code(2)
        .stderr(
            predicates::str::contains("Usage")
                .or(predicates::str::contains("subcommand")),
        );
}

/// `cache frobnicate` exits 2 with a clap error naming the bad subcommand.
#[test]
fn cache_unknown_subcommand_errors_with_exit_2() {
    let home = TempDir::new().unwrap();
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "frobnicate"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("error:"))
        .stderr(predicates::str::contains("frobnicate"));
}

/// `cache list --unknown-flag` exits 2 with a clap error.
#[test]
fn cache_list_unknown_flag_errors_with_exit_2() {
    let home = TempDir::new().unwrap();
    bivvy_assert_cmd_with_home(home.path()).args(["cache", "list", "--unknown-flag"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("error:"))
        .stderr(predicates::str::contains("--unknown-flag"));
}
