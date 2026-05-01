---
title: Requirements
description: Declaring and detecting system-level prerequisites
---

# Requirements

Requirements declare the system-level tools a step needs before it can
run — things like Ruby, Node.js, PostgreSQL, or Docker. Bivvy checks
whether each requirement is satisfied before executing the step and offers
to install missing ones.

## Declaring requirements

Add a `tools:` list to any step. The canonical field name is `tools`;
the legacy name `requires` is still accepted as an alias.

```yaml
steps:
  bundle_install:
    command: bundle install
    tools:
      - ruby

  database_setup:
    command: rails db:setup
    tools:
      - ruby
      - postgres-server
```

## Built-in requirements

Bivvy ships with 33 built-in requirement definitions covering language
runtimes, version managers, services, and common CLI tools.

### Language runtimes

These requirements are version-manager-aware: Bivvy checks for managed
installs (mise, rbenv, nvm, pyenv, asdf, volta, fnm) before falling
back to system paths.

| Name | Check | Install template |
|------|-------|------------------|
| `ruby` | Ruby available via version manager or system path | `mise-ruby` |
| `node` | Node.js available via version manager or system path | `mise-node` |
| `python` | Python (3 or 2) available via version manager or system path | `mise-python` |
| `rust` | `rustc --version` succeeds | `rust-install` |
| `go` | `go version` succeeds | — (hint only) |
| `java` | `java -version` succeeds | — (hint only) |
| `elixir` | `elixir --version` succeeds | — (hint only) |
| `swift` | `swift --version` succeeds | — (hint only) |
| `dart` | `dart --version` succeeds | — (hint only) |
| `flutter` | `flutter --version` succeeds | — (hint only) |
| `dotnet` | `dotnet --version` succeeds | — (hint only) |
| `deno` | `deno --version` succeeds | — (hint only) |
| `php` | `php --version` succeeds | — (hint only) |

### Database client tools

| Name | Check | Install template |
|------|-------|------------------|
| `postgres` | `psql --version` succeeds | `postgres-install` |

### Services

| Name | Check | Install template |
|------|-------|------------------|
| `postgres-server` | `pg_isready -q` succeeds | `postgres-install` |
| `redis-server` | `redis-cli ping` succeeds | `redis-install` |

### Container tools

| Name | Check | Install template |
|------|-------|------------------|
| `docker` | `docker info` succeeds | `docker-install` |

### Build / package tools

| Name | Check | Install template |
|------|-------|------------------|
| `bundler` | `bundle --version` succeeds | — (depends on `ruby`) |
| `mvn` | `mvn --version` succeeds | — (depends on `java`) |
| `terraform` | `terraform version` succeeds | — (hint only) |
| `helm` | `helm version` succeeds | — (hint only) |
| `ansible` | `ansible --version` succeeds | — (hint only) |
| `pulumi` | `pulumi version` succeeds | — (hint only) |
| `pre-commit` | `pre-commit --version` succeeds | — (hint only) |
| `diesel` | `diesel --version` succeeds | — (depends on `rust`) |

### Package and version managers

| Name | Check | Install template |
|------|-------|------------------|
| `brew` | `brew --version` succeeds | `brew-install` |
| `mise` | `mise --version` succeeds | `mise-install` |
| `asdf` | `asdf --version` succeeds | — (hint only) |
| `rbenv` | `rbenv --version` succeeds | — (hint only) |
| `nvm` | `nvm --version` succeeds | — (hint only) |
| `fnm` | `fnm --version` succeeds | — (hint only) |
| `pyenv` | `pyenv --version` succeeds | — (hint only) |
| `volta` | `volta --version` succeeds | — (hint only) |

Requirements without an install template (`— (hint only)`) cannot be
auto-installed by Bivvy. Bivvy will print the install hint and
prompt the user to install manually before continuing.

## What happens when a requirement is missing

Bivvy categorizes each requirement into one of these statuses:

| Status | Meaning | Interactive behavior |
|--------|---------|----------------------|
| **Satisfied** | Tool is available and ready | No action needed |
| **Inactive** | Installed via version manager but not activated | Prompt to activate |
| **SystemOnly** | Available at system path but not via version manager | Warn, offer managed install |
| **ServiceDown** | Binary present but service not running | Prompt to start |
| **Missing** | Not installed at all | Prompt to install |

In interactive mode, Bivvy offers to fix each gap before running the
step. For example:

```
ruby is not installed
  Install Ruby using mise? [Y/n]
```

### Non-interactive behavior

In `--non-interactive` mode:

- **Missing** requirements block execution (the step fails)
- **SystemOnly** requirements produce a warning but allow the step to run
- **ServiceDown** requirements block execution
- **Inactive** requirements block execution

Use `provided_requirements` in environment config to skip checks entirely
in non-interactive environments like CI.

## Custom requirements

Define project-specific requirements at the config root:

```yaml
requirements:
  elasticsearch:
    check:
      type: service_reachable
      command: "curl -sf http://localhost:9200/_cluster/health"
    install_hint: "Install Elasticsearch: https://elastic.co/downloads"

  graphviz:
    check:
      type: command_succeeds
      command: "dot -V"
    install_template: brew-install
    install_hint: "brew install graphviz"
```

### Check types

| Type | Fields | Description |
|------|--------|-------------|
| `command_succeeds` | `command` | Runs a shell command, passes on exit code 0 |
| `file_exists` | `path` | Checks if a file or directory exists |
| `service_reachable` | `command` | Runs a command that probes a service |

### Custom requirement fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `check` | object | yes | How to verify the requirement is satisfied |
| `install_template` | string | no | Template to use for installation |
| `install_hint` | string | no | Human-readable install instructions |

## Provided requirements

Environments can mark requirements as already satisfied, skipping all
checks and install prompts:

```yaml
settings:
  environments:
    ci:
      provided_requirements:
        - docker
        - postgres-server
        - redis-server
    docker:
      provided_requirements:
        - docker
```

This is especially useful for CI pipelines and containers where tools are
pre-installed or managed externally. See
[Environments](environments.md) for more.
