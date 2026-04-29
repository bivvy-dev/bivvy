# Bivvy Development Guidelines

> Cross-language development environment setup automation, built in Rust.

## Project Overview

Bivvy is a CLI tool that replaces ad-hoc `bin/setup` scripts with a declarative YAML configuration and a polished interactive CLI experience. It uses a **Template Registry** instead of hardcoded language logic, allowing user overrides at every level.

<references>
  <reference path="plans/bivvy-plan.md">Full implementation specification</reference>
  <reference path="plans/README.md">Milestone overview and status</reference>
  <reference path=".claude/bivvy-dev-workflow.md">Development workflow (FOLLOW THIS)</reference>
</references>

## Development Workflow

**Follow the workflow defined in `.claude/bivvy-dev-workflow.md` for all development work.**

<workflow-summary>
  <step ref="specification">Write failing tests first</step>
  <step ref="implementation">Implement minimum code to pass tests</step>
  <step ref="documentation">Document while context is fresh (rustdoc for APIs, `docs/` for user-facing changes, source READMEs for design)</step>
  <step ref="linting">Pass `cargo fmt` and `cargo clippy`</step>
  <step ref="testing">Pass all tests with >90% coverage</step>
  <step ref="build">Verify clean build</step>
  <step ref="commit">Create atomic commit with code + tests + docs</step>
</workflow-summary>

<verification-commands>
  <command>cargo fmt -- --check</command>
  <command>cargo clippy --all-targets --all-features -- -D warnings</command>
  <command>cargo test --all-features</command>
  <command>cargo build --all-targets --all-features</command>
  <command>cargo build --release</command>
  <command>cargo llvm-cov --all-features --fail-under-lines 90</command>
</verification-commands>

<critical>
  ALWAYS run `cargo test --all-features` without any filters.

  CORRECT:   cargo test --all-features
  WRONG:     cargo test config
  WRONG:     cargo test schema
  WRONG:     cargo test --lib
  WRONG:     cargo test some_module::

  Every verification must run the ENTIRE test suite with all features.
  Never filter to "just the relevant tests" - ALL tests must pass before every commit.

  If you see "X filtered out" in test output, you ran the wrong command.

  The --all-features and --all-targets flags match what CI runs. If you skip
  them locally, you may push code that fails CI.
</critical>

<prohibited-behaviors>
  <behavior>Skipping, disabling, or modifying tests to make them pass</behavior>
  <behavior>Creating commits without accompanying tests</behavior>
  <behavior>Moving on to other work when tests are failing</behavior>
  <behavior>Large commits with multiple unrelated changes</behavior>
  <behavior>Straying from the implementation plan without explicit approval</behavior>
  <behavior>Filtering tests when not debugging (e.g., `cargo test config` instead of `cargo test`)</behavior>
  <behavior>Using partial strings in expect() calls instead of the actual user-facing messages</behavior>
</prohibited-behaviors>

## Architecture

<philosophy>
Bivvy is a **declarative orchestrator**, not a collection of hardcoded scripts:
  <principle>Uses a Template Registry instead of hardcoded language logic</principle>
  <principle>Optionally injects pre-defined YAML templates during `bivvy init`</principle>
  <principle>Allows user overrides at every level</principle>
  <principle>Tracks execution state to enable smart re-runs</principle>
</philosophy>

### Supported Platforms

- macOS (arm64, x64)
- Linux (arm64, x64)
- Windows (x64)

### Key Dependencies

```toml
clap = { version = "4", features = ["derive", "env"] }   # CLI framework
dialoguer = "0.12"                                        # Interactive prompts
indicatif = "0.18"                                        # Progress indicators
console = "0.16"                                          # Terminal styling
serde = { version = "1", features = ["derive"] }          # Serialization
marked_yaml = "0.8"                                       # YAML with source locations
anyhow = "1"                                              # Error handling
thiserror = "2"                                           # Custom errors
git2 = "0.20"                                             # Git integration
reqwest = { version = "0.13", features = ["blocking"] }   # HTTP client
```

### File Locations

<file-structure>
  <location name="project" path=".bivvy/">
    <file>config.yml</file>
    <file>config.local.yml (gitignored)</file>
    <directory>templates/</directory>
  </location>
  <location name="system" path="~/.bivvy/">
    <file>config.yml</file>
    <directory>templates/</directory>
    <directory>projects/{id}/</directory>
    <directory>cache/</directory>
  </location>
</file-structure>

## CLI Commands

| Command | Description |
|---------|-------------|
| `bivvy` / `bivvy run` | Run default workflow |
| `bivvy init` | Initialize configuration |
| `bivvy add <template>` | Add a template step to config |
| `bivvy templates` | List available templates |
| `bivvy status` | Pre-flight check |
| `bivvy list` | Show steps and workflows |
| `bivvy lint` | Validate configuration |
| `bivvy last` | Show last run info |
| `bivvy history` | Show execution history |

