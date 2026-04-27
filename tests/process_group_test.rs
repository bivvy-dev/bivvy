//! Process group / job control regression tests.
//!
//! These tests verify that bivvy survives running under a real
//! job-controlling shell (zsh with `MONITOR` option). The existing PTY
//! tests spawn bivvy directly — the expectrl PTY *is* the controlling
//! terminal, so there's no parent shell doing process group management.
//! That topology can't surface the bug where zsh reclaims the foreground
//! group after a child process exits, causing SIGTTOU/SIGTTIN or
//! premature suspension.
//!
//! These tests spawn **zsh** inside the PTY, then run bivvy as a
//! command within that zsh session. This gives us:
//!   - Real job control (zsh's `MONITOR` option)
//!   - Real process group assignment and foreground management
//!   - Real signal delivery (SIGTTOU, SIGTTIN, SIGTSTP)
//!   - A parent shell that actively manages the terminal foreground
//!
//! The critical transition under test: after a child process (step
//! command) exits, bivvy must reclaim the terminal foreground to
//! render the next interactive prompt. If process group isolation or
//! foreground reclamation is broken, the prompt never appears and
//! the test times out — or zsh reports "suspended (tty output)".
#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(60);

/// Marker printed by .zshrc to confirm the shell is ready.
const READY: &str = "ZSH_READY";

// ── ANSI stripping ──────────────────────────────────────────────────

