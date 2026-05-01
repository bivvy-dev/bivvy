//! End-to-end behavior tests for `template_sources` in `bivvy add`,
//! `bivvy templates`, and `bivvy run`.
//!
//! These tests run the real `bivvy` binary as a subprocess against a
//! mocked HTTP registry (or a `git daemon` serving a local bare repo for
//! the Git path), then assert on stdout, stderr, exit codes, and the
//! resulting `.bivvy/config.yml` contents.
//
// `cargo_bin` is marked deprecated in favor of the macro form, but both
// work; suppressing until assert_cmd stabilizes the new API.
#![allow(deprecated)]

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command as AssertCommand;
use httpmock::prelude::*;
use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command as StdCommand, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Write `.bivvy/config.yml` containing `body` under `temp` and return `temp`.
fn project_with_config(body: &str) -> TempDir {
    let temp = TempDir::new().expect("create tempdir");
    let bivvy_dir = temp.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).expect("mkdir .bivvy");
    fs::write(bivvy_dir.join("config.yml"), body).expect("write config.yml");
    temp
}

/// Add a project-local template at `.bivvy/templates/steps/<name>.yml`.
fn write_local_template(temp: &TempDir, file_name: &str, body: &str) {
    let dir = temp.path().join(".bivvy").join("templates").join("steps");
    fs::create_dir_all(&dir).expect("mkdir templates/steps");
    fs::write(dir.join(file_name), body).expect("write local template");
}

/// Build a `bivvy` Command rooted at `temp`.
///
/// `HOME` is pointed at the tempdir so user-level templates and config in
/// the developer's real `~/.bivvy/` cannot leak into the test. The default
/// stdin is the test process's stdin, which assert_cmd discards unless the
/// caller writes to it; `add` and `templates` don't prompt unless an
/// ambiguity arises, so this is enough for non-interactive runs.
fn bivvy_cmd(temp: &TempDir) -> AssertCommand {
    let mut cmd = AssertCommand::new(cargo_bin("bivvy"));
    cmd.current_dir(temp.path()).env("HOME", temp.path());
    cmd
}

/// Read back the project config after a command run.
fn read_config(temp: &TempDir) -> String {
    fs::read_to_string(temp.path().join(".bivvy/config.yml"))
        .expect("read config.yml after command")
}

// =========================================================================
// `bivvy add` against remote sources
// =========================================================================

/// `bivvy add <name>` succeeds when `<name>` only exists in a configured
/// remote HTTP template source. The config is rewritten with the new step
/// referencing that template.
#[test]
fn add_resolves_template_from_remote_http_source() {
    let server = MockServer::start();
    let template_yaml = r#"
name: company-bootstrap
description: "Company-wide bootstrap step"
category: company
step:
  command: company-bootstrap --setup
"#;
    server.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(template_yaml);
    });

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
        server.url("/templates.yml")
    );
    let temp = project_with_config(&config);

    bivvy_cmd(&temp)
        .args(["add", "company-bootstrap"])
        .assert()
        .success();

    let after = read_config(&temp);
    assert!(
        after.contains("  company-bootstrap:\n    template: company-bootstrap\n"),
        "step block not added; config:\n{after}"
    );
}

/// A project-local template shadows a remote template that shares the
/// same name. Verify by giving the local and remote different commands;
/// `add` should pick up the local one and surface its `command` in the
/// generated comment.
#[test]
fn local_template_shadows_remote_with_same_name() {
    let server = MockServer::start();
    let remote_yaml = r#"
name: shared-tool
description: "REMOTE shared tool"
category: tools
step:
  command: REMOTE-VARIANT-RUNS
"#;
    server.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(remote_yaml);
    });

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
        server.url("/templates.yml")
    );
    let temp = project_with_config(&config);

    write_local_template(
        &temp,
        "shared-tool.yml",
        r#"name: shared-tool
description: "LOCAL shared tool"
category: tools
step:
  command: LOCAL-VARIANT-RUNS
"#,
    );

    bivvy_cmd(&temp)
        .args(["add", "shared-tool"])
        .assert()
        .success();

    let after = read_config(&temp);
    assert!(
        after.contains("# command: LOCAL-VARIANT-RUNS"),
        "expected local template's command in generated comment; got:\n{after}"
    );
    assert!(
        !after.contains("REMOTE-VARIANT-RUNS"),
        "remote template should have been shadowed by local; got:\n{after}"
    );
}