## Configuration Schema

```yaml
app_name: "MyApp"

settings:
  defaults:
    output: verbose    # verbose | quiet | silent
  logging: false

steps:
  step_name:
    template: template_name  # OR define inline
    command: "..."
    depends_on: [other_step]
    check:
      type: presence | execution | change
      target: "..."
      command: "..."

workflows:
  default:
    steps: [step1, step2, step3]
```

## Documentation

Bivvy has two distinct documentation audiences. Keep them separate.

<documentation>
  <audience name="user-facing" path="docs/">
    <description>Pages served by Starlight on Bivvy's website. Written for end users.</description>
    <rule>All user-facing documentation lives in `docs/`</rule>
    <rule>Write for someone who has never seen the source code</rule>
    <rule>Focus on what Bivvy does and how to use it, not how it's built</rule>
    <rule>Follow the structure in `docs/SUMMARY.md`</rule>
    <rule>When adding a new command, feature, or config option, add or update the corresponding `docs/` page</rule>
  </audience>

  <audience name="dev-facing" path="src/">
    <description>Developer documentation lives inside the source tree. Written for contributors.</description>
    <form name="rustdoc">
      <rule>Add `///` and `//!` doc comments to all public items (structs, functions, modules)</rule>
      <rule>Primary way to document what code does and how to use it</rule>
    </form>
    <form name="source-readmes">
      <description>Higher-level `README.md` files in source directories that document subsystem design. Example: `src/ui/README.md` documents CLX design norms, color assignments, and spinner lifecycle.</description>
      <rule>Place `README.md` files in source directories to document subsystem design (e.g., `src/cli/README.md`, `src/config/README.md`)</rule>
      <rule>Cover: purpose, key modules/files, design decisions, testing guidance, guiding principles</rule>
      <rule>Focus on the "why" and "how it fits together" — leave API details to rustdoc</rule>
    </form>
  </audience>

  <boundaries>
    <rule>Do NOT put developer documentation in `docs/` — that directory is exclusively for end users</rule>
    <rule>Do NOT put user-facing documentation in source READMEs — point to `docs/` instead</rule>
  </boundaries>
</documentation>

## Coding Standards

<standards>
  <standard name="error-handling">
    <rule>Use `anyhow::Result` for application errors</rule>
    <rule>Use `thiserror` for library/domain errors</rule>
    <rule>Provide actionable error messages with context</rule>
  </standard>

  <standard name="output">
    <rule>Support `--verbose`, `--quiet`, `--non-interactive` flags</rule>
    <rule>Mask secrets in all output (`*_KEY`, `*_SECRET`, `*_TOKEN`, etc.)</rule>
    <rule>Use spinners and progress indicators from `indicatif`</rule>
  </standard>

  <standard name="idempotency">
    <rule>Steps should be safe to re-run</rule>
    <rule>Use `check`/`checks` to detect if work is already done</rule>
    <rule>Prompt before re-running completed steps (unless `--force`)</rule>
  </standard>
</standards>

## Testing Patterns

<test-layers>
  <layer name="Unit" tools="cargo test" speed="Fast">
    Individual functions, parsing, logic
  </layer>
  <layer name="Integration" tools="assert_cmd, tempfile" speed="Medium">
    CLI commands, file I/O, config loading
  </layer>
  <layer name="Snapshot" tools="insta" speed="Fast">
    CLI output format, error messages
  </layer>
</test-layers>

<cli-testing>
  Use `parse_from()` instead of `parse()` in tests and doc examples:

  ```rust
  // WRONG - reads actual process args, fails in tests
  let cli = Cli::parse();

  // CORRECT - explicit args, works in tests
  let cli = Cli::parse_from(["bivvy", "run", "--verbose"]);
  ```

  This applies to:
  - Unit tests for CLI parsing
  - Doc tests / examples in rustdoc
  - Any test that needs to verify CLI behavior

  Only use `Cli::parse()` in the actual `main()` function.
</cli-testing>