/// Strip ANSI escape sequences from PTY output for readable snapshots.
///
/// Handles:
///   - CSI sequences: `ESC [ <params> <final>`
///   - OSC sequences: `ESC ] ... BEL` or `ESC ] ... ESC \`
///   - SS2/SS3:       `ESC N <char>` / `ESC O <char>`
///   - Two-byte:      `ESC <letter>` (e.g. `ESC =`, `ESC >`)
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                // CSI: ESC [ <params> <final byte 0x40-0x7E>
                Some('[') => {
                    chars.next(); // consume '['
                    for ch in chars.by_ref() {
                        if ch.is_ascii() && (0x40..=0x7E).contains(&(ch as u8)) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... (BEL | ESC \)
                Some(']') => {
                    chars.next(); // consume ']'
                    for ch in chars.by_ref() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // SS2 / SS3: ESC N/O + one character
                Some('N') | Some('O') => {
                    chars.next();
                    chars.next();
                }
                // Two-byte sequence: ESC + anything else
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else if c == '\x07' {
            // Bare BEL — skip
        } else {
            result.push(c);
        }
    }
    result
}

/// Normalize PTY output for stable snapshots.
///
/// Only strips ANSI codes, removes \r, and replaces timing values
/// with `[TIME]`. Does NOT filter content — everything bivvy produces
/// should be visible in the snapshot.
fn normalize_for_snapshot(s: &str) -> String {
    let stripped = strip_ansi(s);
    let re_time = regex::Regex::new(r"\b\d+(\.\d+)?(ms|s)\b").unwrap();
    // Caret-notation escape sequences echoed by PTY: ^[[B, ^[[A, etc.
    let re_caret_csi = regex::Regex::new(r"\^\[[\[0-9;]*[A-Za-z~]").unwrap();

    stripped
        .replace('\r', "")
        .lines()
        .map(|line| {
            let trimmed = line.trim_end();
            let no_caret = re_caret_csi.replace_all(trimmed, "");
            re_time.replace_all(&no_caret, "[TIME]").to_string()
        })
        // Drop leading zsh prompt noise (% prompt, $ prompt)
        .skip_while(|line| line.is_empty() || line.starts_with('%') || line.starts_with('$'))
        // Stop at trailing zsh prompt
        .take_while(|line| !line.starts_with('%'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert that PTY output never contains "suspended" — the smoking gun
/// for the process group bug. Zsh prints "suspended (tty output)" or
/// "suspended (tty input)" when a background process tries terminal I/O.
fn assert_not_suspended(output: &str, context: &str) {
    let lower = strip_ansi(output).to_lowercase();
    assert!(
        !lower.contains("suspended"),
        "{context}: bivvy was suspended by zsh!\nOutput:\n{output}"
    );
}

// ── Low-level PTY primitives ───────────────────────────────────────
//
// Two safe abstractions over libc that contain all the `unsafe` for
// poll-based PTY interaction.

/// Write bytes to a PTY session's master file descriptor.
fn pty_write(session: &Session, data: &[u8]) {
    use std::os::unix::io::AsRawFd;
    let fd = session.get_stream().as_raw_fd();
    // SAFETY: fd is a valid, open PTY master fd obtained from the Session.
    // data is a valid byte slice that outlives the call.  For the short
    // payloads used in interactive testing (single keys, escape sequences,
    // short lines), partial writes are not a concern.
    unsafe {
        libc::write(fd, data.as_ptr() as *const _, data.len());
    }
}

/// Poll for available data on a PTY fd and drain it into `accumulated`.
///
/// Temporarily sets `O_NONBLOCK` to drain all buffered data without
/// blocking, then restores the original flags.  Returns `true` if the
/// poll indicated data was ready.
fn poll_and_drain(
    fd: std::os::unix::io::RawFd,
    accumulated: &mut String,
    poll_timeout_ms: i32,
) -> bool {
    // SAFETY: poll() with a single stack-allocated pollfd is a standard
    // POSIX syscall.  The pollfd is valid for the duration of the call.
    let ready = unsafe {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        libc::poll(&mut pfd, 1, poll_timeout_ms)
    };

    if ready > 0 {
        let mut buf = [0u8; 4096];
        // SAFETY: fcntl F_GETFL/F_SETFL and read() are standard POSIX
        // calls on a valid PTY fd.  O_NONBLOCK is set temporarily to
        // drain all available data, then the original flags are restored.
        // The buffer is stack-allocated and valid for each read() call.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            loop {
                let n = libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len());
                if n <= 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buf[..n as usize]);
                accumulated.push_str(&chunk);
            }
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
        true
    } else {
        false
    }
}

// ── PTY interaction helpers ─────────────────────────────────────────

/// Poll-based wait for a pattern in PTY output. Returns the full
/// accumulated output (ANSI-stripped) since this call started.
fn wait_for(session: &Session, pattern: &str, context: &str) -> String {
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let fd = session.get_stream().as_raw_fd();
    let mut accumulated = String::new();
    let start = Instant::now();

    loop {
        if start.elapsed() > TIMEOUT {
            let clean = strip_ansi(&accumulated);
            panic!(
                "{context}\n\
                 Expected: {pattern:?}\n\
                 Timed out after {TIMEOUT:?}\n\
                 Accumulated PTY output (ANSI stripped):\n\
                 ---\n{clean}\n---"
            );
        }

        poll_and_drain(fd, &mut accumulated, 200);

        if strip_ansi(&accumulated).contains(pattern) {
            return strip_ansi(&accumulated);
        }
    }
}

/// Wait for a pattern, then send a single key. Returns accumulated output.
fn wait_and_send(session: &Session, pattern: &str, key: u8, context: &str) -> String {
    let output = wait_for(session, pattern, context);
    send_key(session, key);
    output
}

/// Send a single byte to the PTY.
fn send_key(session: &Session, key: u8) {
    pty_write(session, &[key]);
}

/// Send a byte sequence to the PTY.
fn send_bytes(session: &Session, bytes: &[u8]) {
    pty_write(session, bytes);
}

/// Send a command string to the PTY (with trailing newline).
fn send_line(session: &Session, line: &str) {
    let payload = format!("{line}\n");
    pty_write(session, payload.as_bytes());
}

/// Assert the exit code of the last command run in zsh.
///
/// Uses printf with `%s` expansion so the expanded output differs from
/// the echoed command text (which shows `"$?"` literally, not expanded).
fn assert_zsh_exit_code(session: &Session, expected: i32, context: &str) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tag = format!("RC_{nonce}_");
    send_line(session, &format!("printf '{tag}%s\\n' \"$?\""));

    let expected_str = format!("{tag}{expected}");
    let output = wait_for(session, &expected_str, &format!("{context} — exit code"));
    assert!(
        output.contains(&expected_str),
        "{context}: Expected exit code {expected}.\nOutput: {output}"
    );
}

/// `y` — single key selects "Yes" in bivvy's raw-mode prompts (no Enter)
const KEY_Y: u8 = b'y';

/// Arrow down — ANSI escape sequence (3 bytes)
const ARROW_DOWN: &[u8] = b"\x1b[B";

/// Enter / carriage return
const KEY_ENTER: u8 = b'\r';

// ── Project setup ───────────────────────────────────────────────────

fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();

    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();

    let home = temp.path().join(".test_home");
    fs::create_dir_all(&home).unwrap();

    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(temp.path())
            .env("HOME", &home)
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("GIT_CONFIG_SYSTEM")
            .output()
            .expect("git command failed")
    };

    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.email", "test@test.com"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "commit.gpgsign", "false"]);

    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(temp.path().join(".gitignore"), ".test_home/\n").unwrap();

    git(&["add", "."]);
    git(&["commit", "-m", "Initial commit"]);

    temp
}

fn spawn_zsh(project_dir: &std::path::Path) -> Session {
    let home = project_dir.join(".test_home");

    let zdotdir = home.join(".zsh_test");
    fs::create_dir_all(&zdotdir).unwrap();

    fs::write(
        zdotdir.join(".zshrc"),
        format!("setopt MONITOR\nPS1='$ '\necho {READY}\n"),
    )
    .unwrap();

    let bivvy_bin = cargo_bin("bivvy");

    let mut cmd = Command::new("/bin/zsh");
    cmd.arg("-i");
    cmd.current_dir(project_dir);
    cmd.env("HOME", &home);
    cmd.env("ZDOTDIR", &zdotdir);
    cmd.env("BIVVY", &bivvy_bin);
    cmd.env("TERM", "dumb");
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", home.join(".local/share"));
    cmd.env("XDG_CACHE_HOME", home.join(".cache"));
    cmd.env("XDG_STATE_HOME", home.join(".local/state"));

    let mut session = Session::spawn(cmd).expect("Failed to spawn zsh");
    session.set_expect_timeout(Some(TIMEOUT));
    session
}

// ── Configs ─────────────────────────────────────────────────────────

/// 3-step interactive workflow. Every step is skippable (default), so
/// bivvy prompts "Step title?" before each one.
const INTERACTIVE_CONFIG: &str = r#"
app_name: "ProcessGroupTest"

settings:
  default_output: verbose

steps:
  check-toolchain:
    title: "Check toolchain"
    command: "rustc --version > .step1.txt"

  check-git:
    title: "Check git"
    command: "git --version > .step2.txt"
    depends_on: [check-toolchain]

  check-cargo:
    title: "Check cargo"
    command: "cargo --version > .step3.txt"
    depends_on: [check-git]

workflows:
  default:
    steps: [check-toolchain, check-git, check-cargo]
"#;

/// 3-step interactive workflow with checks.
const COMPLETED_CHECK_CONFIG: &str = r#"
app_name: "CompletedCheckTest"

settings:
  default_output: verbose

steps:
  install-tools:
    title: "Install tools"
    command: "rustc --version > .tools-installed.txt"
    check:
      type: presence
      target: ".tools-installed.txt"

  verify-repo:
    title: "Verify repository"
    command: "git rev-parse --git-dir > .repo-verified.txt"
    depends_on: [install-tools]
    check:
      type: execution
      command: "git rev-parse --git-dir"

  run-analysis:
    title: "Run analysis"
    command: "cargo --version > .analysis.txt"
    depends_on: [verify-repo]

workflows:
  default:
    steps: [install-tools, verify-repo, run-analysis]
"#;

/// Failure scenario. First step skippable (prompts), second non-skippable
/// and always fails, third blocked by dependency.
const FAILING_CONFIG: &str = r#"
app_name: "FailTest"

settings:
  default_output: verbose

steps:
  will-succeed:
    title: "Will succeed"
    command: "rustc --version > .succeeded.txt"

  will-fail:
    title: "Will fail"
    command: "sh -c 'exit 1'"
    depends_on: [will-succeed]
    skippable: false

  after-fail:
    title: "After fail"
    command: "cargo --version > .after-fail.txt"
    depends_on: [will-fail]

workflows:
  default:
    steps: [will-succeed, will-fail, after-fail]
"#;

// ── Tests ───────────────────────────────────────────────────────────

