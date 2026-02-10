# Built-in Templates

Bivvy includes 20 built-in templates for common development tools. When you run `bivvy init`, these are auto-detected based on files in your project.

## System Package Managers

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `brew` | macOS, Linux | `Brewfile` | `brew bundle install` |
| `apt` | Linux | `apt-get` available | `sudo apt-get install -y` |
| `yum` | Linux | `yum` available | `sudo yum install -y` |
| `pacman` | Linux | `pacman` available | `sudo pacman -S --noconfirm` |

## Windows Package Managers

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `chocolatey` | Windows | `choco` available | `choco install -y` |
| `scoop` | Windows | `scoop` available | `scoop install` |

## Version Managers

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `mise` | macOS, Linux, Windows | `.mise.toml`, `mise.toml` | `mise install` |
| `asdf` | macOS, Linux | `.tool-versions` | `asdf install` |
| `volta` | macOS, Linux, Windows | `volta` available | `volta install node` |

## Ruby

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `bundler` | macOS, Linux, Windows | `Gemfile` | `bundle install` |

## Node.js

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `yarn` | macOS, Linux, Windows | `yarn.lock` | `yarn install` |
| `npm` | macOS, Linux, Windows | `package-lock.json` | `npm install` |
| `pnpm` | macOS, Linux, Windows | `pnpm-lock.yaml` | `pnpm install` |
| `bun` | macOS, Linux, Windows | `bun.lockb` | `bun install` |

## Python

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `pip` | macOS, Linux, Windows | `requirements.txt` | `pip install -r requirements.txt` |
| `poetry` | macOS, Linux, Windows | `poetry.lock` | `poetry install` |
| `uv` | macOS, Linux, Windows | `uv.lock` | `uv sync` |

## Rust

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `cargo` | macOS, Linux, Windows | `Cargo.toml` | `cargo build` |

## Go

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `go` | macOS, Linux, Windows | `go.mod` | `go mod download` |

## Swift

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `swift` | macOS, Linux | `Package.swift` | `swift package resolve` |

## Example Usage

```yaml
steps:
  deps:
    template: brew
  ruby:
    template: bundler
    depends_on: [deps]
  node:
    template: yarn
    depends_on: [deps]
```

## Template Details

Each template provides:

- **Command** - The shell command to run
- **Completed check** - How to tell if the step already ran (skip if so)
- **Watches** - Files that trigger a re-run when changed
- **Environment impact** - PATH or shell changes the step makes

### brew

Installs Homebrew packages from a Brewfile.

- **Platforms**: macOS, Linux
- **Detects**: `Brewfile`
- **Command**: `brew bundle install`
- **Completion check**: `brew bundle check`
- **Watches**: `Brewfile`, `Brewfile.lock.json`

### bundler

Installs Ruby gems from a Gemfile.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Gemfile`
- **Command**: `bundle install`
- **Completion check**: `bundle check`
- **Watches**: `Gemfile`, `Gemfile.lock`

### yarn

Installs Node.js dependencies using Yarn.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `yarn.lock`, `package.json`
- **Command**: `yarn install`
- **Completion check**: `yarn check --verify-tree`
- **Watches**: `yarn.lock`, `package.json`
- **Environment**: Sets `NODE_ENV=development`

### npm

Installs Node.js dependencies using npm.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `package-lock.json`, `package.json`
- **Command**: `npm install`
- **Completion check**: `node_modules` directory exists
- **Watches**: `package.json`, `package-lock.json`
- **Environment**: Sets `NODE_ENV=development`

### pnpm

Installs Node.js dependencies using pnpm.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pnpm-lock.yaml`
- **Command**: `pnpm install`
- **Completion check**: `node_modules` directory exists
- **Watches**: `package.json`, `pnpm-lock.yaml`
- **Environment**: Sets `NODE_ENV=development`

### bun

Installs Node.js dependencies using Bun.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `bun.lockb`
- **Command**: `bun install`
- **Completion check**: `node_modules` directory exists
- **Watches**: `package.json`, `bun.lockb`

### volta

Installs pinned Node.js version using Volta.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `volta` command available
- **Command**: `volta install node`
- **Completion check**: `volta which node`
- **Watches**: `package.json`

### mise

Installs tool versions using mise.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.mise.toml`, `mise.toml`
- **Command**: `mise install`
- **Completion check**: `mise current`
- **Watches**: `.mise.toml`, `mise.toml`

### asdf

Installs tool versions using asdf.

- **Platforms**: macOS, Linux
- **Detects**: `.tool-versions`
- **Command**: `asdf install`
- **Completion check**: `asdf current`
- **Watches**: `.tool-versions`

### pip

Installs Python packages from requirements.txt.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `requirements.txt`
- **Command**: `pip install -r requirements.txt`
- **Completion check**: `pip check`
- **Watches**: `requirements.txt`

### poetry

Installs Python dependencies using Poetry.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `poetry.lock`
- **Command**: `poetry install`
- **Completion check**: `poetry check`
- **Watches**: `pyproject.toml`, `poetry.lock`

### uv

Syncs Python dependencies using uv.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `uv.lock`
- **Command**: `uv sync`
- **Completion check**: `.venv` directory exists
- **Watches**: `pyproject.toml`, `uv.lock`

### cargo

Builds a Rust project using Cargo.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Cargo.toml`
- **Command**: `cargo build`
- **Completion check**: `target` directory exists
- **Watches**: `Cargo.toml`, `Cargo.lock`

### go

Downloads Go module dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `go.mod`
- **Command**: `go mod download`
- **Completion check**: `go mod verify`
- **Watches**: `go.mod`, `go.sum`

### swift

Resolves Swift Package Manager dependencies.

- **Platforms**: macOS, Linux
- **Detects**: `Package.swift`
- **Command**: `swift package resolve`
- **Completion check**: `.build` directory exists
- **Watches**: `Package.swift`, `Package.resolved`

## Overriding Template Defaults

You can override any field from a template in your step config:

```yaml
steps:
  deps:
    template: bundler
    env:
      BUNDLE_WITHOUT: "production"
    command: "bundle install --jobs=4"
```

The template provides the base configuration, and your overrides take precedence.
