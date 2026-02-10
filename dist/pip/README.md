# Bivvy

[![CI](https://github.com/bivvy-dev/bivvy/actions/workflows/test.yml/badge.svg)](https://github.com/bivvy-dev/bivvy/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/bivvy-dev/bivvy/branch/main/graph/badge.svg)](https://codecov.io/gh/bivvy-dev/bivvy)

> Cross-language development environment setup automation, built in Rust.

Bivvy replaces ad-hoc `bin/setup` scripts with declarative YAML configuration, smart state tracking, and a polished CLI.

## Installation

```bash
pip install bivvy
```

## Quick Start

```bash
cd my-project
bivvy init
```

```bash
bivvy run
```

```bash
bivvy status
```

## What It Does

```yaml
# .bivvy/config.yml
app_name: myapp

steps:
  brew:
    template: brew
  ruby:
    template: bundler
    watches: [Gemfile.lock]
  node:
    template: yarn
    watches: [yarn.lock]
  db:
    command: "rails db:prepare"
    depends_on: [ruby]

workflows:
  default:
    steps: [brew, ruby, node, db]
```

- **State tracking** — only re-runs what's needed
- **Watch files** — detects when dependencies change
- **Error recovery** — retry, fix, skip, or drop to shell
- **Template registry** — reusable, shareable step definitions
- **Remote sources** — central team templates via HTTP or Git
- **Secret masking** — sensitive values hidden in output
- **Multiple output formats** — human, JSON, SARIF for lint

## Commands

| Command | Description |
|---------|-------------|
| `bivvy run` | Run setup workflow |
| `bivvy init` | Initialize configuration |
| `bivvy status` | Show current status |
| `bivvy list` | List steps and workflows |
| `bivvy lint` | Validate configuration |
| `bivvy last` | Show last run info |
| `bivvy history` | Show execution history |
| `bivvy config` | Show resolved configuration |
| `bivvy cache` | Manage template cache |
| `bivvy feedback` | Capture friction points |
| `bivvy completions` | Generate shell completions |

## Documentation

Full documentation available at [bivvy.dev](https://bivvy.dev)

- [Configuration](https://github.com/bivvy-dev/bivvy/blob/main/docs/configuration/index.md)
- [Templates](https://github.com/bivvy-dev/bivvy/blob/main/docs/templates/index.md)
- [CLI Reference](https://github.com/bivvy-dev/bivvy/blob/main/docs/commands/index.md)

## Supported Platforms

| Platform | Architecture |
|----------|--------------|
| Linux | x64, arm64 |
| macOS | x64, arm64 |
| Windows | x64 |

## License

[FSL-1.1-Apache-2.0](https://github.com/bivvy-dev/bivvy/blob/main/LICENSE) - Functional Source License with Apache 2.0 future license (converts to Apache 2.0 after 2 years)
