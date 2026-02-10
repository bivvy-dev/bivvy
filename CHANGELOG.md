# Changelog

All notable changes to Bivvy will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - TBD

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
- Distribution packages (curl, npm, gem, pip, Homebrew)
- Shell completions (bash, zsh, fish, PowerShell)

[Unreleased]: https://github.com/bivvy-dev/bivvy/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/bivvy-dev/bivvy/releases/tag/v1.0.0
