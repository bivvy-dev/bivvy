//! Shared PTY helpers for system tests.
//!
//! Provides project setup, binary spawning, and poll-based PTY interaction
//! utilities.  The poll-based approach avoids a macOS-specific hang where
//! expectrl's rapid O_NONBLOCK toggling on the master fd interferes with
//! bivvy's terminal writes.
//!
//! ## HOME isolation
//!
//! Every spawned `bivvy` process automatically gets `HOME` and the XDG
//! base-directory variables (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
//! `XDG_CACHE_HOME`, `XDG_STATE_HOME`) pointed at a temp directory so tests
//! never touch the real user environment.  Three things fall out of this:
//!
//! 1. `~/.bivvy/projects/{id}/` state (history, last-run, etc.) is isolated
//!    per-test — no leakage into the developer or CI user's real store.
//! 2. The feedback store, which uses `dirs::data_local_dir()` (which honors
//!    `XDG_DATA_HOME` on Linux), is also contained.
//! 3. Git commands run by `setup_project_with_git` see an empty `~/.gitconfig`
//!    so global git settings cannot influence test fixtures.
//!
//! Project-scoped spawns (`spawn_bivvy`, `spawn_bivvy_with_env`, etc.) use
//! `<project>/.test_home` as their HOME; the shared global spawn
//! (`spawn_bivvy_global`) shares a process-lifetime temp dir.  Tests that
//! need a per-test isolated HOME without a project directory — typically
//! cache/feedback tests that seed state before spawning, or tests that
//! assert on an empty starting store — should use `spawn_bivvy_with_home`
//! and manage the `TempDir` themselves.  Callers that pass `HOME`
//! explicitly to `spawn_bivvy_with_env` still win — `Command::env` is
//! last-write-wins.

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;
use tempfile::TempDir;

/// Default timeout for PTY interactions (90 s is generous enough for CI).
pub const TIMEOUT: Duration = Duration::from_secs(90);

/// Shorter timeout for commands that should complete quickly.
pub const SHORT_TIMEOUT: Duration = Duration::from_secs(30);

// ── HOME isolation ───────────────────────────────────────────────────

/// Name of the isolated `HOME` subdirectory created inside every project
/// tempdir by [`setup_project`].  Kept private because callers should not
/// reference this path directly — they interact with it only through the
/// spawn helpers below.
const TEST_HOME_SUBDIR: &str = ".test_home";

/// Return the isolated `HOME` directory for a project tempdir.
///
/// Creates `<dir>/.test_home` if it does not already exist.  Placing the
/// home inside the project tempdir means the parent `TempDir` guard owns
/// cleanup of both the project and the home in one drop.
pub fn project_test_home(dir: &Path) -> PathBuf {
    let home = dir.join(TEST_HOME_SUBDIR);
    fs::create_dir_all(&home).unwrap();
    home
}

/// Return a process-lifetime isolated `HOME` shared by `spawn_bivvy_global`.
///
/// Global spawns have no associated project tempdir, so they share one
/// temp dir that lives for the whole test process.  The `OnceLock` is
/// intentionally leaked — cleanup happens when the OS reaps the test binary.
fn global_test_home() -> &'static Path {
    static HOME: OnceLock<TempDir> = OnceLock::new();
    HOME.get_or_init(|| TempDir::new().unwrap()).path()
}

/// Apply `HOME` + XDG env isolation to a [`Command`].
///
/// Sets every variable that any Bivvy code path (directly or via the `dirs`
/// crate) might read.  XDG vars are essential on Linux because
/// `dirs::data_local_dir()` — used by the feedback store — honors
/// `XDG_DATA_HOME`.  Without overriding them, a developer machine that has
/// `XDG_DATA_HOME` set in the shell would leak test state into the real
/// user's feedback store even though `HOME` was pinned.
fn apply_home_isolation(cmd: &mut Command, home: &Path) {
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", home.join(".local").join("share"));
    cmd.env("XDG_CACHE_HOME", home.join(".cache"));
    cmd.env("XDG_STATE_HOME", home.join(".local").join("state"));
}

// ── Project setup ────────────────────────────────────────────────────

