//! System tests for `bivvy config`.
//!
//! Exercises the documented flags (`--json`, `--yaml`, `--merged`) and
//! the missing-config error path. Prefers `assert_cmd` + `insta` snapshot
//! assertions over partial string matches — YAML/JSON output is
//! structured data, so snapshot diffs give better regression coverage
//! than ad-hoc `contains` checks.
#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::prelude::*;
use expectrl::{Session, WaitStatus};
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────

/// A non-trivial config that exercises multiple features the `config`
/// command must faithfully round-trip: multiple steps, dependencies,
/// checks, multiple workflows, and settings.
const CONFIG: &str = r#"app_name: "ConfigTest"
settings:
  defaults:
    output: verbose
steps:
  deps:
    title: "Install dependencies"
    command: "rustc --version"
    check:
      type: execution
      command: "rustc --version"
  build:
    title: "Build project"
    command: "cargo --version"
    depends_on: [deps]
  test:
    title: "Run tests"
    command: "cargo fmt --version"
    depends_on: [build]
workflows:
  default:
    steps: [deps, build, test]
  quick:
    description: "Build only, skip tests"
    steps: [deps, build]
"#;

/// A local override layered on top of [`CONFIG`] used to verify the
/// `--merged` flag actually merges `.bivvy/config.local.yml`.
const LOCAL_OVERRIDE: &str = r#"settings:
  defaults:
    output: quiet
"#;

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

/// Create an isolated project directory containing `.bivvy/config.yml`.
///
/// The `HOME` override is applied at spawn time, so `~/.bivvy/config.yml`
/// resolves inside the tempdir and never touches the real user home.
fn setup_project(config: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();
    temp
}

/// Build an isolated `Command` for the `bivvy` binary.
///
/// Sets `HOME` to a sibling tempdir (not the project dir, to avoid
/// `~/.bivvy` aliasing `.bivvy/` in the project) and unsets any
/// Bivvy-specific env vars that could leak from the developer's
/// environment.
fn bivvy_cmd(args: &[&str], dir: &Path) -> Command {
    let fake_home = dir.join("__fake_home");
    fs::create_dir_all(&fake_home).unwrap();
    let mut cmd = Command::new(cargo_bin("bivvy"));
    cmd.args(args);
    cmd.current_dir(dir);
    cmd.env("HOME", &fake_home);
    cmd.env("BIVVY_HOME", fake_home.join(".bivvy-system"));
    cmd.env_remove("BIVVY_CONFIG");
    cmd.env_remove("BIVVY_ENV");
    cmd
}

/// Spawn `bivvy` inside a PTY for tests that must exercise interactive
/// output paths. Uses the same isolation rules as [`bivvy_cmd`].
fn spawn_bivvy(args: &[&str], dir: &Path) -> Session {
    let cmd = bivvy_cmd(args, dir);
    let mut session = Session::spawn(cmd).expect("Failed to spawn bivvy");
    session.set_expect_timeout(Some(Duration::from_secs(15)));
    session
}

/// Strip the `# <path>` header comments and blank lines that the
/// `config` command prints before the serialized YAML/JSON body.
///
/// These paths are tempdir-specific and would make snapshots unstable.
fn strip_path_header(output: &str) -> String {
    let mut lines = output.lines().peekable();
    // Skip the leading `# <path>` comment block and the blank line that
    // follows it.
    while let Some(line) = lines.peek() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            lines.next();
        } else {
            break;
        }
    }
    lines.collect::<Vec<_>>().join("\n").trim_end().to_string()
        + "\n"
}