/// Core regression test: interactive multi-step workflow under zsh job
/// control. Each step is skippable, so bivvy prompts before each one.
/// The user presses `y` to accept.
///
/// If process group handling is broken, the second or third prompt
/// never appears and the test times out.
#[test]
fn interactive_workflow_under_zsh_job_control() {
    let temp = setup_project(INTERACTIVE_CONFIG);
    let session = spawn_zsh(temp.path());

    wait_for(&session, READY, "Waiting for zsh to start");
    send_line(&session, "$BIVVY run");

    // Step 1: prompt → y → step runs
    wait_and_send(&session, "Check toolchain?", KEY_Y, "Step 1 prompt");

    // Step 2: prompt appears after step 1's child exits — the critical
    // foreground reclamation transition
    wait_and_send(&session, "Check git?", KEY_Y, "Step 2 prompt");

    // Step 3: one more transition
    wait_and_send(&session, "Check cargo?", KEY_Y, "Step 3 prompt");

    // Wait for and snapshot the summary
    let summary = wait_for(&session, "bivvy status", "Workflow completion");
    assert_not_suspended(&summary, "interactive workflow");
    insta::assert_snapshot!(
        "interactive_workflow_summary",
        normalize_for_snapshot(&summary)
    );

    assert_zsh_exit_code(&session, 0, "interactive workflow");

    assert!(
        temp.path().join(".step1.txt").exists(),
        "Step 1 side-effect missing"
    );
    assert!(
        temp.path().join(".step2.txt").exists(),
        "Step 2 side-effect missing"
    );
    assert!(
        temp.path().join(".step3.txt").exists(),
        "Step 3 side-effect missing"
    );

    send_line(&session, "exit");
}

/// Completed-check steps spawn additional child processes between
/// prompt transitions.
#[test]
fn completed_check_steps_under_zsh_job_control() {
    let temp = setup_project(COMPLETED_CHECK_CONFIG);
    let session = spawn_zsh(temp.path());

    wait_for(&session, READY, "Waiting for zsh to start");
    send_line(&session, "$BIVVY run");

    // Step 1: file doesn't exist yet → regular prompt
    wait_and_send(&session, "Install tools?", KEY_Y, "Step 1 prompt");

    // Step 2: git rev-parse succeeds → "Check passed" re-run prompt
    wait_and_send(&session, "Check passed", KEY_Y, "Step 2 re-run prompt");

    // Step 3: no completed_check → regular prompt
    wait_and_send(&session, "Run analysis?", KEY_Y, "Step 3 prompt");

    let summary = wait_for(&session, "bivvy status", "Workflow completion");
    assert_not_suspended(&summary, "completed-check workflow");
    insta::assert_snapshot!(
        "completed_check_workflow_summary",
        normalize_for_snapshot(&summary)
    );

    assert_zsh_exit_code(&session, 0, "completed-check workflow");

    assert!(
        temp.path().join(".tools-installed.txt").exists(),
        "Step 1 side-effect missing"
    );
    assert!(
        temp.path().join(".repo-verified.txt").exists(),
        "Step 2 side-effect missing"
    );
    assert!(
        temp.path().join(".analysis.txt").exists(),
        "Step 3 side-effect missing"
    );

    send_line(&session, "exit");
}

/// Run bivvy interactively twice in the same zsh session. Catches
/// dirty process group state leaking between runs.
#[test]
fn consecutive_interactive_runs_under_zsh_job_control() {
    let temp = setup_project(INTERACTIVE_CONFIG);
    let session = spawn_zsh(temp.path());

    wait_for(&session, READY, "Waiting for zsh to start");

    // ── First run ───────────────────────────────────────────────
    send_line(&session, "$BIVVY run");

    wait_and_send(&session, "Check toolchain?", KEY_Y, "Run 1, Step 1");
    wait_and_send(&session, "Check git?", KEY_Y, "Run 1, Step 2");
    wait_and_send(&session, "Check cargo?", KEY_Y, "Run 1, Step 3");

    let run1_output = wait_for(&session, "bivvy status", "Run 1 completion");
    assert_not_suspended(&run1_output, "first run");
    assert_zsh_exit_code(&session, 0, "first run");

    // Clean side-effect files for second run
    send_line(&session, "rm -f .step1.txt .step2.txt .step3.txt");
    send_line(&session, &format!("echo {READY}"));
    wait_for(&session, READY, "Waiting between runs");

    // ── Second run ──────────────────────────────────────────────
    send_line(&session, "$BIVVY run");

    wait_and_send(&session, "Check toolchain?", KEY_Y, "Run 2, Step 1");
    wait_and_send(&session, "Check git?", KEY_Y, "Run 2, Step 2");
    wait_and_send(&session, "Check cargo?", KEY_Y, "Run 2, Step 3");

    let run2_output = wait_for(&session, "bivvy status", "Run 2 completion");
    assert_not_suspended(&run2_output, "second run");
    assert_zsh_exit_code(&session, 0, "second run");

    assert!(
        temp.path().join(".step1.txt").exists(),
        "Step 1 missing after run 2"
    );
    assert!(
        temp.path().join(".step2.txt").exists(),
        "Step 2 missing after run 2"
    );
    assert!(
        temp.path().join(".step3.txt").exists(),
        "Step 3 missing after run 2"
    );

    send_line(&session, "exit");
}

