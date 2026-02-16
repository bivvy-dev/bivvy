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
| `bivvy status` | Pre-flight check |
| `bivvy list` | Show steps and workflows |
| `bivvy lint` | Validate configuration |
| `bivvy last` | Show last run info |
| `bivvy history` | Show execution history |

## Configuration Schema

```yaml
app_name: "MyApp"

settings:
  default_output: verbose    # verbose | quiet | silent
  logging: false

steps:
  step_name:
    template: template_name  # OR define inline
    command: "..."
    depends_on: [other_step]
    completed_check:
      type: file_exists | command_succeeds | marker
      path: "..."
      command: "..."
    watches:
      - file_to_watch.lock

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
    <rule>Use `completed_check` to detect if work is already done</rule>
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
