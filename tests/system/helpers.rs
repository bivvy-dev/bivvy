//! Shared PTY helpers for system tests.
//!
//! Provides project setup, binary spawning, and poll-based PTY interaction
//! utilities.  The poll-based approach avoids a macOS-specific hang where
//! expectrl's rapid O_NONBLOCK toggling on the master fd interferes with
//! bivvy's terminal writes.

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

/// Default timeout for PTY interactions (90 s is generous enough for CI).
pub const TIMEOUT: Duration = Duration::from_secs(90);

/// Shorter timeout for commands that should complete quickly.
pub const SHORT_TIMEOUT: Duration = Duration::from_secs(30);

// ── Project setup ────────────────────────────────────────────────────

/// Create a temporary project directory with `.bivvy/config.yml`.
///
/// Returns a `TempDir` whose lifetime controls cleanup.
pub fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    temp
}

/// Create a temporary project with a real git repository and project files.
///
/// Initialises a git repo with one commit containing `Cargo.toml`,
/// `Cargo.lock`, `src/main.rs`, `VERSION`, and `.env`.  This is the
/// baseline for any test that exercises git-dependent steps.
pub fn setup_project_with_git(config: &str) -> TempDir {
    let temp = setup_project(config);

    // Git repo
    Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(temp.path())
        .output()
        .expect("git init failed");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()
        .ok();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(temp.path())
        .output()
        .ok();

    // Project files
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"test-project\"\nversion = \"0.2.5\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(temp.path().join("Cargo.lock"), "# lock\n").unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(temp.path().join("VERSION"), "0.2.5\n").unwrap();
    fs::write(temp.path().join(".env"), "APP_ENV=development\n").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(temp.path())
        .output()
        .ok();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp.path())
        .output()
        .ok();

    temp
}

// ── Spawning ─────────────────────────────────────────────────────────

/// Spawn `bivvy` with the given arguments in a PTY.
pub fn spawn_bivvy(args: &[&str], dir: &Path) -> Session {
    spawn_bivvy_with_timeout(args, dir, TIMEOUT)
}

/// Spawn `bivvy` with a custom timeout.
pub fn spawn_bivvy_with_timeout(args: &[&str], dir: &Path, timeout: Duration) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(timeout));
    session
}

/// Spawn `bivvy` with extra environment variables.
pub fn spawn_bivvy_with_env(
    args: &[&str],
    dir: &Path,
    env: &[(&str, &str)],
) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(TIMEOUT));
    session
}

/// Spawn `bivvy` without a project directory (for commands like `templates`).
pub fn spawn_bivvy_global(args: &[&str]) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(SHORT_TIMEOUT));
    session
}

/// Run `bivvy run` non-interactively (stdout/stderr suppressed) and assert
/// it succeeds.  Useful for priming state before testing `last` / `history`.
pub fn run_workflow_silently(dir: &Path) {
    let bin = cargo_bin("bivvy");
    let status = Command::new(bin)
        .args(["run"])
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(status.success(), "bivvy run should succeed");
}

/// Run `bivvy run` non-interactively with custom args.
pub fn run_bivvy_silently(dir: &Path, args: &[&str]) {
    let bin = cargo_bin("bivvy");
    let status = Command::new(bin)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Failed to run bivvy");
    assert!(status.success(), "bivvy {args:?} should succeed");
}

// ── ANSI / output helpers ────────────────────────────────────────────

/// Strip ANSI escape sequences for readable assertion messages.
pub fn strip_ansi(s: &str) -> String {
    s.chars()
        .fold((String::new(), false), |(mut acc, in_esc), c| {
            if c == '\x1b' {
                (acc, true)
            } else if in_esc {
                if c.is_ascii_alphabetic() {
                    (acc, false)
                } else {
                    (acc, true)
                }
            } else {
                acc.push(c);
                (acc, false)
            }
        })
        .0
}

/// Expect a pattern, panicking with full ANSI-stripped PTY output on failure.
pub fn expect_or_dump(s: &mut Session, pattern: &str, context: &str) {
    if let Err(e) = s.expect(pattern) {
        s.set_expect_timeout(Some(Duration::from_secs(3)));
        let remaining = match s.expect(expectrl::Eof) {
            Ok(m) => String::from_utf8_lossy(m.as_bytes()).to_string(),
            Err(_) => "(process still alive, no EOF in 3s)".to_string(),
        };
        s.set_expect_timeout(Some(TIMEOUT));
        let clean = strip_ansi(&remaining);
        panic!(
            "{context}\n\
             Expected: {pattern:?}\n\
             Error: {e}\n\
             Remaining PTY output (ANSI stripped):\n\
             ---\n{clean}\n---"
        );
    }
}

