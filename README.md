# Bivvy

[![CI](https://github.com/bivvy-dev/bivvy/actions/workflows/test.yml/badge.svg)](https://github.com/bivvy-dev/bivvy/actions/workflows/test.yml)
[![coverage](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/brennacodes/02a14079a59edfce0d250a030c8e0662/raw/bivvy-coverage.json)](https://github.com/bivvy-dev/bivvy/actions/workflows/test.yml)

> Cross-language development environment setup automation, built in Rust.

Bivvy replaces ad-hoc `bin/setup` scripts with declarative YAML configuration, smart state tracking, and a polished CLI.

## Installation

Quick install (macOS/Linux):

```bash
curl -fsSL https://bivvy.dev/install | sh
```

Homebrew:

```bash
brew install https://raw.githubusercontent.com/bivvy-dev/bivvy/main/dist/homebrew/bivvy.rb
```

Cargo:

```bash
cargo install bivvy
```

gem:

```bash
gem install bivvy
```

pip:

```bash
pip install bivvy
```

See [Installation docs](docs/installation/index.md) for details on each method.

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
    template: bundle-install
    watches: [Gemfile.lock]
  node:
    template: yarn-install
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
| `bivvy` / `bivvy run` | Run setup workflow |
| `bivvy init` | Initialize configuration |
| `bivvy add <template>` | Add a template step to configuration |
| `bivvy status` | Show current status |
| `bivvy list` | List steps and workflows |
| `bivvy lint` | Validate configuration |
| `bivvy last` | Show last run info |
| `bivvy history` | Show execution history |
| `bivvy config` | Show resolved configuration |
| `bivvy templates` | List available templates |
| `bivvy cache` | Manage template cache |
| `bivvy feedback` | Capture and manage feedback |
| `bivvy completions` | Generate shell completions |
| `bivvy update` | Check for and install updates |

## Documentation

- [Why Bivvy?](docs/why-bivvy.md)
- [Configuration](docs/configuration/index.md)
- [Templates](docs/templates/index.md)
- [CLI Reference](docs/commands/index.md)
- [Guides](docs/SUMMARY.md)

## Screenshots
<img width="463" height="302" alt="Screenshot 2026-04-30 at 8 12 04 PM" src="https://github.com/user-attachments/assets/467fcee9-7503-4785-a9d9-57f0839cf991" />
<br/>
<img width="534" height="489" alt="Screenshot 2026-04-30 at 7 31 20 PM" src="https://github.com/user-attachments/assets/9445aa71-3294-4a79-9b40-1ddab337d66a" />
<br/>
<img width="910" height="740" alt="Screenshot 2026-04-30 at 7 32 27 PM" src="https://github.com/user-attachments/assets/46ffe44c-f19e-4a73-bef5-c7087e7d8c41" />



## Shell Completions

```bash
bivvy completions bash > ~/.local/share/bash-completion/completions/bivvy
```

```bash
bivvy completions zsh > ~/.zfunc/_bivvy
```

```bash
bivvy completions fish > ~/.config/fish/completions/bivvy.fish
```

```powershell
bivvy completions powershell > $PROFILE.CurrentUserCurrentHost
```

```bash
bivvy completions elvish > ~/.local/share/elvish/lib/bivvy.elv
```

## Supported Platforms

| Platform | Architecture |
|----------|--------------|
| Linux | x64, arm64 |
| macOS | x64, arm64 |
| Windows | x64 |

## License

[FSL-1.1-Apache-2.0](LICENSE) - Functional Source License with Apache 2.0 future license (converts to Apache 2.0 after 2 years)
