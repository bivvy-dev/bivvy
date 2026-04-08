# Bivvy Development Review

Run the checks in each of the following categories, and update the "Result" column with either PASS or FAIL. When you're done with all categories, output all of the checks tables with their results.

## Commits

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| Subject in imperative mood ("Add feature" not "Added feature") | | |
| Subject 50 characters or less | | |
| Subject first letter capitalized | | |
| No period at end of subject | | |
| No type prefixes (no "feat:", "fix:", etc.) | | |
| Blank line between subject and body | | |
| Body wrapped at 72 characters | | |
| Body explains what and why, not how | | |
| No commits created without accompanying tests | | |
| No large commits with multiple unrelated changes | | |

## Documentation — User-Facing (`docs/`)

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| New commands, features, or config options have corresponding documentation in `docs/` | | |
| All user-facing documentation lives in `docs/` | | |
| Focuses on what Bivvy does and how to use it, not how it's built | | |
| Follows the structure in `docs/SUMMARY.md` | | |

## Documentation — Developer-Facing (`src/`)

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| `///` and `//!` doc comments on all public items (structs, functions, modules) | | |
| Developer documentation added and/or updated in source READMEs | | |
| `README.md` files in source directories document subsystem design | | |
| Source READMEs cover purpose, key modules, design decisions, testing guidance, guiding principles | | |
| Source READMEs focus on "why" and "how it fits together," API details left to rustdoc | | |

## Coding Standards — Error Handling

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| `anyhow::Result` used for application errors | | |
| `thiserror` used for library/domain errors | | |
| Error messages are informative, actionable, and include context | | |

## Coding Standards — Output

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| `--verbose`, `--quiet`, `--non-interactive` flags supported | | |
| Secrets masked in all output (`*_KEY`, `*_SECRET`, `*_TOKEN`, etc.) | | |

## Coding Standards — Idempotency

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| Steps are safe to re-run | | |
| `completed_check` used to detect if work is already done | | |
| User prompted before re-running completed steps (unless `--force`) | | |

## Doc Test Guidelines

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| Doc tests are self-contained and runnable with real test fixtures | | |
| `no_run` used only for code that truly can't run in tests (interactive terminal, network) | | |
| `ignore` used only for pseudocode that won't compile, with explanation | | |
| `ignore` or `no_run` never used just because setup is tedious | | |

## Rust CLI Testing Norms

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| No skipping or disabling tests to make them pass | | |
| No partial strings used in `expect()` calls (full user-facing messages used) - always use full messages | | |
| `parse_from()` used instead of `parse()` in tests and doc examples | | |
| `Cli::parse()` only used in actual `main()` function | | |
| `assert_cmd` + `predicates` used for integration tests | | |
| `tempfile::TempDir` used for all filesystem side effects | | |
| `insta` used for snapshot testing output formats | | |
| CLI binary tested as subprocess (`Command::cargo_bin`), not by calling internal functions | | |
| PTY/interactive tests use `rexpect` or `expectrl` with timeouts | | |
| Tests isolated from user environment (`HOME` set to temp dir, env vars cleared) | | |
| `--help` output tested with snapshots | | |
| `--json` output parsed and asserted on structure/values, not string matched | | | |

## System Tests

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| No `.ok()` on expect/assertion results (silent swallow) | | |
| No `if s.expect(...).is_ok()` for conditional test logic | | |
| No tests that only assert `expect(Eof)` without content assertions | | |
| Exit codes verified for commands that document them (0, 1, 2, 130) | | |
| No config constants or test fixtures that are unused or only partially exercised | | |

## System Test Goals

| Check | Result | Notes (file/location, line, other identifying details) |
|-------|--------|--------------------- |
| System tests should cover all documented behavior (flags, output formats, exit codes, error messages, config options) | | |
| Each test should test the full interaction pattern at every step of the way - outputs, inputs, prompts, actions, and outcomes | | |
| Interactive flows exercised (keystrokes sent, prompts verified, selections produce expected results) | | |
| Error conditions tested (invalid configs, missing files, failed commands, malformed input) | | |
| Side effects verified (files created, state modified, structured output parsed and checked) | | |
| Follow-up actions after prompt acceptance have their outcomes asserted | | |
| Multi-phase commands have each phase independently verified | | |
| Real-world commands used in tests (`git`, `rustc`, `cargo`, `node`, `python`) — no `echo`, `ls`, `cat`, `true`, or any other built-in terminal commands | | |
| Configs exercise features under test (dependencies, watches, completed checks, templates, workflows) | | |
