//! End-to-end coverage for `bivvy last` × `StepOutcome`.
//!
//! Spawns `bivvy run` in a PTY against a workflow that produces all six
//! [`StepOutcomeKind`] variants in a single run, then invokes `bivvy last
//! --json` and asserts that each step's parsed outcome matches what the
//! runner emitted.
//!
//! Variants exercised:
//! - `Completed`   — a plain step that runs successfully
//! - `Failed`      — a step whose command exits non-zero (`allow_failure:
//!   true` so the recovery menu does not interrupt the PTY flow)
//! - `Satisfied`   — a step whose `check` passes (skipped without prompting)
//! - `Declined`    — a `confirm: true` step where the PTY answers "no"
//! - `FilteredOut` — a step removed at the workflow level via `--skip`
//! - `Blocked`     — a step whose precondition fails (independent of
//!   `Failed` so it triggers without depending on a real failure)
//!
//! [`StepOutcomeKind`]: bivvy::logging::StepOutcomeKind
#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use expectrl::Session;
use tempfile::TempDir;

const TEST_HOME_SUBDIR: &str = ".test_home";
const TIMEOUT: Duration = Duration::from_secs(60);

/// Pin `HOME` and the XDG base-directory variables to a sandbox so the
/// child process never reads or writes real user state.
fn apply_home_isolation(cmd: &mut Command, home: &Path) {
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("XDG_DATA_HOME", home.join(".local").join("share"));
    cmd.env("XDG_CACHE_HOME", home.join(".cache"));
    cmd.env("XDG_STATE_HOME", home.join(".local").join("state"));
}

/// Create a temporary project rooted at the returned tempdir, with the
/// given config written under `.bivvy/config.yml` and a sandbox home dir.
fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    fs::create_dir_all(temp.path().join(TEST_HOME_SUBDIR)).unwrap();
    temp
}

/// Spawn `bivvy` interactively in `dir` with HOME/XDG pinned to the
/// project's sandbox.
fn spawn_bivvy(args: &[&str], dir: &Path) -> Session {
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.current_dir(dir);
    apply_home_isolation(&mut cmd, &dir.join(TEST_HOME_SUBDIR));
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(TIMEOUT));
    session
}