/// When two remote sources both define the same template, the source with
/// the lower `priority` number wins. Set up two MockServers and assert on
/// which command appears in the generated step block.
#[test]
fn lower_priority_remote_wins_on_collision() {
    let high_priority = MockServer::start();
    let low_priority = MockServer::start();

    high_priority.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(
            r#"
name: shared-tool
description: "high-priority variant"
category: tools
step:
  command: HIGH-PRIORITY-COMMAND
"#,
        );
    });

    low_priority.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(
            r#"
name: shared-tool
description: "low-priority variant"
category: tools
step:
  command: LOW-PRIORITY-COMMAND
"#,
        );
    });

    // Lower number = higher priority. List the loser first to prove
    // priority — not declaration order — is what wins.
    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
    priority: 100
  - url: "{}"
    priority: 10
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
        low_priority.url("/templates.yml"),
        high_priority.url("/templates.yml"),
    );
    let temp = project_with_config(&config);

    bivvy_cmd(&temp)
        .args(["add", "shared-tool"])
        .assert()
        .success();

    let after = read_config(&temp);
    assert!(
        after.contains("# command: HIGH-PRIORITY-COMMAND"),
        "priority=10 source should win over priority=100; got:\n{after}"
    );
    assert!(
        !after.contains("LOW-PRIORITY-COMMAND"),
        "low-priority source leaked through; got:\n{after}"
    );
}

/// `bivvy add` rejects names that already exist in the project's
/// `.bivvy/config.yml`, regardless of whether the template comes from a
/// remote source.
#[test]
fn add_fails_when_step_name_collides_with_existing() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(
            r#"
name: shared-tool
description: "Remote shared tool"
category: tools
step:
  command: shared-tool run
"#,
        );
    });

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
steps:
  shared-tool:
    command: echo already here
workflows:
  default:
    steps: [shared-tool]
"#,
        server.url("/templates.yml")
    );
    let temp = project_with_config(&config);

    bivvy_cmd(&temp)
        .args(["add", "shared-tool"])
        .assert()
        .failure()
        .code(1);

    // Config should NOT have been modified — the original line still wins.
    let after = read_config(&temp);
    assert!(
        after.contains("    command: echo already here"),
        "original step body should be preserved on failure; got:\n{after}"
    );
}

/// An unreachable remote (server not listening) should not break `bivvy
/// add` for templates that exist as built-ins. The remote is logged as a
/// warning and skipped; the built-in registry still resolves.
#[test]
fn unreachable_remote_falls_back_to_builtins() {
    // Pick a port that nothing is listening on. We let the OS allocate
    // one, immediately drop the listener, and reuse the URL for a
    // guaranteed connection-refused.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);
    let dead_url = format!("http://127.0.0.1:{dead_port}/templates.yml");

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{dead_url}"
    timeout: 2
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#
    );
    let temp = project_with_config(&config);

    // bundle-install is a built-in template; it should still resolve even
    // though the remote source is unreachable.
    bivvy_cmd(&temp)
        .args(["add", "bundle-install"])
        .timeout(std::time::Duration::from_secs(15))
        .assert()
        .success();

    let after = read_config(&temp);
    assert!(
        after.contains("template: bundle-install"),
        "built-in template should still be added when remote is unreachable; got:\n{after}"
    );
}