<doc-test-guidelines>
  Doc tests should be self-contained and runnable whenever possible.

  PREFER runnable examples with real test fixtures:
  ```rust
  /// ```
  /// use bivvy::config::load_merged_config;
  /// use tempfile::TempDir;
  /// use std::fs;
  ///
  /// // Create real test files
  /// let temp = TempDir::new().unwrap();
  /// fs::create_dir_all(temp.path().join(".bivvy")).unwrap();
  /// fs::write(temp.path().join(".bivvy/config.yml"), "app_name: test").unwrap();
  ///
  /// // Test with real data
  /// let config = load_merged_config(temp.path()).unwrap();
  /// assert_eq!(config.app_name, Some("test".to_string()));
  /// ```
  ```

  Use `no_run` ONLY for code that truly can't run in tests (interactive terminal
  input, network calls to real servers, etc.):
  ```rust
  /// ```no_run
  /// // Requires interactive terminal input
  /// let answer = ui.prompt_confirm("Continue?")?;
  /// ```
  ```

  Use `ignore` only for pseudocode that won't compile. Always explain why:
  ```rust
  /// ```ignore
  /// // Pseudocode showing concept - yaml() helper doesn't exist
  /// let config = yaml("key: value");
  /// ```
  ```

  NEVER use `ignore` or `no_run` just because setup is tedious - use tempfile!
</doc-test-guidelines>

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_merge_replaces_at_conflict_point() {
        // Test actual merge behavior
    }
}
```

### Integration Test Example

```rust
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn init_creates_config_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    let mut cmd = Command::cargo_bin("bivvy")?;
    cmd.arg("init")
        .arg("--non-interactive")
        .current_dir(&temp)
        .assert()
        .success();

    assert!(temp.path().join(".bivvy/config.yml").exists());
    Ok(())
}
```

### Test Utilities

Create helpers in `tests/common/mod.rs`:

```rust
use tempfile::TempDir;

pub fn setup_project(config: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    std::fs::create_dir_all(temp.path().join(".bivvy"))?;
    std::fs::write(temp.path().join(".bivvy/config.yml"), config)?;
    Ok(temp)
}
```

### System Test Quality Standards

These rules apply to all PTY-based system tests (`tests/system_*.rs`). They exist
because a prior audit (`SYSTEM-TEST-AUDIT.md`) found that ~40-50% of system tests
were functionally vacuous — they could not fail unless the binary panicked.

<system-test-anti-patterns>
  <anti-pattern name="silent-swallow">
    NEVER use `.ok()` on expect/assertion results. This silently discards failures
    and makes the test pass regardless of output. Every `expect()` must propagate
    or `unwrap()`.

    NEVER use `if s.expect(...).is_ok()` to conditionally execute test logic.
    This makes the entire block optional — if the expected output never appears,
    the test silently skips the interaction and passes vacuously. If the prompt
    is expected, use `.unwrap()`. If the prompt is genuinely conditional, assert
    on the alternative path too.

    ```rust
    // WRONG — test passes even if "Step completed" never appears
    session.expect("Step completed").ok();

    // WRONG — if "Run setup now" never appears, the send("y") is skipped
    // and the test passes without ever testing the run path
    if s.expect("Run setup now").is_ok() {
        s.send("y").unwrap();
    }

    // CORRECT — test fails if pattern is missing
    session.expect("Step completed").unwrap();

    // CORRECT — prompt must appear, and the response is tested
    s.expect("Run setup now").unwrap();
    s.send("y").unwrap();
    ```
  </anti-pattern>

  <anti-pattern name="eof-only">
    A test that only asserts `expect(Eof)` verifies nothing beyond "the process
    eventually exits." Always assert on specific output content before checking EOF.

    ```rust
    // WRONG — proves nothing about behavior
    session.expect(Eof).unwrap();

    // CORRECT — verify behavior, then clean exit
    session.expect("3 steps passed").unwrap();
    session.expect(Eof).unwrap();
    ```
  </anti-pattern>

  <anti-pattern name="no-exit-code">
    Always verify exit codes for commands that document them. Bivvy defines exit
    codes 0, 1, 2, and 130 — these must be tested.

    ```rust
    // CORRECT — verify the process exit status
    let status = session.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    ```
  </anti-pattern>

  <anti-pattern name="dead-test-data">
    Do not define config constants or test fixtures that are never used or only
    partially exercised. If a constant exists, it should be meaningfully tested.
  </anti-pattern>
</system-test-anti-patterns>

