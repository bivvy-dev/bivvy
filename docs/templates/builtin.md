# Built-in Templates

Bivvy includes 45+ built-in templates for common development tools. When you run `bivvy init`, these are auto-detected based on files in your project.

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
| `nvm` | macOS, Linux | `.nvmrc` | `nvm install` |
| `fnm` | macOS, Linux, Windows | `.nvmrc`, `.node-version` | `fnm install` |
| `rbenv` | macOS, Linux | `.ruby-version` | `rbenv install` |
| `pyenv` | macOS, Linux | `.python-version` | `pyenv install` |

## Ruby

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `bundler` | macOS, Linux, Windows | `Gemfile` | `bundle install` |
| `rails-db` | macOS, Linux, Windows | `bin/rails`, `config/routes.rb` | `bin/rails db:prepare` |

## Node.js

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `yarn` | macOS, Linux, Windows | `yarn.lock` | `yarn install` |
| `npm` | macOS, Linux, Windows | `package-lock.json` | `npm install` |
| `pnpm` | macOS, Linux, Windows | `pnpm-lock.yaml` | `pnpm install` |
| `bun` | macOS, Linux, Windows | `bun.lockb` | `bun install` |
| `next` | macOS, Linux, Windows | `next.config.js`, `next.config.mjs` | `npm run dev` |
| `vite` | macOS, Linux, Windows | `vite.config.ts`, `vite.config.js` | `npm run dev` |
| `remix` | macOS, Linux, Windows | `remix.config.js` | `npm run dev` |

## Python

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `pip` | macOS, Linux, Windows | `requirements.txt` | `pip install -r requirements.txt` |
| `poetry` | macOS, Linux, Windows | `poetry.lock` | `poetry install` |
| `uv` | macOS, Linux, Windows | `uv.lock` | `uv sync` |
| `django` | macOS, Linux, Windows | `manage.py` | `python manage.py migrate` |
| `alembic` | macOS, Linux, Windows | `alembic.ini` | `alembic upgrade head` |

## Rust

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `cargo` | macOS, Linux, Windows | `Cargo.toml` | `cargo build` |
| `version-bump` | macOS, Linux, Windows | — | `cargo set-version` |

### version-bump