/// Qualified `category/name` syntax routes through the remote source when
/// the category matches what the remote declares.
#[test]
fn qualified_category_name_resolves_remote_template() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(
            r#"
name: special-tool
description: "Qualified-name remote tool"
category: very-specific-category
step:
  command: special-tool --run
"#,
        );
    });

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#,
        server.url("/templates.yml")
    );
    let temp = project_with_config(&config);

    bivvy_cmd(&temp)
        .args(["add", "very-specific-category/special-tool"])
        .assert()
        .success();

    let after = read_config(&temp);
    assert!(
        after.contains("template: very-specific-category/special-tool"),
        "qualified template reference should be written verbatim; got:\n{after}"
    );
}

// =========================================================================
// `bivvy templates` against remote sources
// =========================================================================

/// `bivvy templates` lists templates discovered from a remote source
/// alongside built-ins.
#[test]
fn templates_command_lists_remote_templates() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(
            r#"
name: super-unique-listed-tool
description: "Surfaced by bivvy templates"
category: tools
step:
  command: super-unique-listed-tool run
"#,
        );
    });

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
"#,
        server.url("/templates.yml")
    );
    let temp = project_with_config(&config);

    let output = bivvy_cmd(&temp)
        .arg("templates")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.contains("super-unique-listed-tool"),
        "remote template missing from `bivvy templates` listing:\n{stdout}"
    );
    assert!(
        stdout.contains("Surfaced by bivvy templates"),
        "remote template description missing from listing:\n{stdout}"
    );
}

// =========================================================================
// `bivvy run` against remote sources
// =========================================================================

/// Full lifecycle: configure a remote source, declare a step that
/// references its template, and run the workflow. The remote template's
/// command is what actually executes.
#[test]
fn run_executes_step_resolved_from_remote_template() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/templates.yml");
        then.status(200).body(
            r#"
name: remote-runner
description: "Runs via a remote template"
category: tools
step:
  command: "echo REMOTE-TEMPLATE-RAN"
"#,
        );
    });

    let config = format!(
        r#"app_name: Test
template_sources:
  - url: "{}"
steps:
  go:
    template: remote-runner
workflows:
  default:
    steps: [go]
"#,
        server.url("/templates.yml")
    );
    let temp = project_with_config(&config);

    let output = bivvy_cmd(&temp)
        .args(["run", "--non-interactive"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("REMOTE-TEMPLATE-RAN"),
        "remote template's command did not execute; output:\n{combined}"
    );
}

// =========================================================================
// Git remote source — end-to-end via `git daemon`
// =========================================================================

/// Wrapper around a spawned `git daemon` child process that kills the
/// daemon on drop, even if the test panics.
struct GitDaemon {
    child: Child,
}

impl Drop for GitDaemon {
    fn drop(&mut self) {
        // Best-effort kill; the daemon doesn't hold any state we care
        // about preserving on cleanup.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Pick a free TCP port by binding to `:0` and immediately releasing.
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind to ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

/// Wait up to `deadline` for `port` to start accepting TCP connections.
fn wait_for_port(port: u16, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(75));
    }
    false
}