/// Read all remaining PTY output until EOF, return ANSI-stripped text.
pub fn read_to_eof(s: &mut Session) -> String {
    let output = s.expect(expectrl::Eof).unwrap();
    let text = String::from_utf8_lossy(output.as_bytes());
    strip_ansi(&text)
}

// ── Poll-based PTY interaction ───────────────────────────────────────
//
// These use `libc::poll()` + manual `O_NONBLOCK` management instead of
// expectrl's `expect()` to avoid the macOS PTY hang.

/// Send a single byte to the PTY master fd.
pub fn send_key(s: &Session, key: u8) {
    use std::os::unix::io::AsRawFd;
    let fd = s.get_stream().as_raw_fd();
    unsafe {
        libc::write(fd, &key as *const u8 as *const _, 1);
    }
}

/// Wait for `pattern` in PTY output, then send `key`.
///
/// Uses poll-based reading to avoid macOS PTY hangs.
pub fn wait_and_answer(s: &Session, pattern: &str, key: u8, context: &str) {
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let fd = s.get_stream().as_raw_fd();
    let mut accumulated = String::new();
    let start = Instant::now();
    let mut buf = [0u8; 4096];

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

        let ready = unsafe {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut pfd, 1, 100)
        };

        if ready > 0 {
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

            if accumulated.contains(pattern) {
                send_key(s, key);
                return;
            }
        }
    }
}

/// Wait for `pattern` in PTY output without sending a key.
pub fn wait_for(s: &Session, pattern: &str, context: &str) {
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let fd = s.get_stream().as_raw_fd();
    let mut accumulated = String::new();
    let start = Instant::now();
    let mut buf = [0u8; 4096];

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

        let ready = unsafe {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut pfd, 1, 100)
        };

        if ready > 0 {
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

            if accumulated.contains(pattern) {
                return;
            }
        }
    }
}

/// Wait for a pattern and send a full line (text + newline).
pub fn wait_and_send_line(s: &Session, pattern: &str, line: &str, context: &str) {
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let fd = s.get_stream().as_raw_fd();
    let mut accumulated = String::new();
    let start = Instant::now();
    let mut buf = [0u8; 4096];

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

        let ready = unsafe {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut pfd, 1, 100)
        };

        if ready > 0 {
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

            if accumulated.contains(pattern) {
                // Write the line followed by newline
                let payload = format!("{line}\n");
                let bytes = payload.as_bytes();
                unsafe {
                    libc::write(fd, bytes.as_ptr() as *const _, bytes.len());
                }
                return;
            }
        }
    }
}

// ── Common key constants ─────────────────────────────────────────────

/// ASCII `y` — accept / yes
pub const KEY_Y: u8 = b'y';
/// ASCII `n` — decline / no
pub const KEY_N: u8 = b'n';
/// ASCII space — toggle selection or confirm
pub const KEY_SPACE: u8 = b' ';
/// ASCII carriage return — Enter/Return
pub const KEY_ENTER: u8 = b'\r';
/// ASCII escape — cancel / abort
pub const KEY_ESC: u8 = 0x1b;
/// ASCII ETX (Ctrl-C) — interrupt
pub const KEY_CTRL_C: u8 = 0x03;
/// Arrow down (ANSI escape sequence, sent as 3 bytes)
pub const ARROW_DOWN: &[u8] = b"\x1b[B";
/// Arrow up (ANSI escape sequence, sent as 3 bytes)
pub const ARROW_UP: &[u8] = b"\x1b[A";

/// Send a multi-byte key sequence (e.g. arrow keys).
pub fn send_keys(s: &Session, keys: &[u8]) {
    use std::os::unix::io::AsRawFd;
    let fd = s.get_stream().as_raw_fd();
    unsafe {
        libc::write(fd, keys.as_ptr() as *const _, keys.len());
    }
}

/// Wait for a pattern, then send a multi-byte key sequence.
pub fn wait_and_send_keys(s: &Session, pattern: &str, keys: &[u8], context: &str) {
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let fd = s.get_stream().as_raw_fd();
    let mut accumulated = String::new();
    let start = Instant::now();
    let mut buf = [0u8; 4096];

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

        let ready = unsafe {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut pfd, 1, 100)
        };

        if ready > 0 {
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

            if accumulated.contains(pattern) {
                send_keys(s, keys);
                return;
            }
        }
    }
}