/// Create a temporary project directory with `.bivvy/config.yml`.
///
/// Returns a `TempDir` whose lifetime controls cleanup.  Also creates
/// `<tempdir>/.test_home` so spawn helpers can automatically pin the child
/// process's `HOME` to it (see [`project_test_home`] and the module docs
/// on HOME isolation).
pub fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    // Pre-create the isolated HOME so the first spawn doesn't race on creation.
    project_test_home(temp.path());
    temp
}

/// Create a temporary project with a real git repository and project files.
///
/// Initialises a git repo with one commit containing `Cargo.toml`,
/// `Cargo.lock`, `src/main.rs`, `VERSION`, and `.env`.  This is the
/// baseline for any test that exercises git-dependent steps.
///
/// Git commands run with `HOME` pinned to the project's isolated test home
/// so that the developer's `~/.gitconfig` (e.g. `init.defaultBranch`, signing
/// keys, commit templates) cannot influence the fixture.
pub fn setup_project_with_git(config: &str) -> TempDir {
    let temp = setup_project(config);
    let home = project_test_home(temp.path());

    // Helper: build a git invocation with HOME pinned to the isolated test home.
    let git = |args: &[&str]| {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(temp.path());
        apply_home_isolation(&mut cmd, &home);
        // Also unset GIT_CONFIG_GLOBAL/SYSTEM if the user has them exported,
        // so git can't be pointed at an outside config by env.
        cmd.env_remove("GIT_CONFIG_GLOBAL");
        cmd.env_remove("GIT_CONFIG_SYSTEM");
        cmd
    };

    // Git repo
    git(&["init", "--initial-branch=main"])
        .output()
        .expect("git init failed");
    git(&["config", "user.email", "test@test.com"])
        .output()
        .expect("git config user.email failed");
    git(&["config", "user.name", "Test"])
        .output()
        .expect("git config user.name failed");
    // Disable GPG signing in case the user has commit.gpgsign=true globally;
    // the env isolation above already handles this, but be belt-and-braces.
    git(&["config", "commit.gpgsign", "false"])
        .output()
        .expect("git config commit.gpgsign failed");

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
    // Ignore the isolated test HOME so tests that run `git status --short`
    // as a step command don't see bivvy's own state files as untracked.
    fs::write(
        temp.path().join(".gitignore"),
        format!("{TEST_HOME_SUBDIR}/\n"),
    )
    .unwrap();

    git(&["add", "."]).output().expect("git add failed");
    git(&["commit", "-m", "Initial commit"])
        .output()
        .expect("git commit failed");

    temp
}

// ── Spawning ─────────────────────────────────────────────────────────
//
// All spawn helpers below automatically apply HOME + XDG isolation so tests
// never write to the real user's `~/.bivvy/` or feedback store.  Project
// spawns use `<project>/.test_home`; global spawns share a process-lifetime
// temp dir.  Callers can still override any env var via `spawn_bivvy_with_env`.

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
    apply_home_isolation(&mut cmd, &project_test_home(dir));
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(timeout));
    session
}

/// Spawn `bivvy` with extra environment variables.
///
/// HOME/XDG isolation is applied automatically.  If the caller passes a
/// `HOME` override in `env`, the XDG vars are re-derived from that HOME
/// (so state doesn't split across two unrelated directories).  All other
/// caller-provided vars are applied last and win via `Command::env`'s
/// last-write-wins semantics.
pub fn spawn_bivvy_with_env(
    args: &[&str],
    dir: &Path,
    env: &[(&str, &str)],
) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    // If the caller is overriding HOME, derive XDG from it so feedback-store
    // and history-store paths stay consistent.  Otherwise use the default
    // project-scoped test home.
    let caller_home = env
        .iter()
        .find_map(|(k, v)| (*k == "HOME").then_some(Path::new(*v)));
    let effective_home = caller_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_test_home(dir));
    apply_home_isolation(&mut cmd, &effective_home);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(TIMEOUT));
    session
}

/// Spawn `bivvy` without a project directory (for commands like `templates`,
/// `--help`, `--version`, `update`, `completions`).
///
/// Uses a process-lifetime shared test home so global state (update cache,
/// etc.) stays out of the real user environment.
pub fn spawn_bivvy_global(args: &[&str]) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    apply_home_isolation(&mut cmd, global_test_home());
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(SHORT_TIMEOUT));
    session
}