Bumps the version in `Cargo.toml` and updates `Cargo.lock`. Requires [cargo-edit](https://github.com/killercup/cargo-edit) (`cargo install cargo-edit`).

**Input:** `bump` (required) — `patch`, `minor`, `major`, or an explicit semver string like `1.5.0`.

Three ways to provide the `bump` value:

1. **Interactive prompt** — add a `prompts:` section to your step config
2. **Template input** — `inputs: { bump: "minor" }` in your step config
3. **Environment variable** — `BUMP=minor bivvy run --workflow release`

```yaml
steps:
  version-bump:
    template: version-bump
    prompts:
      - key: bump
        question: "Version bump type"
        type: select
        options:
          - label: "Patch (x.y.Z)"
            value: patch
          - label: "Minor (x.Y.0)"
            value: minor
          - label: "Major (X.0.0)"
            value: major
```

## Java

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `maven` | macOS, Linux, Windows | `pom.xml` | `mvn install` |
| `spring-boot` | macOS, Linux, Windows | `application.properties`, `application.yml` | `./gradlew bootRun` |

## .NET

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `dotnet` | macOS, Linux, Windows | `*.sln`, `*.csproj` | `dotnet restore` |

## Dart / Flutter

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `dart` | macOS, Linux, Windows | `pubspec.yaml` | `dart pub get` |
| `flutter` | macOS, Linux, Windows | `pubspec.yaml` (with Flutter SDK) | `flutter pub get` |

## Deno

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `deno` | macOS, Linux, Windows | `deno.json`, `deno.jsonc` | `deno cache` |

## Go

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `go` | macOS, Linux, Windows | `go.mod` | `go mod download` |

## Swift

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `swift` | macOS, Linux | `Package.swift` | `swift package resolve` |

## Database Migrations

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `rails-db` | macOS, Linux, Windows | `bin/rails`, `config/routes.rb` | `bin/rails db:prepare` |
| `prisma` | macOS, Linux, Windows | `prisma/schema.prisma` | `npx prisma migrate dev` |
| `diesel` | macOS, Linux, Windows | `diesel.toml` | `diesel migration run` |
| `alembic` | macOS, Linux, Windows | `alembic.ini` | `alembic upgrade head` |

## Containers & Orchestration

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `docker-compose` | macOS, Linux, Windows | `compose.yml`, `docker-compose.yml` | `docker compose up -d` |
| `helm` | macOS, Linux, Windows | `Chart.yaml` | `helm dependency build` |

## Infrastructure as Code

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `pulumi` | macOS, Linux, Windows | `Pulumi.yaml` | `pulumi up` |
| `ansible` | macOS, Linux | `ansible.cfg`, `playbook.yml` | `ansible-playbook playbook.yml` |

## Cross-cutting Tools

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `env-copy` | macOS, Linux, Windows | `.env.example` | `cp .env.example .env` |
| `pre-commit` | macOS, Linux, Windows | `.pre-commit-config.yaml` | `pre-commit install` |

## Monorepo / Workspace

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `nx` | macOS, Linux, Windows | `nx.json` | `npx nx run-many --target=build` |
| `turborepo` | macOS, Linux, Windows | `turbo.json` | `npx turbo run build` |
| `lerna` | macOS, Linux, Windows | `lerna.json` | `npx lerna bootstrap` |

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

### nvm

Installs Node.js version using nvm.

- **Platforms**: macOS, Linux
- **Detects**: `.nvmrc`
- **Command**: `nvm install`
- **Completion check**: `nvm current`
- **Watches**: `.nvmrc`

### fnm

Installs Node.js version using fnm (Fast Node Manager).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.nvmrc`, `.node-version`
- **Command**: `fnm install`
- **Completion check**: `fnm current`
- **Watches**: `.nvmrc`, `.node-version`

### rbenv

Installs Ruby version using rbenv.

- **Platforms**: macOS, Linux
- **Detects**: `.ruby-version`
- **Command**: `rbenv install`
- **Completion check**: `rbenv version`
- **Watches**: `.ruby-version`

### pyenv

Installs Python version using pyenv.

- **Platforms**: macOS, Linux
- **Detects**: `.python-version`
- **Command**: `pyenv install`
- **Completion check**: `pyenv version`
- **Watches**: `.python-version`

### maven

Installs Java dependencies using Maven.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pom.xml`
- **Command**: `mvn install`
- **Completion check**: `target` directory exists
- **Watches**: `pom.xml`

### spring-boot

Sets up a Spring Boot project (Gradle-based).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `application.properties`, `application.yml`
- **Command**: `./gradlew bootRun`
- **Completion check**: `build` directory exists
- **Watches**: `build.gradle`, `build.gradle.kts`, `application.properties`, `application.yml`

### dotnet

Restores .NET project dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `*.sln`, `*.csproj`
- **Command**: `dotnet restore`
- **Completion check**: `dotnet build --no-restore` succeeds
- **Watches**: `*.sln`, `*.csproj`, `Directory.Build.props`

### dart

Installs Dart package dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pubspec.yaml`
- **Command**: `dart pub get`
- **Completion check**: `.dart_tool` directory exists
- **Watches**: `pubspec.yaml`, `pubspec.lock`

### flutter

Installs Flutter package dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pubspec.yaml` (with Flutter SDK dependency)
- **Command**: `flutter pub get`
- **Completion check**: `.dart_tool` directory exists
- **Watches**: `pubspec.yaml`, `pubspec.lock`

### deno

Caches Deno module dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `deno.json`, `deno.jsonc`
- **Command**: `deno cache`
- **Completion check**: `deno info` succeeds
- **Watches**: `deno.json`, `deno.jsonc`, `deno.lock`

### next

Detects a Next.js project. Uses the project's Node.js package manager for dependency installation.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `next.config.js`, `next.config.mjs`
- **Command**: `npm run dev`
- **Completion check**: `.next` directory exists
- **Watches**: `next.config.js`, `next.config.mjs`, `package.json`

### vite

Detects a Vite project. Uses the project's Node.js package manager for dependency installation.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `vite.config.ts`, `vite.config.js`
- **Command**: `npm run dev`
- **Completion check**: `node_modules` directory exists
- **Watches**: `vite.config.ts`, `vite.config.js`, `package.json`

### remix

Detects a Remix project. Uses the project's Node.js package manager for dependency installation.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `remix.config.js`
- **Command**: `npm run dev`
- **Completion check**: `node_modules` directory exists
- **Watches**: `remix.config.js`, `package.json`

### django

Runs Django database migrations.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `manage.py`
- **Command**: `python manage.py migrate`
- **Completion check**: `python manage.py showmigrations --plan` shows no unapplied migrations
- **Watches**: `manage.py`, `*/migrations/*.py`

### rails-db

Prepares the Rails database (creates, migrates, seeds).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `bin/rails`, `config/routes.rb`
- **Command**: `bin/rails db:prepare`
- **Completion check**: `bin/rails db:version` succeeds
- **Watches**: `db/migrate/*`, `db/schema.rb`, `db/seeds.rb`

### prisma

Runs Prisma database migrations.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `prisma/schema.prisma`
- **Command**: `npx prisma migrate dev`
- **Completion check**: `npx prisma migrate status` shows no pending migrations
- **Watches**: `prisma/schema.prisma`, `prisma/migrations/*`

### diesel

Runs Diesel database migrations (Rust).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `diesel.toml`
- **Command**: `diesel migration run`
- **Completion check**: `diesel migration pending` returns empty
- **Watches**: `diesel.toml`, `migrations/*`

### alembic

Runs Alembic database migrations (Python/SQLAlchemy).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `alembic.ini`
- **Command**: `alembic upgrade head`
- **Completion check**: `alembic current` matches head
- **Watches**: `alembic.ini`, `alembic/versions/*`

### docker-compose

Starts services defined in a Docker Compose file.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `compose.yml`, `docker-compose.yml`
- **Command**: `docker compose up -d`
- **Completion check**: `docker compose ps` shows running services
- **Watches**: `compose.yml`, `docker-compose.yml`, `Dockerfile`

### helm

Builds Helm chart dependencies for Kubernetes deployments.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Chart.yaml`
- **Command**: `helm dependency build`
- **Completion check**: `charts/` directory exists
- **Watches**: `Chart.yaml`, `Chart.lock`, `values.yaml`

### pulumi

Deploys infrastructure using Pulumi.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Pulumi.yaml`
- **Command**: `pulumi up`
- **Completion check**: `pulumi stack` succeeds
- **Watches**: `Pulumi.yaml`, `Pulumi.*.yaml`

### ansible

Runs an Ansible playbook.

- **Platforms**: macOS, Linux
- **Detects**: `ansible.cfg`, `playbook.yml`
- **Command**: `ansible-playbook playbook.yml`
- **Completion check**: Command succeeds
- **Watches**: `ansible.cfg`, `playbook.yml`, `inventory/*`, `roles/*`

### env-copy

Copies `.env.example` to `.env` if it doesn't already exist.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.env.example`
- **Command**: `cp .env.example .env`
- **Completion check**: `.env` file exists
- **Watches**: `.env.example`

### pre-commit

Installs pre-commit hook scripts into your Git repository.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.pre-commit-config.yaml`
- **Command**: `pre-commit install`
- **Completion check**: `.git/hooks/pre-commit` file exists
- **Watches**: `.pre-commit-config.yaml`

### nx

Sets up an Nx monorepo workspace.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `nx.json`
- **Command**: `npx nx run-many --target=build`
- **Completion check**: `node_modules` directory exists
- **Watches**: `nx.json`, `workspace.json`, `package.json`

### turborepo

Sets up a Turborepo monorepo workspace.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `turbo.json`
- **Command**: `npx turbo run build`
- **Completion check**: `node_modules` directory exists
- **Watches**: `turbo.json`, `package.json`

### lerna

Bootstraps a Lerna monorepo workspace.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `lerna.json`
- **Command**: `npx lerna bootstrap`
- **Completion check**: `node_modules` directory exists
- **Watches**: `lerna.json`, `package.json`

## Install Templates

Install templates are used by the requirements system to install missing
tools. They are triggered automatically when a requirement gap is detected
and the user accepts the install prompt.

| Template | Description | Platforms |
|----------|-------------|-----------|
| `brew-install` | Install Homebrew package manager | macOS, Linux |
| `docker-install` | Install Docker Desktop or Docker Engine | macOS, Linux, Windows |
| `mise-install` | Install mise version manager | macOS, Linux |
| `mise-node` | Install Node.js using mise | macOS, Linux |
| `mise-python` | Install Python using mise | macOS, Linux |
| `mise-ruby` | Install Ruby using mise | macOS, Linux |
| `postgres-install` | Install PostgreSQL server and client tools | macOS, Linux |
| `redis-install` | Install Redis in-memory data store | macOS, Linux |
| `rust-install` | Install Rust toolchain via rustup | macOS, Linux, Windows |

### brew-install

Installs Homebrew package manager.

- **Platforms**: macOS, Linux
- **Command**: `/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"`
- **Completion check**: `brew --version`

### docker-install

Installs Docker Desktop (macOS) or Docker Engine (Linux).

- **Platforms**: macOS, Linux, Windows
- **Command**: Platform-specific (manual on macOS, `curl -fsSL https://get.docker.com | sh` on Linux)
- **Completion check**: `docker info`

### mise-install

Installs the mise version manager.

- **Platforms**: macOS, Linux
- **Command**: `curl https://mise.run | sh`
- **Completion check**: `mise --version`

### mise-node

Installs Node.js using mise. Requires `mise` to be installed first.

- **Platforms**: macOS, Linux
- **Requires**: `mise`
- **Command**: `mise install node`
- **Completion check**: `mise where node`

### mise-python

Installs Python using mise. Requires `mise` to be installed first.

- **Platforms**: macOS, Linux
- **Requires**: `mise`
- **Command**: `mise install python`
- **Completion check**: `mise where python`

### mise-ruby

Installs Ruby using mise. Requires `mise` to be installed first.

- **Platforms**: macOS, Linux
- **Requires**: `mise`
- **Command**: `mise install ruby`
- **Completion check**: `mise where ruby`

### postgres-install

Installs PostgreSQL database server and client tools.

- **Platforms**: macOS, Linux
- **Command**: Platform-specific (Homebrew on macOS, apt-get on Debian/Ubuntu)
- **Completion check**: `pg_isready -q`

### redis-install

Installs Redis in-memory data store.

- **Platforms**: macOS, Linux
- **Command**: Platform-specific (Homebrew on macOS, apt-get on Debian/Ubuntu)
- **Completion check**: `redis-cli ping`

### rust-install

Installs the Rust toolchain via rustup.

- **Platforms**: macOS, Linux, Windows
- **Command**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`
- **Completion check**: `rustc --version`

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
