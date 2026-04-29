# Changelog

All notable changes to Bivvy will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project (mostly) adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-release versions are < 2.0.0.

## [Unreleased] - 1.9.0

### Added
- Portable workflow files: `.bivvy/workflows/<name>.yml` can now carry its own `steps:` and `vars:` blocks alongside a `workflow:` declaration. Drop a self-contained workflow into a project and `bivvy run <name>` works without further setup. Legacy workflow files (with `description` + an ordered `steps:` list) keep working unchanged
- `Discovery` module for cheap filename-based enumeration of `.bivvy/workflows/` and `.bivvy/steps/` plus light header parsing of workflow files
- Per-command load profiles: `load_project_config`, `load_single_workflow_file`, `load_single_step_file`, and `load_for_run`/`load_for_run_with_trust` so each command pays only for the files it actually needs
- `bivvy schema` command: outputs JSON Schema to stdout or `--output` file
- `bivvy snapshot` command: capture, list, and delete execution snapshots
- JSON Schema generation with `schemars` derive macros on all config types; schema embedded at compile time via `include_str!`
- Global config bootstrapping: `~/.bivvy/config.yml` created on first run with commented-out examples of all settings
- JSON Schema moved to global config directory (`~/.bivvy/schema.json`), rewritten on every invocation to stay current after upgrades
- Docker daemon detection in diagnostic funnel: suggests `open -a Docker` (macOS) or `systemctl start docker` (Linux) for connection-refused errors
- Split-file steps and workflows: define steps/workflows in individual files under `.bivvy/steps/` and `.bivvy/workflows/`; filename stem becomes the key
- `check`/`checks` fields on step config as the new check system (replaces `completed_check`)
- `satisfied_when` field on steps with ref and inline check support
- `settings.defaults` section for project-wide behavior defaults (`auto_run`, `prompt_on_rerun`, `rerun_window`)
- Decision engine for auto-running steps (`auto_run` separated from `prompt_on_rerun`)
- Diagnostic funnel pipeline
- Structured event logging module with event emission wired throughout codebase
- Snapshot store for check baselines and execution state
- Error patterns for Corepack and pg_dump version mismatch
- `TemplateName` enum for validated template references
- Deprecated fields lint rule detecting `completed_check`, `watches`, `marker`, and legacy `precondition` usage with migration suggestions
- Mutual exclusivity lint for `check`/`checks` vs legacy check fields
- 21 missing requirements registered with `CommandSucceeds` checks, install hints, and dependency chains
- Insta snapshot testing support
- Audited system tests with artifact audit templates for 11 ecosystems
- `--clear` flag added to `bivvy history` command to clear run history for a project

### Changed
- `bivvy run <workflow>` now performs a two-phase load: phase 1 reads only `.bivvy/config.yml` to resolve the workflow name; phase 2 deep-merges with only the named workflow file in the chain. Sibling workflow files are not parsed, so a malformed neighbor cannot break a run of an unrelated workflow
- `bivvy lint` now requires a target. Bare `bivvy lint` lints `.bivvy/config.yml` only. `bivvy lint <name>` resolves to `.bivvy/workflows/<name>.yml` first, then `.bivvy/steps/<name>.yml`. `--workflow <name>` and `--step <name>` are explicit disambiguators. `--all` opts into the legacy full-merge behavior
- `bivvy list` now uses cheap discovery by default — workflow file bodies are not parsed, only headers (description + step ordering). Pass a positional workflow name to load that single file in detail, or `--all` to restore the legacy full-merge behavior
- `bivvy add` now uses the project-only loader instead of walking the full merge chain
- `bivvy status` accepts an optional positional workflow name; with `<workflow>`, workflow-bundled steps are visible
- Generated JSON Schema now rejects unknown fields. Editors with the YAML language server flag typos and unsupported keys (e.g., `dark_mode` under `settings`) instead of silently accepting them. Runtime deserialization remains tolerant of unknown fields under structs that use `#[serde(flatten)]` (`settings`, `steps.*`) for backward compatibility; everywhere else, unknown fields also produce a parse error
- Persistent workflow progress bar pinned at terminal bottom using `MultiProgress`; step output scrolls above while the bar updates in place
- Separated `StepManager` from workflow orchestration: step-level logic (prompts, execution, recovery, error display) extracted into dedicated module; workflow layer owns only sequencing, filtering, and aggregate state
- Environment name merged into run header line (`env: X`) instead of a separate line
- System redesign: decomposed `workflow.rs` into focused modules, unified output through `OutputWriter` trait, wired `CheckEvaluator` into orchestrator, added deprecation and migration support
- `StepConfig` broken into logical sub-structs
- `ResolvedStep` broken into matching sub-structs
- `Settings` refactored into grouped sub-structs
- `UserInterface` split into six focused sub-traits
- Error patterns refactored into declarative ecosystem modules
- Consumer signatures narrowed to minimal trait bounds (`OutputWriter`)
- PTY unsafe code consolidated into fewer, narrower unsafe blocks

### Removed
- 7 unnecessary dependencies replaced with hand-rolled implementations
- Legacy code removed as part of system redesign

### Fixed
- Docker daemon connection-refused errors now produce actionable recovery suggestions instead of a generic menu
- Bundler recovery bugs and version resolver
- Process drop in zsh
- Template mapping
- Command output in error blocks no longer shows internal exit code
- Rerun check logic
- Cache boundary in `GapChecker`
- Formatting after `StepConfig` refactor
- Run summary no longer shows redundant `satisfied:` prefix inside `Check passed (...)`
- Steps that were check-passed (skipped because already satisfied) no longer overwrite their last-run timestamp, so subsequent runs report the actual last execution time instead of the last evaluation time
- Run summary footer's "already satisfied" count now correctly reflects check-passed steps

## [1.0.0]

### Added
- Core CLI commands: run, init, status, list, lint, last, history, config, cache
- Template registry with built-in language templates
- YAML configuration with deep merge and validation
- Step execution with dependency resolution
- Workflow orchestration with hooks
- Lint engine with human, JSON, and SARIF output
- Auto-fix for common configuration issues
- Secret masking in output
- Shell refresh detection and resume
- Remote config fetching with extends
- Environment variable layering
- .env file loading
- Sensitive step handling
- Remote template sources (HTTP, Git)
- Template caching with TTL/ETag revalidation
- Cache CLI commands
- Install method detection
- Version checking and auto-update
- Session tracking and feedback command
- Distribution packages (curl, gem, pip, Homebrew)
- Shell completions (bash, zsh, fish, PowerShell)

[Unreleased]: https://github.com/bivvy-dev/bivvy/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/bivvy-dev/bivvy/releases/tag/v1.0.0