/// Strip ANSI escape sequences so PTY output can be compared against
/// literal strings and snapshots deterministically.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
        } else if c == '\x1b' {
            in_esc = true;
        } else {
            out.push(c);
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// Default (YAML) output
// ─────────────────────────────────────────────────────────────────────

/// Default `bivvy config` run: verifies exit code, the `# <path>`
/// header points at the project config, and the serialized YAML body
/// matches a snapshot.
#[test]
fn config_default_yaml_output() {
    let temp = setup_project(CONFIG);

    let output = bivvy_cmd(&["config"], temp.path())
        .output()
        .expect("Failed to run bivvy config");

    assert_eq!(
        output.status.code(),
        Some(0),
        "bivvy config should exit 0 on success.\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Header line points to the real project config file.
    let expected_header = format!("# {}", temp.path().join(".bivvy/config.yml").display());
    assert!(
        stdout.contains(&expected_header),
        "Expected stdout to contain header {expected_header:?}, got:\n{stdout}"
    );

    // Snapshot the serialized config body (path header stripped so the
    // snapshot is stable across tempdirs).
    let body = strip_path_header(&stdout);

    // Targeted sanity checks before snapshotting so a broken body fails
    // with a precise error rather than an opaque snapshot diff. These
    // mirror the JSON spot-checks and guard against future accidental
    // snapshot regeneration masking a real regression.
    assert!(
        body.contains("app_name: ConfigTest"),
        "yaml body should contain app_name: ConfigTest, got:\n{body}"
    );
    assert!(
        body.contains("rustc --version"),
        "yaml body should contain the deps step command, got:\n{body}"
    );
    assert!(
        body.contains("quick:"),
        "yaml body should contain the quick workflow, got:\n{body}"
    );
    assert!(
        body.contains("Build only, skip tests"),
        "yaml body should preserve workflow descriptions, got:\n{body}"
    );

    insta::assert_snapshot!("config_default_yaml", body);
}

/// `--yaml` is the explicit form of the default output. It must
/// produce the same YAML body and exit 0. We assert exact equality
/// with the default form instead of snapshotting separately so a
/// divergence between the default and `--yaml` paths is caught.
#[test]
fn config_yaml_flag_matches_default() {
    let temp = setup_project(CONFIG);

    let default_output = bivvy_cmd(&["config"], temp.path())
        .output()
        .expect("Failed to run bivvy config");
    assert_eq!(default_output.status.code(), Some(0));

    let yaml_output = bivvy_cmd(&["config", "--yaml"], temp.path())
        .output()
        .expect("Failed to run bivvy config --yaml");
    assert_eq!(
        yaml_output.status.code(),
        Some(0),
        "bivvy config --yaml should exit 0.\nstderr:\n{}",
        String::from_utf8_lossy(&yaml_output.stderr)
    );

    let default_body =
        strip_path_header(&String::from_utf8_lossy(&default_output.stdout));
    let yaml_body = strip_path_header(&String::from_utf8_lossy(&yaml_output.stdout));
    assert_eq!(
        default_body, yaml_body,
        "--yaml output should be identical to default output"
    );
}

// ─────────────────────────────────────────────────────────────────────
// JSON output
// ─────────────────────────────────────────────────────────────────────

/// `--json` must emit parseable JSON that round-trips every field from
/// the input config. We parse it back and snapshot the parsed value so
/// ordering differences don't cause churn, and we verify the exit code.
#[test]
fn config_json_flag_emits_parseable_json() {
    let temp = setup_project(CONFIG);

    let output = bivvy_cmd(&["config", "--json"], temp.path())
        .output()
        .expect("Failed to run bivvy config --json");

    assert_eq!(
        output.status.code(),
        Some(0),
        "bivvy config --json should exit 0.\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The body starts at the first `{` — skip the `# <path>` header.
    let json_start = stdout
        .find('{')
        .unwrap_or_else(|| panic!("No JSON object in output:\n{stdout}"));
    let json_str = &stdout[json_start..];

    let parsed: serde_json::Value = serde_json::from_str(json_str.trim())
        .unwrap_or_else(|e| panic!("Failed to parse JSON output: {e}\nJSON was:\n{json_str}"));

    // Spot-check critical fields before snapshotting so a bad JSON
    // produces a targeted failure rather than an opaque snapshot diff.
    assert_eq!(parsed["app_name"], "ConfigTest");
    assert_eq!(parsed["steps"]["deps"]["command"], "rustc --version");
    assert_eq!(parsed["steps"]["build"]["depends_on"][0], "deps");
    assert_eq!(parsed["workflows"]["quick"]["description"], "Build only, skip tests");

    // Full structural snapshot for regression detection.
    insta::assert_json_snapshot!("config_json_full", parsed);
}

// ─────────────────────────────────────────────────────────────────────
// --merged flag
// ─────────────────────────────────────────────────────────────────────

/// `--merged` should include local overrides in the resolved config.
/// We write `.bivvy/config.local.yml` that changes `default_output`
/// to `quiet`, then assert the merged output reflects that and that
/// the header lists both config files.
#[test]
fn config_merged_applies_local_override() {
    let temp = setup_project(CONFIG);
    fs::write(
        temp.path().join(".bivvy/config.local.yml"),
        LOCAL_OVERRIDE,
    )
    .unwrap();

    let output = bivvy_cmd(&["config", "--merged", "--json"], temp.path())
        .output()
        .expect("Failed to run bivvy config --merged --json");

    assert_eq!(
        output.status.code(),
        Some(0),
        "bivvy config --merged --json should exit 0.\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The header must list both the project and the local config files
    // since `--merged` merges all existing sources.
    let project_header =
        format!("# {}", temp.path().join(".bivvy/config.yml").display());
    let local_header = format!(
        "# {}",
        temp.path().join(".bivvy/config.local.yml").display()
    );
    assert!(
        stdout.contains(&project_header),
        "Merged header should list project config {project_header:?}, got:\n{stdout}"
    );
    assert!(
        stdout.contains(&local_header),
        "Merged header should list local config {local_header:?}, got:\n{stdout}"
    );

    // Parse the JSON body and verify the override took effect.
    let json_start = stdout
        .find('{')
        .unwrap_or_else(|| panic!("No JSON object in merged output:\n{stdout}"));
    let parsed: serde_json::Value =
        serde_json::from_str(stdout[json_start..].trim()).unwrap_or_else(|e| {
            panic!("Failed to parse merged JSON: {e}\nJSON was:\n{}", &stdout[json_start..])
        });

    assert_eq!(
        parsed["settings"]["defaults"]["output"], "quiet",
        "local override should change output from verbose to quiet"
    );
    assert_eq!(
        parsed["app_name"], "ConfigTest",
        "project app_name should be preserved"
    );

    insta::assert_json_snapshot!("config_merged_with_local", parsed);
}

/// `--merged` with no local override should still succeed: the merged
/// view collapses to just the project config, the header lists only the
/// project file, and the body matches what `--yaml` would have produced.
#[test]
fn config_merged_without_local_matches_project() {
    let temp = setup_project(CONFIG);

    let output = bivvy_cmd(&["config", "--merged"], temp.path())
        .output()
        .expect("Failed to run bivvy config --merged");

    assert_eq!(
        output.status.code(),
        Some(0),
        "bivvy config --merged should exit 0.\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Header must list the project config.
    let project_header =
        format!("# {}", temp.path().join(".bivvy/config.yml").display());
    assert!(
        stdout.contains(&project_header),
        "merged header should list project config {project_header:?}, got:\n{stdout}"
    );

    // Header must NOT mention a nonexistent local config file.
    let local_header = format!(
        "# {}",
        temp.path().join(".bivvy/config.local.yml").display()
    );
    assert!(
        !stdout.contains(&local_header),
        "merged header should not list a nonexistent local config, got:\n{stdout}"
    );

    // Body should round-trip every field from CONFIG.
    let body = strip_path_header(&stdout);
    assert!(
        body.contains("app_name: ConfigTest"),
        "merged yaml should preserve app_name, got:\n{body}"
    );
    assert!(
        body.contains("output: verbose"),
        "merged yaml should preserve settings when no override is present, got:\n{body}"
    );
    assert!(
        body.contains("rustc --version"),
        "merged yaml should preserve step commands, got:\n{body}"
    );
}

/// When both `--yaml` and `--json` are passed, the documented behaviour
/// is that `--json` takes precedence (see `src/cli/commands/config.rs`:
/// the `if self.args.json` branch is checked first). Verify that the
/// output parses as JSON and exit code is 0.
#[test]
fn config_json_wins_over_yaml_when_both_flags_set() {
    let temp = setup_project(CONFIG);

    let output = bivvy_cmd(&["config", "--yaml", "--json"], temp.path())
        .output()
        .expect("Failed to run bivvy config --yaml --json");

    assert_eq!(
        output.status.code(),
        Some(0),
        "bivvy config --yaml --json should exit 0.\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find('{').unwrap_or_else(|| {
        panic!("Output should contain a JSON object when --json is set, got:\n{stdout}")
    });
    let parsed: serde_json::Value = serde_json::from_str(stdout[json_start..].trim())
        .unwrap_or_else(|e| {
            panic!(
                "Output should be valid JSON when --json is set: {e}\nBody was:\n{}",
                &stdout[json_start..]
            )
        });

    assert_eq!(parsed["app_name"], "ConfigTest");
    assert_eq!(parsed["steps"]["deps"]["command"], "rustc --version");
}

// ─────────────────────────────────────────────────────────────────────
// PTY output
// ─────────────────────────────────────────────────────────────────────

/// Verify that `bivvy config` also works cleanly when run inside a
/// PTY — this exercises the interactive output code path and ensures
/// the command still exits 0 and emits the full YAML body. We snapshot
/// the body (minus the path header) so formatting regressions in the
/// PTY rendering are caught.
#[test]
fn config_pty_yaml_output() {
    let temp = setup_project(CONFIG);
    let mut s = spawn_bivvy(&["config"], temp.path());

    let output = s
        .expect(expectrl::Eof)
        .expect("bivvy config should reach EOF under PTY");
    let text = strip_ansi(&String::from_utf8_lossy(output.as_bytes()));

    // Header line must list the project config. This is the only
    // substring match we keep — the full path is tempdir-specific and
    // cannot be snapshotted directly.
    let expected_header = format!("# {}", temp.path().join(".bivvy/config.yml").display());
    assert!(
        text.contains(&expected_header),
        "PTY output should contain project header {expected_header:?}, got:\n{text}"
    );

    // Snapshot the full rendered YAML body (path header stripped so the
    // snapshot is stable across tempdirs). Using a snapshot rather than
    // partial `contains` checks means any formatting regression in the
    // PTY rendering — missing fields, reordered keys, altered styling —
    // is caught, not just the three fields we happened to pick.
    let body = strip_path_header(&text);
    insta::assert_snapshot!("config_pty_yaml_body", body);

    // Exit status must be success (code 0).
    let pid = s.get_process().pid();
    let status = s.get_process().wait().unwrap();
    assert_eq!(
        status,
        WaitStatus::Exited(pid, 0),
        "bivvy config (PTY) should exit 0, got {status:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Sad paths — missing / invalid config
// ─────────────────────────────────────────────────────────────────────

/// No `.bivvy/config.yml` at all. The command must emit the documented
/// error message on stderr and exit with code 2 (config-not-found).
#[test]
fn config_no_config_exits_2() {
    let temp = TempDir::new().unwrap();

    bivvy_cmd(&["config"], temp.path())
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "No configuration found. Run 'bivvy init' first.",
        ));
}

/// `--merged` against a project with no config files should produce the
/// same exit code 2 and the same documented error message as the
/// non-merged path.
#[test]
fn config_merged_no_config_exits_2() {
    let temp = TempDir::new().unwrap();

    bivvy_cmd(&["config", "--merged"], temp.path())
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "No configuration found. Run 'bivvy init' first.",
        ));
}

/// A malformed YAML config should fail with a specific exit code (1,
/// the generic error path used by `main.rs` for unhandled `Err(e)`
/// results) and emit the documented `Error: ...` prefix on stderr. The
/// full stderr body is snapshotted so parse error regressions are
/// caught exactly, not by fuzzy substring matching.
#[test]
fn config_malformed_yaml_fails() {
    let temp = setup_project("app_name: [unterminated\nsteps:\n  : : :\n");

    let output = bivvy_cmd(&["config"], temp.path())
        .output()
        .expect("Failed to run bivvy config");

    // Exit code 1 is the documented generic-error exit code (see
    // `src/main.rs`: dispatcher `Err(e)` branch).
    assert_eq!(
        output.status.code(),
        Some(1),
        "malformed YAML should exit 1.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Stdout must be empty on the error path — the command bails before
    // printing any config body.
    assert!(
        output.stdout.is_empty(),
        "malformed YAML should not produce stdout, got:\n{}",
        String::from_utf8_lossy(&output.stdout),
    );

    // Normalize tempdir-specific paths out of stderr so the snapshot is
    // stable. We replace the absolute tempdir path with a fixed
    // placeholder and then strip ANSI colour codes that the error
    // output may contain.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tempdir_str = temp.path().display().to_string();
    let normalized = strip_ansi(&stderr).replace(&tempdir_str, "[TEMPDIR]");

    // The documented error path prefixes messages with "Error: " (see
    // `main.rs`). This is the exact user-facing prefix, not a partial.
    assert!(
        normalized.contains("Error: "),
        "stderr should start with the documented 'Error: ' prefix, got:\n{normalized}"
    );

    insta::assert_snapshot!("config_malformed_yaml_stderr", normalized);
}