<system-test-goals>
  <goal name="assert-on-content">
    Every test must make at least one assertion on specific output content — a
    string, pattern, or structured value. A test with no content assertion is not
    a test.
  </goal>

  <goal name="doc-coverage">
    System tests should cover documented behavior. When docs describe flags,
    output formats, exit codes, error messages, or config options, there should
    be a corresponding test. Reference `SYSTEM-TEST-AUDIT.md` for current gaps.
  </goal>

  <goal name="sad-paths">
    Test error conditions, not just happy paths. Invalid configs, missing files,
    failed commands, and malformed input should produce specific, documented error
    messages and correct exit codes.
  </goal>

  <goal name="side-effect-verification">
    For commands that create files, modify state, or produce structured output
    (JSON, YAML), read the output back and verify its content — don't just check
    that the process exited.
  </goal>

  <goal name="interactive-workflows">
    PTY tests should exercise real interactive flows: send keystrokes, verify
    prompts appear, confirm the right options are presented, and check that
    selections produce the expected result. Don't just spawn and wait for EOF.

    When a test accepts a prompt that triggers a follow-up action (e.g., "Run
    setup now?" → "y"), the test MUST assert on the outcome of that action —
    success output, error messages, exit code, or side effects. Accepting a
    prompt and then only checking EOF proves nothing about whether the triggered
    action worked.
  </goal>

  <goal name="multi-phase-commands">
    Some commands have multiple phases (e.g., `init` creates config then
    optionally runs setup; `add` modifies config then optionally runs the step).
    Each phase that executes must be independently verified. Asserting on phase
    1 output does not cover phase 2 — if a test triggers phase 2, it must assert
    on phase 2's outcome (success, failure, output, exit code, side effects).

    ```rust
    // WRONG — verifies init but not the run it triggered
    s.expect("Created .bivvy/config.yml").unwrap();
    s.expect("Run setup now").unwrap();
    s.send("y").unwrap();
    s.expect(expectrl::Eof).unwrap(); // ← run could have failed

    // CORRECT — verifies both phases
    s.expect("Created .bivvy/config.yml").unwrap();
    s.expect("Run setup now").unwrap();
    s.send("y").unwrap();
    s.expect("steps passed").unwrap(); // ← phase 2 outcome
    s.expect(expectrl::Eof).unwrap();
    ```
  </goal>

  <goal name="real-commands">
    Test with real-world commands that exercise actual tool behavior — `git`,
    `rustc`, `cargo`, `node`, `python`, etc. Never use `echo`, `ls`, `cat`,
    `true`, or other shell builtins as stand-ins for real step commands. Bivvy
    orchestrates development environment setup; tests should reflect that. A step
    running `echo hello` proves nothing about how Bivvy handles real tool output,
    exit codes, timing, or failure modes.
  </goal>

  <goal name="realistic-configs">
    Use configs that exercise the features under test: dependencies, watches,
    completed checks, templates, environment constraints, workflows with multiple
    steps. Trivial single-step configs miss the interesting behavior.
  </goal>
</system-test-goals>

<rust-cli-testing-norms>
  Community-established patterns for testing Rust CLI tools:

  <norm>Use `assert_cmd` + `predicates` for integration tests — assert on stdout,
  stderr, and exit code in a single chain</norm>
  <norm>Use `tempfile::TempDir` for all filesystem side effects — never write to
  the real filesystem or rely on cwd</norm>
  <norm>Use `insta` for snapshot testing output formats — catches unintended
  regressions in human-readable output</norm>
  <norm>Test the CLI binary as a subprocess (`Command::cargo_bin`), not by calling
  internal functions — this catches arg parsing, output formatting, and exit code
  bugs that unit tests miss</norm>
  <norm>For PTY/interactive tests, use `rexpect` or `expectrl` and always set
  timeouts — a missing prompt should fail fast, not hang forever</norm>
  <norm>Isolate tests from user environment: set `HOME` to a temp dir, clear
  interfering env vars, don't depend on global state</norm>
  <norm>Test `--help` output with snapshots — flag renames and description changes
  are regressions too</norm>
  <norm>For commands with `--json` output, parse the JSON and assert on structure
  and values, not string matching</norm>
</rust-cli-testing-norms>

## Git Workflow

### Commit Message Format

<commit-format>
  <subject>
    <rule>Imperative mood ("Add feature" not "Added feature")</rule>
    <rule>50 characters or less</rule>
    <rule>Capitalize first letter</rule>
    <rule>No period at the end</rule>
    <rule>No type prefixes (no "feat:", "fix:", etc.)</rule>
  </subject>
  <body>
    <rule>Blank line between subject and body</rule>
    <rule>Wrap at 72 characters</rule>
    <rule>Explain what and why, not how</rule>
  </body>
</commit-format>

```
Add BivvyError enum with thiserror

Centralizes error handling with typed variants for
config parsing, template resolution, and execution
failures. Each variant includes context for debugging.
```

## What NOT to Do

<anti-patterns>
  <anti-pattern>Straying from `plans/` unless addressing an open question</anti-pattern>
  <anti-pattern>Over-engineering or adding features not in the plan</anti-pattern>
  <anti-pattern>Creating the full project structure upfront</anti-pattern>
  <anti-pattern>Committing without tests</anti-pattern>
  <anti-pattern>Committing with failing tests</anti-pattern>
  <anti-pattern>Skipping or disabling tests to make them pass</anti-pattern>
  <anti-pattern>Large commits with multiple concerns</anti-pattern>
  <anti-pattern>Using deprecated APIs (check dependency versions)</anti-pattern>
</anti-patterns>