/// Drain a PTY session until a marker substring is seen, returning the
/// accumulated bytes. The marker is matched after stripping ANSI escape
/// sequences so it is robust against styling.
fn drain_until(session: &Session, marker: &str, ctx: &str) -> Vec<u8> {
    let fd = session.get_stream().as_raw_fd();
    let mut accumulated = Vec::new();
    let start = Instant::now();

    loop {
        if start.elapsed() > TIMEOUT {
            panic!(
                "{ctx}: timed out waiting for marker {marker:?}\nGot: {}",
                String::from_utf8_lossy(&accumulated)
            );
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

        if strip_ansi(&accumulated).contains(marker) {
            // Drain a little more so we capture the prompt's full
            // rendering before the test sends keys back.
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                if libc::poll(&mut pfd, 1, 50) > 0 {
                    let mut buf = [0u8; 4096];
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
            return accumulated;
        }
    }
}

/// Strip ANSI CSI/OSC escape sequences for substring assertions.
fn strip_ansi(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if n.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if n == '\u{7}' {
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

const ARROW_DOWN: &[u8] = b"\x1b[B";

/// Write bytes to a PTY master fd. Partial-write robustness is unnecessary
/// for the short interactive payloads used here (a few escape sequences
/// and Enter), matching the convention in `tests/process_group_test.rs`.
fn send_bytes(session: &Session, bytes: &[u8]) {
    let fd = session.get_stream().as_raw_fd();
    // SAFETY: fd is a valid PTY master fd from the Session and `bytes`
    // outlives this call.
    unsafe {
        libc::write(fd, bytes.as_ptr() as *const _, bytes.len());
    }
}

/// Workflow with one step per `StepOutcomeKind` variant.
///
/// Order in the `default` workflow: satisfied → completed → declined →
/// filtered → failed → blocked. `step_filtered` is removed before the
/// loop runs via `--skip`, so the runtime sequence is:
///
/// 1. step_satisfied — auto-skipped, check passes (Satisfied)
/// 2. step_completed — runs, exits 0 (Completed)
/// 3. step_declined  — `confirm: true`, PTY answers "no" (Declined)
/// 4. step_failed    — runs, exits 1, `allow_failure: true` so the
///    recovery menu does not fire (Failed)
/// 5. step_blocked   — precondition fails (Blocked)
const SIX_OUTCOMES_CONFIG: &str = r#"
app_name: "OutcomeTest"

steps:
  step_satisfied:
    title: "Already satisfied"
    command: "true"
    check:
      type: execution
      command: "true"

  step_completed:
    title: "Completes"
    command: "true"

  step_declined:
    title: "Declined"
    command: "false"
    confirm: true

  step_filtered:
    title: "Filtered"
    command: "true"

  step_failed:
    title: "Fails"
    command: "false"
    allow_failure: true

  step_blocked:
    title: "Blocked by precondition"
    command: "true"
    precondition:
      type: presence
      target: "this/path/does/not/exist/anywhere"

workflows:
  default:
    steps:
      - step_satisfied
      - step_completed
      - step_declined
      - step_filtered
      - step_failed
      - step_blocked
"#;

/// Build a name → outcome map from `bivvy last --json` output.
///
/// Accepts the JSON tag form (`completed`, `failed`, `satisfied`,
/// `declined`, `filtered_out`, `blocked`).
fn collect_outcomes(json: &serde_json::Value) -> HashMap<String, String> {
    let steps = json
        .get("steps")
        .and_then(|v| v.as_array())
        .expect("`steps` array missing from `bivvy last --json` output");
    let mut out = HashMap::new();
    for step in steps {
        let name = step["name"]
            .as_str()
            .expect("step entry missing `name`")
            .to_string();
        let outcome = step["outcome"]
            .as_str()
            .expect("step entry missing `outcome`")
            .to_string();
        out.insert(name, outcome);
    }
    out
}

#[test]
fn bivvy_last_reports_all_six_step_outcome_variants() {
    let temp = setup_project(SIX_OUTCOMES_CONFIG);
    let project = temp.path();

    // ── Drive `bivvy run` interactively ──────────────────────────────
    let session = spawn_bivvy(&["run", "--skip", "step_filtered"], project);

    // The first interactive prompt is the "Declined?" confirm. Wait for
    // the prompt text, then navigate Down + Enter to pick "No". Both
    // Yes and No are in the rendered selector, so we anchor on a token
    // that only appears in this prompt.
    drain_until(&session, "Declined?", "confirm prompt for step_declined");
    send_bytes(&session, ARROW_DOWN);
    send_bytes(&session, b"\r");

    // Wait until the workflow finishes — the run path always prints a
    // closing hint at the end ("Run `bivvy run`" or "Run `bivvy
    // status`"). Either signals workflow completion.
    drain_until(&session, "bivvy", "post-run hint");

    drop(session);

    // ── Invoke `bivvy last --json` ───────────────────────────────────
    let bin = cargo_bin("bivvy");
    let mut cmd = Command::new(&bin);
    cmd.args(["last", "--json"]).current_dir(project);
    apply_home_isolation(&mut cmd, &project.join(TEST_HOME_SUBDIR));
    let output = cmd.output().expect("run `bivvy last`");
    assert!(
        output.status.success(),
        "`bivvy last` exited non-zero (status: {:?})\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("`bivvy last --json` did not emit JSON: {e}\nstdout: {stdout}"));

    // Workflow-level summary.
    assert_eq!(json["workflow"], "default", "wrong workflow name: {json:#}");
    assert_eq!(json["aborted"], false, "workflow should not have aborted");
    // The workflow as a whole is `success: false` because `step_blocked`
    // was blocked (and `step_failed`'s allow_failure: true does not
    // suppress the workflow-level failure flag).
    assert_eq!(
        json["success"], false,
        "workflow should report failed (blocked + allow_failure step) — got {json:#}"
    );

    let outcomes = collect_outcomes(&json);

    // Every variant must appear, mapped to the right step name.
    let expected: &[(&str, &str)] = &[
        ("step_satisfied", "satisfied"),
        ("step_completed", "completed"),
        ("step_declined", "declined"),
        ("step_filtered", "filtered_out"),
        ("step_failed", "failed"),
        ("step_blocked", "blocked"),
    ];
    for (name, expected_outcome) in expected {
        let got = outcomes.get(*name).unwrap_or_else(|| {
            panic!("step {name:?} missing from `bivvy last --json`. Got: {outcomes:?}")
        });
        assert_eq!(
            got, expected_outcome,
            "step {name:?}: expected outcome {expected_outcome:?}, got {got:?}\nFull outcomes: {outcomes:?}"
        );
    }

    // Every step name in the config must appear exactly once — no
    // dupes, no drops. (Ordering is not asserted because test fixtures
    // may evolve and the run-time UI does not promise it.)
    assert_eq!(
        outcomes.len(),
        expected.len(),
        "expected {} steps in `bivvy last --json`, got {}: {outcomes:?}",
        expected.len(),
        outcomes.len(),
    );
}
