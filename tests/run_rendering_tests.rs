//! Rendering tests for `bivvy run` using vt100 + insta.
//!
//! Spawns a real `bivvy run` in a PTY, captures the byte stream
//! (including ANSI escape sequences), feeds it through a vt100 terminal
//! emulator at a fixed window size, and snapshots the resulting screen
//! via insta.
//!
//! These tests pin the visual output of a multi-step workflow so
//! refactors to the run-path display layer can't silently change what
//! the user sees. They also act as the regression test for the
//! multi-line spinner clearing bug from `ui-surface-refactor-plan.md`:
//! a single-line workflow run must not leave dangling progress-bar
//! fragments or stale `Running …` lines in scrollback.
#![cfg(unix)]

use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use regex::Regex;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Project + spawn helpers (intentionally inlined — keeping this file
// self-contained avoids dragging in the broader system-test toolbox.)
// ─────────────────────────────────────────────────────────────────────

/// Subdirectory inside each project tempdir used as a sandboxed `HOME`.
const TEST_HOME_SUBDIR: &str = ".test_home";

/// Pin `HOME` and the XDG base-directory variables to a sandbox so the
/// child process never reads or writes real user state.
fn apply_home_isolation(cmd: &mut Command, home: &Path) {
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", home.join(".local").join("share"));
    cmd.env("XDG_CACHE_HOME", home.join(".cache"));
    cmd.env("XDG_STATE_HOME", home.join(".local").join("state"));
}

/// Create a temporary project with `.bivvy/config.yml` populated and a
/// `.test_home` directory ready for spawn-time `HOME` isolation.
fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    fs::create_dir_all(temp.path().join(TEST_HOME_SUBDIR)).unwrap();
    temp
}

/// Spawn `bivvy` with `args` in `dir`, with HOME/XDG pinned to the
/// project's `.test_home` sandbox.
fn spawn_bivvy(args: &[&str], dir: &Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    apply_home_isolation(&mut cmd, &dir.join(TEST_HOME_SUBDIR));
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
}

/// Window size used by every vt100 capture in this file.
///
/// 24 rows × 80 cols matches `xterm`'s default and is the size assumed
/// by many of bivvy's rendering routines (line wrapping, header
/// horizontal rules, etc.). Keeping it constant makes snapshots stable
/// across machines.
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Drive a PTY session, accumulating every byte the child writes
/// until the post-run hint appears or the timeout expires.
///
/// We can't rely on EOF because the child sometimes lingers after the
/// hint (the parent shell isn't done flushing). Polling for the
/// terminating hint is enough — the run is observably finished by then.
fn drain_until_done(session: expectrl::Session, timeout: Duration) -> Vec<u8> {
    let fd = session.get_stream().as_raw_fd();
    let mut accumulated = Vec::new();
    let start = Instant::now();

    loop {
        if start.elapsed() > timeout {
            break;
        }

        let ready = unsafe {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut pfd, 1, 200)
        };

        if ready > 0 {
            let mut buf = [0u8; 4096];
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                loop {
                    let n = libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len());
                    if n <= 0 {
                        break;
                    }
                    accumulated.extend_from_slice(&buf[..n as usize]);
                }
                libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            }
        }

        // Stop once the post-run hint has rendered — any further output
        // is tear-down noise from the parent shell, not run output.
        let view = String::from_utf8_lossy(&accumulated);
        if view.contains("bivvy status") || view.contains("aborted by user") {
            // Drain a little more to capture trailing newlines.
            std::thread::sleep(Duration::from_millis(150));
            let ready = unsafe {
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                libc::poll(&mut pfd, 1, 100)
            };
            if ready > 0 {
                let mut buf = [0u8; 4096];
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                    loop {
                        let n = libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len());
                        if n <= 0 {
                            break;
                        }
                        accumulated.extend_from_slice(&buf[..n as usize]);
                    }
                    libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
                }
            }
            break;
        }
    }

    accumulated
}

