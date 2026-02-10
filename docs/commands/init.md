---
title: bivvy init
description: Initialize configuration
---

# bivvy init

Initialize Bivvy configuration for your project.

## Usage

```bash
bivvy init
```

```bash
bivvy init --minimal
```

```bash
bivvy init --template=rails
```

```bash
bivvy init --from=../other
```

## Options

| Option | Description |
|--------|-------------|
| `--minimal` | Generate config without prompts |
| `--template` | Start from a specific template |
| `--from` | Copy configuration from another project |
| `--force` | Overwrite existing configuration |

## What It Does

1. Scans your project for technologies
2. Detects package managers and version managers
3. Identifies potential conflicts
4. Generates `.bivvy/config.yml`
5. Updates `.gitignore` for local overrides

## Examples

Interactive setup:

```bash
bivvy init
```

Quick setup for CI:

```bash
bivvy init --minimal
```

Force overwrite existing config:

```bash
bivvy init --force
```

## Detection

Bivvy automatically detects technologies and maps them to built-in templates:

| Category | Detected via | Template |
|----------|-------------|----------|
| System | Brewfile | `brew` |
| Ruby | Gemfile | `bundler` |
| Node.js | yarn.lock, package-lock.json, pnpm-lock.yaml, bun.lockb | `yarn`, `npm`, `pnpm`, `bun` |
| Python | requirements.txt, poetry.lock, uv.lock | `pip`, `poetry`, `uv` |
| Rust | Cargo.toml | `cargo` |
| Go | go.mod | `go` |
| Swift | Package.swift | `swift` |
| Version managers | .mise.toml, .tool-versions, volta | `mise`, `asdf`, `volta` |

## Enriched Output

The generated config includes commented-out template details so you can see what Bivvy will do and how to customize it:

```yaml
# Bivvy configuration for my-app
# Docs: https://bivvy.dev/configuration
app_name: "my-app"

settings:
  default_output: verbose  # verbose | quiet | silent

steps:
  bundler:
    template: bundler
    # command: bundle install
    # completed_check:
    #   type: command_succeeds
    #   command: "bundle check"
    # watches: [Gemfile, Gemfile.lock]

  yarn:
    template: yarn
    # command: yarn install
    # completed_check:
    #   type: command_succeeds
    #   command: "yarn check --verify-tree"
    # watches: [yarn.lock, package.json]

workflows:
  default:
    steps: [bundler, yarn]

# --- Customize further ---
# Override any template field per-step:
#   steps:
#     example:
#       template: bundler
#       env:
#         BUNDLE_WITHOUT: "production"
```

## Conflicts

When conflicts are detected (e.g., multiple lock files), Bivvy will:
1. Show a warning about the conflict
2. Suggest a resolution
3. Allow you to choose which to include
