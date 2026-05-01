---
title: Artifact Audits
description: Catch source maps, secrets, and debug symbols before they ship
---

# Artifact Audits

Artifact audit templates scan your build output for files that should never
reach production: source maps, leaked secrets, debug symbols, `.env` files,
and more. They run as post-build safety gates and fail with a non-zero exit
code when issues are found.

## Why Artifact Audits?

Build tools sometimes produce artifacts you didn't ask for. A runtime update
enables source maps by default. A config change includes debug symbols in
your release binary. A `.env` file slips into a Docker image. These are the
kinds of silent drift that cause real incidents.

Artifact audits make these checks explicit, repeatable, and part of your
setup -- not something you remember to do manually.

## Available Audit Templates

| Template | Ecosystem | Key checks |
|----------|-----------|------------|
| `node-artifact-audit` | Node.js | Source maps, `.env` files, secrets in JS, `node_modules` in dist |
| `rust-artifact-audit` | Rust | Debug symbols (DWARF), `.pdb` files, debug profile config |
| `python-artifact-audit` | Python | Secrets in wheels/sdists, `.env`, `__pycache__`, test files |
| `go-artifact-audit` | Go | DWARF symbols, embedded local paths, embedded secrets |
| `java-artifact-audit` | Java/JVM | Source in JARs, debug compilation, hardcoded secrets in config |
| `dotnet-artifact-audit` | .NET | `.pdb` files, `appsettings.Development.json`, `web.config debug=true` |
| `docker-artifact-audit` | Docker | `.env`, `.git`, SSH keys, source maps in images |
| `ruby-artifact-audit` | Ruby | Credentials in gems, `master.key`, broad gemspec globs |
| `php-artifact-audit` | PHP | `APP_DEBUG=true`, dev deps in vendor, `phpinfo()` calls |
| `elixir-artifact-audit` | Elixir | Hardcoded `secret_key_base`, dev/test config in release |
| `swift-artifact-audit` | Swift | Debug symbols in release, dSYM bundles, embedded secrets |

## Quick Start

Add an audit step after your build step:

```yaml
steps:
  build:
    template: vite-build

  audit:
    template: node-artifact-audit
    depends_on: [build]
```

That's it. Bivvy will scan `dist/` for source maps, secrets, and other
leaks after every build.

## Audit Inputs

Each audit template takes a different input describing where to look. The
key name varies by ecosystem -- there is no universal `dist_dir` field.
Use the table below to pick the right input for the audit you're running.

| Template | Input key | Default | Notes |
|----------|-----------|---------|-------|
| `node-artifact-audit` | `dist_dir` | `dist` | Plus optional `fail_on_sourcemaps` and `fail_on_secrets` (both default `true`) |
| `python-artifact-audit` | `dist_dir` | `dist` | |
| `rust-artifact-audit` | `profile` | `release` | Cargo build profile; resolves to `target/<profile>` |
| `go-artifact-audit` | `binary_path` | (empty) | Path to the binary; empty means scan the current directory |
| `java-artifact-audit` | `build_dir` | `target` | Maven/Gradle build output |
| `dotnet-artifact-audit` | `publish_dir` | `bin/Release/net8.0/publish` | `dotnet publish` output |
| `swift-artifact-audit` | `build_config` | `release` | Resolves to `.build/<config>` |
| `php-artifact-audit` | `deploy_dir` | `.` | Defaults to current directory |
| `elixir-artifact-audit` | `release_dir` | `_build/prod/rel` | OTP release directory |
| `ruby-artifact-audit` | `pkg_dir` | `pkg` | Directory containing built `.gem` files |
| `docker-artifact-audit` | `image_name` | (required) | Image name or tag to inspect |

## Customizing the Audit

### Pointing at a non-default directory

Pass the input key that matches the audit you're using. Examples:

```yaml
steps:
  audit_node:
    template: node-artifact-audit
    inputs:
      dist_dir: build       # default is "dist"

  audit_rust:
    template: rust-artifact-audit
    inputs:
      profile: release      # resolves to target/release

  audit_java:
    template: java-artifact-audit
    inputs:
      build_dir: build/libs # Gradle default

  audit_docker:
    template: docker-artifact-audit
    inputs:
      image_name: myapp:latest
```

### Control failure behavior (Node only)

The Node audit lets you downgrade individual checks to warnings:

```yaml
steps:
  audit:
    template: node-artifact-audit
    inputs:
      dist_dir: dist
      fail_on_sourcemaps: false   # warn instead of fail
      fail_on_secrets: true       # still fail on secrets
```

Other audit templates do not currently expose `fail_on_*` toggles -- they
either pass or fail based on what they find.

### Add to a CI workflow

Audits pair well with CI workflows. Add them as a mandatory gate before
deployment:

```yaml
workflows:
  ci:
    steps: [deps, build, audit]

  deploy:
    steps: [deps, build, audit, publish]
```

If the audit step fails, subsequent steps won't run.

## Example Configurations

### JavaScript / TypeScript Application

```yaml
app_name: "my-vite-app"

steps:
  deps:
    template: npm-install

  build:
    template: vite-build
    depends_on: [deps]

  audit:
    template: node-artifact-audit
    depends_on: [build]
    inputs:
      dist_dir: dist

workflows:
  default:
    steps: [deps, build, audit]
```

### Rust Application

```yaml
app_name: "my-rust-app"

steps:
  build:
    template: cargo-build

  audit:
    template: rust-artifact-audit
    depends_on: [build]
    inputs:
      profile: release

workflows:
  default:
    steps: [build]

  release:
    steps: [build, audit]
```

### Ruby on Rails Application

```yaml
app_name: "my-rails-app"

steps:
  deps:
    template: bundle-install

  db:
    template: rails-db
    depends_on: [deps]

  audit:
    template: ruby-artifact-audit
    depends_on: [deps]
    inputs:
      pkg_dir: pkg

workflows:
  default:
    steps: [deps, db]

  release:
    steps: [deps, audit]
```

### Python Application

```yaml
app_name: "my-python-app"

steps:
  deps:
    template: poetry-install

  migrate:
    template: django-migrate
    depends_on: [deps]

  audit:
    template: python-artifact-audit
    depends_on: [deps]
    inputs:
      dist_dir: dist

workflows:
  default:
    steps: [deps, migrate]

  release:
    steps: [deps, audit]
```

### Docker Image

```yaml
app_name: "my-service"

steps:
  audit:
    template: docker-artifact-audit
    inputs:
      image_name: my-service:latest

workflows:
  release:
    steps: [audit]
```

## What Each Audit Checks

### Source maps and debug info

Source maps (`.js.map`, `.css.map`) expose your original source code.
Debug symbols (DWARF, `.pdb`) expose internal structure. Release builds
should strip both.

### Secrets and credentials

Scans for patterns like `_SECRET=`, `_TOKEN=`, `_PASSWORD=`, `api_key`,
and PEM private keys in build output files. Also catches `.env` files
and framework-specific credential files (`master.key`, `appsettings.Development.json`).

### Unexpected files

Catches directories and files that shouldn't ship: `.git/`, `node_modules/`,
test files, source code in compiled archives, and dev-only configuration.

### Binary analysis

For compiled languages (Rust, Go, Swift), audits use `file`, `strings`,
and toolchain-specific commands to check for debug symbols, embedded
filesystem paths, and hardcoded secrets in the binary itself.

## Next Steps

- [Built-in Templates](../templates/builtin.md) -- full template reference
- [CI Integration](ci-integration.md) -- run audits in your pipeline
- [Steps](../configuration/steps.md) -- step configuration reference