/// Spawn `bivvy` without a project directory, with HOME pinned to a
/// caller-supplied path.
///
/// Unlike [`spawn_bivvy_global`], which uses a shared process-lifetime
/// HOME, this helper lets each test own its own `TempDir` for the HOME.
/// That matters for tests that:
///
/// - Seed state before spawning (e.g. pre-populating the cache store or
///   feedback store via the library API), which requires knowing the
///   exact path bivvy will resolve.
/// - Assert on an *empty* starting store — a shared HOME would be
///   polluted by prior tests in the same process.
///
/// Full HOME + XDG isolation is applied, matching every other spawn
/// helper in this module.  Uses the short timeout since these commands
/// are generally non-interactive reads/writes of local state.
pub fn spawn_bivvy_with_home(args: &[&str], home: &Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    apply_home_isolation(&mut cmd, home);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(SHORT_TIMEOUT));
    session
}

/// Apply full HOME + XDG isolation to an `assert_cmd::Command`.
///
/// Shared by [`bivvy_assert_cmd`] and [`bivvy_assert_cmd_with_home`]
/// so the two entry points can't drift apart.
fn apply_home_isolation_assert(cmd: &mut assert_cmd::Command, home: &Path) {
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", home.join(".local").join("share"));
    cmd.env("XDG_CACHE_HOME", home.join(".cache"));
    cmd.env("XDG_STATE_HOME", home.join(".local").join("state"));
}

/// Build an `assert_cmd::Command` for `bivvy` with `cwd` and full HOME +
/// XDG isolation pre-applied.
///
/// Returned ready for `.args(...)`, `.assert()`, or `.output()` chaining
/// per the community Rust CLI testing norms.  Use this (instead of
/// rolling a raw `Command`) whenever you need exit-code, stdout, or
/// stderr predicates — the spawn helpers above return PTY sessions
/// which don't plug into `assert_cmd`/`predicates`.
///
/// The isolated HOME is the project's `<dir>/.test_home`, same as
/// [`spawn_bivvy`], so state written by an `assert_cmd` run is visible
/// to a subsequent PTY spawn in the same project — and vice versa.
pub fn bivvy_assert_cmd(dir: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("bivvy").unwrap();
    cmd.current_dir(dir);
    apply_home_isolation_assert(&mut cmd, &project_test_home(dir));
    cmd
}

/// Build an `assert_cmd::Command` for `bivvy` with HOME pinned to a
/// caller-supplied path — the `assert_cmd` analogue of
/// [`spawn_bivvy_with_home`].
///
/// Used by cache and feedback tests that seed state via the library
/// API before spawning, which requires the caller to know the exact
/// HOME path bivvy will resolve.  Full HOME + XDG isolation is applied
/// so bivvy's cache/feedback store lands inside the caller's tempdir
/// on both macOS and Linux.
pub fn bivvy_assert_cmd_with_home(home: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("bivvy").unwrap();
    apply_home_isolation_assert(&mut cmd, home);
    cmd
}

/// Run `bivvy run` non-interactively (stdout/stderr suppressed) and assert
/// it succeeds.  Useful for priming state before testing `last` / `history`.
pub fn run_workflow_silently(dir: &Path) {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(["run"])
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    apply_home_isolation(&mut cmd, &project_test_home(dir));
    let status = cmd.status().expect("Failed to run bivvy");
    assert!(status.success(), "bivvy run should succeed");
}

/// Run `bivvy run` non-interactively with custom args.
pub fn run_bivvy_silently(dir: &Path, args: &[&str]) {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    apply_home_isolation(&mut cmd, &project_test_home(dir));
    let status = cmd.status().expect("Failed to run bivvy");
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

/// Assert that the PTY process exited with the expected code.
///
/// Must be called after `read_to_eof` or `expect(Eof)` — the process
/// must have finished before we can retrieve its exit status.
pub fn assert_exit_code(s: &Session, expected: i32) {
    use expectrl::WaitStatus;
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, expected),
        "Expected exit code {expected}, got {status:?}"
    );
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