/// Spawn `git daemon` serving any `*.git` repo under `base_path` over
/// `git://localhost:<port>/`. Returns a guard that kills the daemon on
/// drop.
fn spawn_git_daemon(base_path: &Path, port: u16) -> Option<GitDaemon> {
    let child = StdCommand::new("git")
        .args([
            "daemon",
            "--reuseaddr",
            "--export-all",
            "--informative-errors",
            &format!("--base-path={}", base_path.display()),
            &format!("--port={port}"),
            base_path.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    if wait_for_port(port, Duration::from_secs(5)) {
        Some(GitDaemon { child })
    } else {
        // Daemon never came up. Still need to reap the child.
        let _ = {
            let mut c = child;
            c.kill().ok();
            c.wait()
        };
        None
    }
}

/// Initialize a bare repo at `<base>/<name>.git` with the given files
/// committed on `main`. Returns the bare path.
fn seed_bare_repo(base: &Path, name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let bare = base.join(format!("{name}.git"));
    let work = base.join(format!("{name}-work"));
    fs::create_dir_all(&work).expect("mkdir work");

    let run = |args: &[&str], dir: Option<&Path>| {
        let mut cmd = StdCommand::new("git");
        cmd.args(args);
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        let status = cmd.status().expect("git command");
        assert!(status.success(), "git {args:?} failed");
    };

    run(
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            bare.to_string_lossy().as_ref(),
        ],
        None,
    );
    run(
        &[
            "clone",
            bare.to_string_lossy().as_ref(),
            work.to_string_lossy().as_ref(),
        ],
        None,
    );
    run(&["config", "user.name", "Test"], Some(&work));
    run(&["config", "user.email", "test@test.com"], Some(&work));

    for (rel, content) in files {
        let dest = work.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).expect("mkdir for template");
        }
        fs::write(&dest, content).expect("write template");
    }

    run(&["add", "."], Some(&work));
    run(&["commit", "-m", "Seed templates"], Some(&work));
    run(&["push", "origin", "HEAD:main"], Some(&work));

    // git daemon's default `daemon.uploadpack` config requires this marker
    // file to exist before it'll serve the repo when `--export-all` isn't
    // sufficient on some platforms; create it as a belt-and-suspenders.
    fs::write(bare.join("git-daemon-export-ok"), "").ok();

    bare
}

/// Skip the test (with a printed reason) if `git daemon` is unavailable
/// on this system. `git` is required to build bivvy's deps so the binary
/// itself is always present, but the daemon subcommand may not be.
fn git_daemon_available() -> bool {
    StdCommand::new("git")
        .args(["daemon", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `bivvy add` resolves templates from a Git source served over
/// `git://localhost:<port>/<repo>.git`. Mirrors the HTTP happy-path test
/// to prove the Git wiring works end-to-end through the CLI binary.
#[test]
fn add_resolves_template_from_git_source_over_daemon() {
    if !git_daemon_available() {
        eprintln!("skipping: git daemon not available on this system");
        return;
    }

    let temp = TempDir::new().expect("tempdir");

    let template_yaml = r#"
name: git-served-tool
description: "Served via git daemon"
category: tools
step:
  command: git-served-tool --setup
"#;
    seed_bare_repo(
        temp.path(),
        "templates-repo",
        &[("git-served-tool.yml", template_yaml)],
    );

    let port = pick_free_port();
    let _daemon = match spawn_git_daemon(temp.path(), port) {
        Some(d) => d,
        None => {
            eprintln!("skipping: git daemon failed to start on port {port}");
            return;
        }
    };

    // Project root must be separate from the daemon's base_path so the
    // bare repo isn't accidentally treated as a project.
    let project = TempDir::new().expect("project tempdir");
    let bivvy_dir = project.path().join(".bivvy");
    fs::create_dir_all(&bivvy_dir).unwrap();
    let config = format!(
        r#"app_name: Test
template_sources:
  - type: git
    url: "git://127.0.0.1:{port}/templates-repo.git"
    ref: main
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
"#
    );
    fs::write(bivvy_dir.join("config.yml"), config).unwrap();

    let mut cmd = AssertCommand::new(cargo_bin("bivvy"));
    cmd.current_dir(project.path())
        .env("HOME", project.path())
        .args(["add", "git-served-tool"])
        .timeout(Duration::from_secs(20))
        .assert()
        .success();

    let after = fs::read_to_string(project.path().join(".bivvy/config.yml")).unwrap();
    assert!(
        after.contains("  git-served-tool:\n    template: git-served-tool\n"),
        "step block not added; config:\n{after}"
    );
    assert!(
        after.contains("# command: git-served-tool --setup"),
        "git template's command should appear in generated comment; got:\n{after}"
    );
}