/// A failing step under zsh job control. Step 1 prompts interactively,
/// step 2 fails and shows the recovery menu. We navigate to "Abort"
/// via arrow keys and Enter.
#[test]
fn failed_step_under_zsh_job_control() {
    let temp = setup_project(FAILING_CONFIG);
    let session = spawn_zsh(temp.path());

    wait_for(&session, READY, "Waiting for zsh to start");
    send_line(&session, "$BIVVY run");

    // Step 1: skippable, accept the prompt
    wait_and_send(&session, "Will succeed?", KEY_Y, "Step 1 prompt");

    // Step 2: skippable: false, runs without prompt, fails.
    // Recovery menu: Retry / Skip / Shell / Abort.
    // Navigate to Abort: 3x arrow-down, then Enter.
    //
    // TERM=dumb routes through prompt_select_dumb (bypasses dialoguer
    // entirely). Options are printed once with no redraw cycle. Arrow
    // keys are parsed by console::Term::read_key() in raw mode — ANSI
    // escape parsing is hardcoded, not terminfo-dependent, so arrow
    // navigation works regardless of TERM value. Internal selection
    // state updates silently (no visual feedback on dumb terminals).
    //
    // Wait for the last option ("Abort") to confirm all options rendered
    // and raw mode is active before sending arrow keys.
    wait_for(&session, "Abort", "Recovery menu options");
    for _ in 0..3 {
        send_bytes(&session, ARROW_DOWN);
    }
    send_key(&session, KEY_ENTER);

    // Wait for the closing box line to ensure full summary is captured
    let summary = wait_for(
        &session,
        "\u{2514}\u{2500}\u{2500}\u{2500}", // └───
        "Workflow summary after abort",
    );
    assert_not_suspended(&summary, "failed step workflow");
    insta::assert_snapshot!(
        "failed_step_workflow_summary",
        normalize_for_snapshot(&summary)
    );

    assert_zsh_exit_code(&session, 1, "failing workflow");

    assert!(
        temp.path().join(".succeeded.txt").exists(),
        "Step 1 should have run"
    );
    assert!(
        !temp.path().join(".after-fail.txt").exists(),
        "Step 3 should NOT have run (blocked)"
    );

    // Zsh must still be responsive after bivvy failure
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let alive_tag = format!("ALIVE_{nonce}");
    send_line(&session, &format!("printf '{alive_tag}\\n'"));
    wait_for(&session, &alive_tag, "Zsh responsive after failure");

    send_line(&session, "exit");
}

// ── Unit tests for strip_ansi ───────────────────────────────────────

#[cfg(test)]
mod strip_ansi_tests {
    use super::strip_ansi;

    #[test]
    fn strips_csi_sequences() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("\x1b[1;32mbold green\x1b[0m"), "bold green");
    }

    #[test]
    fn strips_osc_with_bel() {
        assert_eq!(strip_ansi("\x1b]0;My Title\x07hello"), "hello");
    }

    #[test]
    fn strips_osc_with_st() {
        assert_eq!(strip_ansi("\x1b]0;My Title\x1b\\hello"), "hello");
    }

    #[test]
    fn strips_bare_bel() {
        assert_eq!(strip_ansi("hello\x07world"), "helloworld");
    }

    #[test]
    fn preserves_plain_text() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn handles_mixed_sequences() {
        let input = "\x1b]0;title\x07\x1b[1mBold\x1b[0m plain \x1b[32mgreen\x1b[0m";
        assert_eq!(strip_ansi(input), "Bold plain green");
    }

    #[test]
    fn preserves_utf8_characters() {
        assert_eq!(strip_ansi("⛺ hello ✓ world ┌─┐"), "⛺ hello ✓ world ┌─┐");
    }

    #[test]
    fn strips_ansi_around_utf8() {
        assert_eq!(strip_ansi("\x1b[32m✓\x1b[0m done"), "✓ done");
    }
}