/// Render a byte stream through a vt100 emulator and return the final
/// screen contents as a plain-text string with one row per line and
/// no trailing whitespace.
fn render_screen(bytes: &[u8]) -> String {
    let mut parser = vt100::Parser::new(ROWS, COLS, 0);
    parser.process(bytes);
    let screen = parser.screen();
    let mut out = String::new();
    for row in 0..ROWS {
        let mut line = String::new();
        for col in 0..COLS {
            let cell = screen.cell(row, col);
            line.push_str(&cell.map(|c| c.contents()).unwrap_or_default());
        }
        // Trim trailing whitespace per row to keep snapshots compact
        // and stable across emulator quirks.
        out.push_str(line.trim_end());
        out.push('\n');
    }
    // Drop trailing blank lines — snapshots are easier to read without
    // a forest of empty rows at the bottom.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// Replace every duration substring (`12ms`, `1.2s`, etc.) with a
/// stable placeholder so snapshots don't churn on per-run timing noise.
///
/// The matchable forms come from `crate::ui::progress::format_duration`
/// in the library: integer ms, fractional s, integer s, and m/s pairs.
fn normalize_durations(s: &str) -> String {
    static_regex_replace(s, r"\d+(?:\.\d+)?(?:ms|s)|\d+m\s?\d+s|\d+m", "<dur>")
}

/// One-shot regex replacement that compiles the pattern lazily.
fn static_regex_replace(haystack: &str, pattern: &str, replacement: &str) -> String {
    let re = Regex::new(pattern).expect("static regex compiles");
    re.replace_all(haystack, replacement).into_owned()
}

/// Render the full byte stream — including scrollback that has
/// scrolled out of the visible window — by replaying it through a tall
/// emulator and returning every row, blank rows included.
///
/// Interior blank lines are preserved so the snapshot reflects vertical
/// spacing accurately (e.g. the blank line between consecutive steps).
/// Only the trailing blank rows from the unused portion of the tall
/// window are trimmed.
fn render_full_transcript(bytes: &[u8]) -> String {
    // Use a tall window so scrollback is preserved verbatim.
    let mut parser = vt100::Parser::new(200, COLS, 0);
    parser.process(bytes);
    let screen = parser.screen();

    let mut rows: Vec<String> = Vec::with_capacity(200);
    for row in 0..200 {
        let mut line = String::new();
        for col in 0..COLS {
            let cell = screen.cell(row, col);
            line.push_str(&cell.map(|c| c.contents()).unwrap_or_default());
        }
        rows.push(line.trim_end().to_string());
    }

    // Drop trailing blank rows from the unused window.
    while rows.last().map(|l| l.is_empty()).unwrap_or(false) {
        rows.pop();
    }

    let mut out = rows.join("\n");
    out.push('\n');
    out
}

// ─────────────────────────────────────────────────────────────────────
// Multi-step rendering snapshots
// ─────────────────────────────────────────────────────────────────────

/// Five-step workflow that exercises real-world tools (`git`, `cargo`,
/// `rustc`). This is the canonical "happy path" snapshot — it verifies
/// the run header, per-step header, indented result line, summary, and
/// post-run hint all render cleanly together.
///
/// Per CLAUDE.md system test goals: shell builtins like `echo`/`true`
/// prove nothing about how Bivvy handles real tool output. Tests use
/// the same kind of commands a real `bin/setup` script runs.
#[test]
fn renders_five_step_happy_path() {
    let config = r#"
app_name: "Snapshot Demo"

steps:
  rustc_version:
    title: "Print rustc version"
    command: "rustc --version"
  cargo_version:
    title: "Print cargo version"
    command: "cargo --version"
  git_version:
    title: "Print git version"
    command: "git --version"
  rustc_target:
    title: "Show rustc host triple"
    command: "rustc -vV"
  cargo_locate:
    title: "Locate cargo binary"
    command: "cargo --list"

workflows:
  default:
    steps:
      - rustc_version
      - cargo_version
      - git_version
      - rustc_target
      - cargo_locate
"#;
    let temp = setup_project(config);
    let session = spawn_bivvy(&["run"], temp.path());
    let bytes = drain_until_done(session, Duration::from_secs(30));
    let transcript = normalize_durations(&render_full_transcript(&bytes));
    insta::assert_snapshot!("five_step_happy_path", transcript);
}

/// Workflow with a multi-line YAML `command:`. This is the regression
/// test for the multi-line spinner clearing bug: with the old
/// `TerminalUI`, the dangling `[██████░░░░░░░░░░] N/M steps` and
/// stale `Running …` fragments would leak into scrollback. With the
/// new `TerminalSurface` + `TransientRegion`, the region is cleared
/// deterministically and the scrollback contains exactly one final
/// result line per step.
#[test]
fn renders_multiline_command_without_residue() {
    let config = r#"
app_name: "Multiline Bug"

steps:
  rustc_version:
    title: "rustc version"
    command: "rustc --version"
  multi_step:
    title: "Multi-line invocation"
    command: |
      git --version
      cargo --version
      rustc --version
  cargo_metadata:
    title: "cargo --version"
    command: "cargo --version"
  git_status:
    title: "git status"
    command: "git --version"
  rustc_check:
    title: "rustc --version"
    command: "rustc --version"

workflows:
  default:
    steps:
      - rustc_version
      - multi_step
      - cargo_metadata
      - git_status
      - rustc_check
"#;
    let temp = setup_project(config);
    let session = spawn_bivvy(&["run"], temp.path());
    let bytes = drain_until_done(session, Duration::from_secs(30));
    let raw_transcript = render_full_transcript(&bytes);

    // Assert the bug-fix invariant against the raw transcript: between
    // step boundaries there must be no dangling progress-bar block and
    // no stale "Running …" fragment from the multi-line command. The
    // transient region is cleared deterministically once a step
    // finishes.
    let multi_running_count = raw_transcript
        .lines()
        .filter(|l| l.contains("Running ") && l.contains("git --version") && l.contains("cargo"))
        .count();
    assert!(
        multi_running_count <= 1,
        "expected at most one 'Running ...' line for the multi-line step \
         (deterministic clear should have wiped any residue), but found \
         {} occurrences in:\n{}",
        multi_running_count,
        raw_transcript
    );

    let transcript = normalize_durations(&raw_transcript);
    insta::assert_snapshot!("multiline_command_no_residue", transcript);
}

/// 24×80 final-screen snapshot of a six-step workflow. Pins what the
/// user sees in their terminal viewport at the moment the run finishes
/// (the summary box + post-run hint).
#[test]
fn final_screen_after_six_step_run() {
    let config = r#"
app_name: "Final Screen"

steps:
  rustc:
    command: "rustc --version"
  cargo_v:
    command: "cargo --version"
  git_v:
    command: "git --version"
  cargo_help:
    command: "cargo --help"
  rustup:
    command: "rustup --version"
  rustc_target:
    command: "rustc --print host-tuple"

workflows:
  default:
    steps:
      - rustc
      - cargo_v
      - git_v
      - cargo_help
      - rustup
      - rustc_target
"#;
    let temp = setup_project(config);
    let session = spawn_bivvy(&["run"], temp.path());
    let bytes = drain_until_done(session, Duration::from_secs(30));
    let final_screen = normalize_durations(&render_screen(&bytes));
    insta::assert_snapshot!("final_screen_six_step", final_screen);
}
