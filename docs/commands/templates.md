---
title: bivvy templates
description: List available templates
---

# bivvy templates

Lists all available templates from built-in, local, and remote sources, organized by category.

## Usage

```bash
bivvy templates
```

```bash
bivvy templates --category ruby
```

## Options

| Option | Description |
|--------|-------------|
| `--category` | Filter templates by category (e.g., `ruby`, `node`, `python`) |

## What It Shows

Templates are grouped by category. Each template shows its name and a short description:

```
Available Templates

  system — System-level package managers
    brew  Install macOS dependencies from Brewfile
    apt   Install Debian/Ubuntu packages

  ruby — Ruby ecosystem tools
    bundler   Install Ruby dependencies using Bundler
    rails-db  Set up Rails database

  node — Node.js ecosystem tools
    yarn  Install Node.js dependencies using Yarn
    npm   Install Node.js dependencies using npm
    pnpm  Install Node.js dependencies using pnpm
    ...

  rust — Rust ecosystem tools
    cargo  Build Rust project with Cargo
    ...

  82 templates available. Use `bivvy add <template>` to add one.
```

## Template Sources

Templates are loaded from multiple sources in priority order:

1. **Project-local** (`.bivvy/templates/steps/`) — highest priority
2. **User-local** (`~/.bivvy/templates/steps/`)
3. **Remote** (configured via `template_sources` in config)
4. **Built-in** (bundled with Bivvy)

Local and remote templates that aren't in the built-in manifest appear under a **custom** category.

## Categories

Bivvy ships with templates organized in these categories:

| Category | Description |
|----------|-------------|
| `system` | System-level package managers (brew, apt, yum, pacman) |
| `windows` | Windows package managers (chocolatey, scoop) |
| `version_manager` | Version managers (mise, asdf, volta, fnm) |
| `ruby` | Ruby ecosystem (bundler, rails-db) |
| `node` | Node.js ecosystem (yarn, npm, pnpm, bun, and frameworks) |
| `python` | Python ecosystem (pip, poetry, uv, and frameworks) |
| `rust` | Rust ecosystem (cargo, diesel-migrate) |
| `go` | Go ecosystem |
| `php` | PHP ecosystem (composer, laravel) |
| `swift` | Swift ecosystem |
| `gradle` | Gradle/Spring Boot |
| `elixir` | Elixir ecosystem (mix) |
| `java` | Java ecosystem (maven) |
| `dotnet` | .NET ecosystem |
| `dart` | Dart and Flutter |
| `deno` | Deno runtime |
| `iac` | Infrastructure as Code (terraform, aws-cdk, pulumi, ansible) |
| `containers` | Container orchestration (docker-compose, helm) |
| `common` | Cross-cutting concerns (env-copy, pre-commit) |
| `monorepo` | Monorepo tools (nx, turborepo, lerna) |
| `install` | Runtime and tool installers |

## Examples

List all available templates:

```bash
bivvy templates
```

Show only Python templates:

```bash
bivvy templates --category python
```

## See Also

- [`bivvy add`](./add.md) — Add a template to your project
- [`bivvy init`](./init.md) — Initialize configuration with auto-detected templates
- [Built-in Templates](../templates/builtin.md) — Full reference of all built-in templates
