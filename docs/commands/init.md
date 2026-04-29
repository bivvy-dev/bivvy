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
bivvy init --template=ruby
```

```bash
bivvy init --from=../other-project
```

## Options

| Option | Description |
|--------|-------------|
| `--minimal` | Generate config without prompts, using only auto-detected templates |
| `--template <name>` | Start from a specific template or category (skips auto-detection). Accepts a template name (e.g., `bundle-install`) or a category (e.g., `ruby`) to include all templates in that category. |
| `--from <path>` | Copy `.bivvy/config.yml` from another project directory into the current project |
| `--force` | Overwrite existing configuration |

## What It Does

1. Scans your project for technologies
2. Detects package managers and version managers
3. Identifies potential conflicts
4. Generates `.bivvy/config.yml`
5. Updates `.gitignore` for local overrides
6. Offers to run setup immediately (interactive mode only)

When running interactively, after generating the config Bivvy prompts "Run setup now?" with options to run immediately or exit. Choosing "Yes" chains directly into `bivvy run`. Choosing "No" (the default) shows a hint to run `bivvy run` later.

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

Start from the Ruby template:

```bash
bivvy init --template=bundle-install
```

Include all Ruby-category templates:

```bash
bivvy init --template=ruby
```

Clone config from a sibling project:

```bash
bivvy init --from=../other-project
```

## Detection

Bivvy automatically detects technologies and maps them to built-in templates:

| Category | Detected via | Template |
|----------|-------------|----------|
| System | Brewfile | `brew-bundle` |
| Ruby | Gemfile | `bundle-install` |
| Ruby (Rails) | bin/rails, config/routes.rb | `rails-db` |
| Node.js | yarn.lock, package-lock.json, pnpm-lock.yaml, bun.lockb | `yarn-install`, `npm-install`, `pnpm-install`, `bun-install` |
| Node.js (Next.js) | next.config.js, next.config.mjs | `nextjs-build` |
| Node.js (Vite) | vite.config.ts, vite.config.js | `vite-build` |
| Node.js (Remix) | remix.config.js | `remix-build` |
| Python | requirements.txt, poetry.lock, uv.lock | `pip-install`, `poetry-install`, `uv-sync` |
| Python (Django) | manage.py | `django-migrate` |
| Python (Alembic) | alembic.ini | `alembic-migrate` |
| Rust | Cargo.toml | `cargo-build` |
| Rust (Diesel) | diesel.toml | `diesel-migrate` |
| Go | go.mod | `go-mod-download` |
| Swift | Package.swift | `swift-resolve` |
| Java (Maven) | pom.xml | `maven-resolve` |
| Java (Spring Boot) | application.properties, application.yml | `spring-boot-build` |
| .NET | *.sln, *.csproj | `dotnet-restore` |
| Dart / Flutter | pubspec.yaml | `dart-pub-get`, `flutter-pub-get` |
| Deno | deno.json, deno.jsonc | `deno-install` |
| Database (Prisma) | prisma/schema.prisma | `prisma-migrate` |
| Containers | compose.yml, docker-compose.yml | `docker-compose-up` |
| Kubernetes | Chart.yaml | `helm-deps` |
| IaC (Pulumi) | Pulumi.yaml | `pulumi-install` |
| IaC (Ansible) | ansible.cfg, playbook.yml | `ansible-install` |
| Cross-cutting | .env.example | `env-copy` |
| Cross-cutting | .pre-commit-config.yaml | `pre-commit-install` |
| Monorepo | nx.json | `nx-build` |
| Monorepo | turbo.json | `turbo-build` |
| Monorepo | lerna.json | `lerna-bootstrap` |
| Version managers | .mise.toml, .tool-versions, volta | `mise-tools`, `asdf-tools`, `volta-setup` |
| Version managers | .nvmrc, .node-version | `nvm-node`, `fnm-node` |
| Version managers | .ruby-version | `rbenv-ruby` |
| Version managers | .python-version | `pyenv-python` |

## Enriched Output

The generated config includes commented-out template details so you can see what Bivvy will do and how to customize it:

```yaml
# Bivvy configuration for my-app
# Docs: https://bivvy.dev/configuration
#
# Override any template field per-step:
#   steps:
#     example:
#       template: bundle-install
#       env:
#         BUNDLE_WITHOUT: "production"
#
# Add custom steps:
#   steps:
#     setup_db:
#       title: "Set up database"
#       command: "bin/rails db:setup"
#       check:
#         type: execution
#         command: "bin/rails db:version"
#         validation: success
#
# Create named workflows:
#   workflows:
#     ci:
#       steps: [bundle-install, yarn-install]
#       settings:
#         defaults:
#           output: quiet

app_name: "my-app"

settings:
  defaults:
    output: verbose  # verbose | quiet | silent

steps:
  bundle-install:
    template: bundle-install
    # command: bundle install
    # check:
    #   type: execution
    #   command: "bundle check"
    #   validation: success

  yarn-install:
    template: yarn-install
    # command: yarn install
    # check:
    #   type: execution
    #   command: "yarn check --verify-tree"
    #   validation: success

workflows:
  default:
    steps: [bundle-install, yarn-install]
```

## Conflicts

When conflicts are detected (e.g., multiple lock files), Bivvy will:
1. Show a warning about the conflict
2. Suggest a resolution
3. Allow you to choose which to include
