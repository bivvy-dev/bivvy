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

When running interactively, after generating the config Bivvy presents a
"Run setup now?" picker with two options — `No (n)` (default) and
`Yes (y)`. Choosing `Yes` chains directly into `bivvy run` with every
generated step forced. Choosing `No` (or pressing Enter) shows a hint to
run `bivvy run` later.

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

Bivvy automatically detects technologies and maps them to built-in templates.
The table below mirrors the project-type detection in
`src/detection/project.rs` and reflects every marker file the detector
inspects:

| Category | Detected via | Template |
|----------|-------------|----------|
| Ruby | `Gemfile` | `bundle-install` |
| Ruby (Rails) | `config/routes.rb`, `config/application.rb` | `rails-db` |
| Node.js | `package.json` (template chosen by lockfile) | `yarn-install` (yarn.lock), `pnpm-install` (pnpm-lock.yaml), `bun-install` (bun.lockb), `npm-install` (default) |
| Node.js (Next.js) | `next.config.js`, `next.config.mjs`, `next.config.ts` | `nextjs-build` |
| Node.js (Vite) | `vite.config.js`, `vite.config.ts`, `vite.config.mjs` | `vite-build` |
| Node.js (Remix) | `remix.config.js`, `remix.config.ts`, `app/root.tsx` | `remix-build` |
| Python | `pyproject.toml`, `requirements.txt`, `setup.py` | `poetry-install` (poetry.lock), `uv-sync` (uv.lock), `pip-install` (default) |
| Python (Alembic) | `alembic.ini`, `alembic/env.py` | `alembic-migrate` |
| Python (Django) | `manage.py` | `django-migrate` |
| Rust | `Cargo.toml` | `cargo-build` |
| Rust (Diesel) | `diesel.toml` | `diesel-migrate` |
| Go | `go.mod` | `go-mod-download` |
| PHP | `composer.json` | `composer-install` |
| PHP (Laravel) | `artisan` | `laravel-setup` |
| Kotlin / JVM (Gradle) | `build.gradle`, `build.gradle.kts` | `gradle-deps` |
| Spring Boot | `src/main/resources/application.properties`, `src/main/resources/application.yml` | `spring-boot-build` |
| Elixir | `mix.exs` | `mix-deps-get` |
| Swift | `Package.swift` | `swift-resolve` |
| Terraform | `main.tf`, `terraform.tf`, `versions.tf` | `terraform-init` |
| AWS CDK | `cdk.json` | `cdk-synth` |
| Java (Maven) | `pom.xml` | `maven-resolve` |
| .NET | `*.sln`, `*.csproj` | `dotnet-restore` |
| Dart | `pubspec.yaml` (no platform dirs) | `dart-pub-get` |
| Flutter | `pubspec.yaml` + `android/`, `ios/`, `web/`, `macos/`, `linux/`, or `windows/` directory | `flutter-pub-get` |
| Deno | `deno.json`, `deno.jsonc`, `deno.lock` | `deno-install` |
| Database (Prisma) | `prisma/schema.prisma` | `prisma-migrate` |
| Containers | `docker-compose.yml`, `docker-compose.yaml`, `compose.yml`, `compose.yaml` | `docker-compose-up` |
| Kubernetes (Helm) | `Chart.yaml` | `helm-deps` |
| IaC (Pulumi) | `Pulumi.yaml` | `pulumi-install` |
| IaC (Ansible) | `ansible.cfg`, `playbook.yml`, `playbook.yaml`, `site.yml`, `site.yaml` | `ansible-install` |
| Cross-cutting | `.env.example`, `.env.sample`, `.env.template` (and no `.env`) | `env-copy` |
| Cross-cutting | `.pre-commit-config.yaml` | `pre-commit-install` |
| Monorepo | `nx.json` | `nx-build` |
| Monorepo | `turbo.json` | `turbo-build` |
| Monorepo | `lerna.json` | `lerna-bootstrap` |

Version-manager detection is layered separately by the package-manager
detector and resolves system-wide to one of: `mise-tools`, `asdf-tools`,
`volta-setup`, `fnm-setup`, `nvm-node`, `rbenv-ruby`, `pyenv-python`.

## Enriched Output

The generated config includes commented-out template details so you can see what Bivvy will do and how to customize it. The first line is a `yaml-language-server` directive pointing at the schema Bivvy installs locally, so editors with the YAML language server (VS Code, Neovim) get completion and validation out of the box:

```yaml
# yaml-language-server: $schema=/Users/you/.bivvy/schema.json
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
