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
| Ruby (Rails) | bin/rails, config/routes.rb | `rails-db` |
| Node.js | yarn.lock, package-lock.json, pnpm-lock.yaml, bun.lockb | `yarn`, `npm`, `pnpm`, `bun` |
| Node.js (Next.js) | next.config.js, next.config.mjs | `next` |
| Node.js (Vite) | vite.config.ts, vite.config.js | `vite` |
| Node.js (Remix) | remix.config.js | `remix` |
| Python | requirements.txt, poetry.lock, uv.lock | `pip`, `poetry`, `uv` |
| Python (Django) | manage.py | `django` |
| Python (Alembic) | alembic.ini | `alembic` |
| Rust | Cargo.toml | `cargo` |
| Rust (Diesel) | diesel.toml | `diesel` |
| Go | go.mod | `go` |
| Swift | Package.swift | `swift` |
| Java (Maven) | pom.xml | `maven` |
| Java (Spring Boot) | application.properties, application.yml | `spring-boot` |
| .NET | *.sln, *.csproj | `dotnet` |
| Dart / Flutter | pubspec.yaml | `dart`, `flutter` |
| Deno | deno.json, deno.jsonc | `deno` |
| Database (Prisma) | prisma/schema.prisma | `prisma` |
| Containers | compose.yml, docker-compose.yml | `docker-compose` |
| Kubernetes | Chart.yaml | `helm` |
| IaC (Pulumi) | Pulumi.yaml | `pulumi` |
| IaC (Ansible) | ansible.cfg, playbook.yml | `ansible` |
| Cross-cutting | .env.example | `env-copy` |
| Cross-cutting | .pre-commit-config.yaml | `pre-commit` |
| Monorepo | nx.json | `nx` |
| Monorepo | turbo.json | `turborepo` |
| Monorepo | lerna.json | `lerna` |
| Version managers | .mise.toml, .tool-versions, volta | `mise`, `asdf`, `volta` |
| Version managers | .nvmrc, .node-version | `nvm`, `fnm` |
| Version managers | .ruby-version | `rbenv` |
| Version managers | .python-version | `pyenv` |

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
