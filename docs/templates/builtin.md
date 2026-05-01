# Built-in Templates

Bivvy ships dozens of built-in templates for common development tools. When you run `bivvy init`, applicable templates are auto-detected based on files in your project, and you can also reference them by name in `.bivvy/config.yml`.

> Every template is referenced by its full name (`yarn-install`, `bundle-install`, `cargo-build`, …). There are no short-name aliases. When two categories define the same name (e.g. `version-bump`), prefix it with the category (`rust/version-bump`).

## System Package Managers

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `brew-bundle` | macOS, Linux | `Brewfile` | `brew bundle install` |
| `apt-install` | Linux | `apt-get` available | `sudo apt-get install -y` |
| `yum-install` | Linux | `yum` available | `sudo yum install -y` |
| `pacman-install` | Linux | `pacman` available | `sudo pacman -S --noconfirm` |

## Windows Package Managers

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `choco-install` | Windows | `choco` available | `choco install -y` |
| `scoop-install` | Windows | `scoop` available | `scoop install` |

## Version Managers

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `mise-tools` | macOS, Linux, Windows | `.mise.toml`, `mise.toml` | `mise install` |
| `asdf-tools` | macOS, Linux | `.tool-versions` | `asdf install` |
| `volta-setup` | macOS, Linux, Windows | `volta` available | `volta install node` |
| `fnm-setup` | macOS, Linux, Windows | `.nvmrc`, `.node-version` | `fnm install && fnm use` |
| `nvm-node` | macOS, Linux | `.nvmrc` | `nvm install` |
| `rbenv-ruby` | macOS, Linux | `.ruby-version` | `rbenv install --skip-existing` |
| `pyenv-python` | macOS, Linux | `.python-version` | `pyenv install --skip-existing` |

## Ruby

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `bundle-install` | macOS, Linux, Windows | `Gemfile` | `bundle install` |
| `rails-db` | macOS, Linux, Windows | `bin/rails`, `config/routes.rb` | `bundle exec rails db:prepare` |
| `ruby/version-bump` | macOS, Linux, Windows | — | `bump "${bump}" --no-commit --no-tag` |

## Node.js

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `yarn-install` | macOS, Linux, Windows | `yarn.lock` | `yarn install` |
| `npm-install` | macOS, Linux, Windows | `package-lock.json`, `package.json` | `npm install` |
| `pnpm-install` | macOS, Linux, Windows | `pnpm-lock.yaml` | `pnpm install` |
| `bun-install` | macOS, Linux, Windows | `bun.lockb` | `bun install` |
| `nextjs-build` | macOS, Linux, Windows | `next.config.js`, `next.config.mjs`, `next.config.ts` | `npx next build` |
| `vite-build` | macOS, Linux, Windows | `vite.config.ts`, `vite.config.js`, `vite.config.mjs` | `npx vite build` |
| `remix-build` | macOS, Linux, Windows | `remix.config.js`, `remix.config.ts` | `npx remix build` |
| `node/version-bump` | macOS, Linux, Windows | — | `npm version "${bump}" --no-git-tag-version` |

## Python

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `pip-install` | macOS, Linux, Windows | `requirements.txt`, `pyproject.toml` | `pip install -r requirements.txt` |
| `poetry-install` | macOS, Linux, Windows | `poetry.lock` | `poetry install` |
| `uv-sync` | macOS, Linux, Windows | `uv.lock` | `uv sync` |
| `django-migrate` | macOS, Linux, Windows | `manage.py` | `python manage.py migrate` |
| `alembic-migrate` | macOS, Linux, Windows | `alembic.ini` | `alembic upgrade head` |
| `python/version-bump` | macOS, Linux, Windows | — | `bump-my-version bump "${bump}" --no-commit --no-tag` |

## PHP

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `composer-install` | macOS, Linux, Windows | `composer.json` | `composer install` |
| `laravel-setup` | macOS, Linux, Windows | Laravel project files | `cp -n .env.example .env; php artisan key:generate --force` |
| `php/version-bump` | macOS, Linux, Windows | — | Updates `version` in `composer.json` |

## Rust

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `cargo-build` | macOS, Linux, Windows | `Cargo.toml` | `cargo build` |
| `diesel-migrate` | macOS, Linux, Windows | `diesel.toml` | `diesel setup && diesel migration run` |
| `rust/version-bump` | macOS, Linux, Windows | — | `cargo set-version` (requires [cargo-edit](https://github.com/killercup/cargo-edit)) |

## Go

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `go-mod-download` | macOS, Linux, Windows | `go.mod` | `go mod download` |
| `go/version-bump` | macOS, Linux, Windows | — | Computes the next semver Git tag (`vX.Y.Z`) |

## Swift

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `swift-resolve` | macOS, Linux | `Package.swift` | `swift package resolve` |
| `swift/version-bump` | macOS | — | `agvtool new-marketing-version …` |

## Java

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `maven-resolve` | macOS, Linux, Windows | `pom.xml` | `mvn dependency:resolve` |
| `java/version-bump` | macOS, Linux, Windows | — | `mvn versions:set -DnewVersion="${bump}" -DgenerateBackupPoms=false` |

## Kotlin / Gradle

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `gradle-deps` | macOS, Linux, Windows | `build.gradle`, `build.gradle.kts`, `settings.gradle*` | `./gradlew dependencies` |
| `spring-boot-build` | macOS, Linux, Windows | `application.properties`, `application.yml` | `./gradlew build -x test` |
| `kotlin/version-bump` | macOS, Linux, Windows | — | Updates `version` in `gradle.properties` or `build.gradle.kts` |

## Elixir

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `mix-deps-get` | macOS, Linux, Windows | `mix.exs` | `mix deps.get` |
| `elixir/version-bump` | macOS, Linux, Windows | — | Updates `version:` in `mix.exs` |

## .NET

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `dotnet-restore` | macOS, Linux, Windows | `*.sln`, `*.csproj` | `dotnet restore` |
| `dotnet/version-bump` | macOS, Linux, Windows | — | Updates `<Version>` in `*.csproj` |

## Dart / Flutter

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `dart-pub-get` | macOS, Linux, Windows | `pubspec.yaml` | `dart pub get` |
| `flutter-pub-get` | macOS, Linux, Windows | `pubspec.yaml` (with Flutter SDK) | `flutter pub get` |

## Deno

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `deno-install` | macOS, Linux, Windows | `deno.json`, `deno.jsonc` | `deno install` |

## Database Migrations

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `rails-db` | macOS, Linux, Windows | `bin/rails`, `config/routes.rb` | `bundle exec rails db:prepare` |
| `prisma-migrate` | macOS, Linux, Windows | `prisma/schema.prisma` | `npx prisma migrate dev` |
| `diesel-migrate` | macOS, Linux, Windows | `diesel.toml` | `diesel setup && diesel migration run` |
| `alembic-migrate` | macOS, Linux, Windows | `alembic.ini` | `alembic upgrade head` |
| `django-migrate` | macOS, Linux, Windows | `manage.py` | `python manage.py migrate` |

## Containers & Orchestration

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `docker-compose-up` | macOS, Linux, Windows | `compose.yml`, `docker-compose.yml` | `docker compose up -d` |
| `helm-deps` | macOS, Linux, Windows | `Chart.yaml` | `helm dependency update` |

## Infrastructure as Code

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `terraform-init` | macOS, Linux, Windows | `*.tf` | `terraform init` |
| `cdk-synth` | macOS, Linux, Windows | `cdk.json` | `cdk synth` |
| `pulumi-install` | macOS, Linux, Windows | `Pulumi.yaml` | `pulumi install` |
| `ansible-install` | macOS, Linux | `ansible.cfg`, `playbook.yml` | `ansible-galaxy install -r requirements.yml` |

## Artifact Audits

Post-build security audits that scan for source maps, secrets, debug symbols, and other files that should not ship to production. See the [Artifact Audits guide](../guides/artifact-audits.md) for usage examples.

| Template | Platforms | Detects | Checks |
|----------|-----------|---------|--------|
| `node-artifact-audit` | macOS, Linux, Windows | `package.json` | Source maps, `.env`, secrets in JS, `node_modules` in dist |
| `rust-artifact-audit` | macOS, Linux, Windows | `Cargo.toml` | Debug symbols, `.pdb` files, debug profile config |
| `python-artifact-audit` | macOS, Linux, Windows | `pyproject.toml` | Secrets in wheels/sdists, `.env`, `__pycache__`, test files |
| `go-artifact-audit` | macOS, Linux, Windows | `go.mod` | DWARF symbols, embedded paths, embedded secrets |
| `java-artifact-audit` | macOS, Linux, Windows | `pom.xml` | Source in JARs, debug info, hardcoded secrets |
| `dotnet-artifact-audit` | macOS, Linux, Windows | `*.sln`, `*.csproj` | `.pdb` files, Development config, `web.config debug=true` |
| `docker-artifact-audit` | macOS, Linux | `compose.yml` | `.env`, `.git`, SSH keys, source maps in images |
| `ruby-artifact-audit` | macOS, Linux, Windows | `Gemfile` | Credentials in gems, `master.key`, broad globs |
| `php-artifact-audit` | macOS, Linux, Windows | `composer.json` | `APP_DEBUG=true`, dev deps, `phpinfo()` |
| `elixir-artifact-audit` | macOS, Linux | `mix.exs` | Hardcoded secrets, dev/test config in release |
| `swift-artifact-audit` | macOS, Linux | `Package.swift` | Debug symbols, dSYM bundles, embedded secrets |

## Cross-cutting Tools

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `env-copy` | macOS, Linux, Windows | `.env.example`, `.env.sample`, `.env.template` | `cp -n .env.example .env` (and fallbacks) |
| `pre-commit-install` | macOS, Linux | `.pre-commit-config.yaml` | `pre-commit install` |

## Monorepo / Workspace

| Template | Platforms | Detects | Command |
|----------|-----------|---------|---------|
| `nx-build` | macOS, Linux, Windows | `nx.json` | `npx nx reset && npx nx run-many --target=build --all --skip-nx-cache` |
| `turbo-build` | macOS, Linux, Windows | `turbo.json` | `npx turbo build` |
| `lerna-bootstrap` | macOS, Linux, Windows | `lerna.json` | `npx lerna bootstrap` |

## Example Usage

```yaml
steps:
  deps:
    template: brew-bundle
  ruby:
    template: bundle-install
    depends_on: [deps]
  node:
    template: yarn-install
    depends_on: [deps]
```

## Template Details

Each template provides:

- **Command** — the shell command to run
- **Completed checks** — how Bivvy decides the step is already done (it skips when checks pass)
- **Watches** — files whose changes mark the step dirty and force a re-run
- **Environment impact** — `PATH` or shell config changes the step makes

### brew-bundle

Installs Homebrew packages from a Brewfile.

- **Platforms**: macOS, Linux
- **Detects**: `Brewfile`
- **Command**: `brew bundle install`
- **Completion check**: `brew bundle check`
- **Watches**: `Brewfile`, `Brewfile.lock.json`

### bundle-install

Installs Ruby gems from a Gemfile.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Gemfile`
- **Command**: `bundle install`
- **Completion check**: `bundle check`
- **Watches**: `Gemfile`, `Gemfile.lock`

### yarn-install

Installs Node.js dependencies using Yarn.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `yarn.lock`
- **Command**: `yarn install`
- **Completion check**: `yarn check --verify-tree`
- **Watches**: `yarn.lock`, `package.json`
- **Environment**: sets `NODE_ENV=development`

### npm-install

Installs Node.js dependencies using npm.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `package-lock.json`, `package.json`
- **Command**: `npm install`
- **Completion check**: `node_modules` directory exists
- **Watches**: `package.json`, `package-lock.json`
- **Environment**: sets `NODE_ENV=development`

### pnpm-install

Installs Node.js dependencies using pnpm.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pnpm-lock.yaml`
- **Command**: `pnpm install`
- **Completion check**: `node_modules` directory exists
- **Watches**: `package.json`, `pnpm-lock.yaml`
- **Environment**: sets `NODE_ENV=development`

### bun-install

Installs Node.js dependencies using Bun.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `bun.lockb`
- **Command**: `bun install`
- **Completion check**: `node_modules` directory exists
- **Watches**: `package.json`, `bun.lockb`

### volta-setup

Installs the pinned Node.js toolchain using Volta.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `volta` command available
- **Command**: `volta install node`
- **Completion check**: `volta which node`
- **Watches**: `package.json`

### mise-tools

Installs tool versions using mise.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.mise.toml`, `mise.toml`
- **Command**: `mise install`
- **Completion check**: `mise current`
- **Watches**: `.mise.toml`, `mise.toml`

### asdf-tools

Installs tool versions using asdf.

- **Platforms**: macOS, Linux
- **Detects**: `.tool-versions`
- **Command**: `asdf install`
- **Completion check**: `asdf current`
- **Watches**: `.tool-versions`

### pip-install

Installs Python packages from `requirements.txt`.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `requirements.txt`, `pyproject.toml`
- **Command**: `pip install -r requirements.txt`
- **Completion check**: `pip check`
- **Watches**: `requirements.txt`

### poetry-install

Installs Python dependencies using Poetry.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `poetry.lock`
- **Command**: `poetry install`
- **Completion check**: `poetry check`
- **Watches**: `pyproject.toml`, `poetry.lock`

### uv-sync

Syncs Python dependencies using uv.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `uv.lock`
- **Command**: `uv sync`
- **Completion check**: `.venv` directory exists
- **Watches**: `pyproject.toml`, `uv.lock`

### composer-install

Installs PHP packages from `composer.json`.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `composer.json`
- **Command**: `composer install`
- **Completion check**: `vendor` directory exists
- **Watches**: `composer.json`, `composer.lock`

### laravel-setup

Bootstraps a Laravel project: creates `.env` from `.env.example` and generates the application key.

- **Platforms**: macOS, Linux, Windows
- **Detects**: Laravel project files
- **Command**: `cp -n .env.example .env 2>/dev/null; php artisan key:generate --force`
- **Completion check**: `.env` file exists
- **Watches**: `.env.example`

### cargo-build

Builds a Rust project using Cargo.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Cargo.toml`
- **Command**: `cargo build`
- **Completion check**: `target` directory exists
- **Watches**: `Cargo.toml`, `Cargo.lock`

### go-mod-download

Downloads Go module dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `go.mod`
- **Command**: `go mod download`
- **Completion check**: `go mod verify`
- **Watches**: `go.mod`, `go.sum`

### swift-resolve

Resolves Swift Package Manager dependencies.

- **Platforms**: macOS, Linux
- **Detects**: `Package.swift`
- **Command**: `swift package resolve`
- **Completion check**: `.build` directory exists
- **Watches**: `Package.swift`, `Package.resolved`

### nvm-node

Installs the Node.js version pinned in `.nvmrc` using nvm.

- **Platforms**: macOS, Linux
- **Detects**: `.nvmrc`
- **Command**: `nvm install`
- **Watches**: `.nvmrc`

### fnm-setup

Installs and activates the Node.js version pinned by `.nvmrc` or `.node-version` using fnm.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.nvmrc`, `.node-version`, or `fnm` available
- **Command**: `fnm install && fnm use`
- **Completion check**: `fnm current`
- **Watches**: `.nvmrc`, `.node-version`

### rbenv-ruby

Installs the Ruby version pinned in `.ruby-version` using rbenv.

- **Platforms**: macOS, Linux
- **Detects**: `.ruby-version`
- **Command**: `rbenv install --skip-existing`
- **Completion check**: `rbenv version`
- **Watches**: `.ruby-version`

### pyenv-python

Installs the Python version pinned in `.python-version` using pyenv.

- **Platforms**: macOS, Linux
- **Detects**: `.python-version`
- **Command**: `pyenv install --skip-existing`
- **Completion check**: `pyenv version`
- **Watches**: `.python-version`

### maven-resolve

Resolves Java dependencies using Maven.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pom.xml`
- **Command**: `mvn dependency:resolve`
- **Completion check**: `target` directory exists
- **Watches**: `pom.xml`

### gradle-deps

Downloads Gradle project dependencies via the Gradle wrapper.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `build.gradle`, `build.gradle.kts`, `settings.gradle`, `settings.gradle.kts`
- **Command**: `./gradlew dependencies`
- **Completion check**: `.gradle` directory exists
- **Watches**: `build.gradle`, `build.gradle.kts`, `settings.gradle*`, `gradle.properties`

### spring-boot-build

Builds a Spring Boot Gradle project (skipping tests).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `application.properties`, `application.yml`
- **Command**: `./gradlew build -x test`
- **Completion check**: `build/libs` directory exists
- **Watches**: `build.gradle`, `build.gradle.kts`, `src/main/resources/application.properties`, `src/main/resources/application.yml`

### mix-deps-get

Installs Elixir project dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `mix.exs`
- **Command**: `mix deps.get`
- **Completion check**: `deps` directory exists
- **Watches**: `mix.exs`, `mix.lock`

### dotnet-restore

Restores .NET project dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `*.sln`, `*.csproj`
- **Command**: `dotnet restore`
- **Completion check**: `dotnet restore --no-restore` reports nothing to do
- **Watches**: `*.sln`, `*.csproj`, `Directory.Build.props`

### dart-pub-get

Installs Dart package dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pubspec.yaml`
- **Command**: `dart pub get`
- **Completion check**: `.dart_tool` directory exists
- **Watches**: `pubspec.yaml`, `pubspec.lock`

### flutter-pub-get

Installs Flutter package dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `pubspec.yaml` (with Flutter SDK dependency)
- **Command**: `flutter pub get`
- **Completion check**: `.dart_tool` directory exists
- **Watches**: `pubspec.yaml`, `pubspec.lock`

### deno-install

Caches Deno project dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `deno.json`, `deno.jsonc`
- **Command**: `deno install`
- **Completion check**: `deno.lock` file exists
- **Watches**: `deno.json`, `deno.jsonc`, `deno.lock`

### nextjs-build

Builds a Next.js application to verify setup.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `next.config.js`, `next.config.mjs`, `next.config.ts`
- **Command**: `npx next build`
- **Completion check**: `.next` directory exists
- **Watches**: `next.config.js`, `next.config.mjs`, `next.config.ts`

### vite-build

Builds a Vite project to verify setup.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `vite.config.ts`, `vite.config.js`, `vite.config.mjs`
- **Command**: `npx vite build`
- **Completion check**: `dist` directory exists
- **Watches**: `vite.config.js`, `vite.config.ts`, `vite.config.mjs`

### remix-build

Builds a Remix application to verify setup.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `remix.config.js`, `remix.config.ts`
- **Command**: `npx remix build`
- **Completion check**: `build` directory exists
- **Watches**: `remix.config.js`, `remix.config.ts`, `app/root.tsx`

### django-migrate

Runs Django database migrations.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `manage.py`
- **Command**: `python manage.py migrate`
- **Watches**: `manage.py`, `*/migrations`

### rails-db

Prepares the Rails database (creates, migrates, seeds).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `bin/rails`, `config/routes.rb`
- **Command**: `bundle exec rails db:prepare`
- **Precondition**: `config/database.yml` is present
- **Watches**: `db/migrate`, `db/seeds.rb`, `config/database.yml`

### prisma-migrate

Runs Prisma database migrations and regenerates the Prisma Client.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `prisma/schema.prisma`
- **Command**: `npx prisma migrate dev`
- **Watches**: `prisma/schema.prisma`, `prisma/migrations`

### diesel-migrate

Sets up the database and runs pending Diesel migrations (Rust).

- **Platforms**: macOS, Linux, Windows
- **Detects**: `diesel.toml`
- **Command**: `diesel setup && diesel migration run`
- **Watches**: `diesel.toml`, `migrations`

### alembic-migrate

Upgrades the database to the latest Alembic migration revision.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `alembic.ini`
- **Command**: `alembic upgrade head`
- **Watches**: `alembic.ini`, `alembic/versions`

### docker-compose-up

Starts services defined in a Docker Compose file.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `compose.yml`, `compose.yaml`, `docker-compose.yml`, `docker-compose.yaml`
- **Command**: `docker compose up -d`
- **Completion check**: `docker compose ps` reports a running service
- **Watches**: `compose.yml`, `compose.yaml`, `docker-compose.yml`, `docker-compose.yaml`

### helm-deps

Updates Helm chart dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Chart.yaml`
- **Command**: `helm dependency update`
- **Completion check**: `charts/` directory exists
- **Watches**: `Chart.yaml`, `Chart.lock`

### terraform-init

Initializes a Terraform working directory.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `*.tf`
- **Command**: `terraform init`
- **Completion check**: `.terraform` directory exists
- **Watches**: `*.tf`, `.terraform.lock.hcl`

### cdk-synth

Synthesizes CloudFormation templates from AWS CDK code.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `cdk.json`
- **Command**: `cdk synth`
- **Completion check**: `cdk.out` directory exists
- **Watches**: `cdk.json`

### pulumi-install

Installs Pulumi project plugins and dependencies.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `Pulumi.yaml`
- **Command**: `pulumi install`
- **Completion check**: `pulumi plugin ls` reports installed plugins
- **Watches**: `Pulumi.yaml`, `Pulumi.lock`

### ansible-install

Installs Ansible Galaxy roles and collections from a `requirements.yml` file.

- **Platforms**: macOS, Linux
- **Detects**: `ansible.cfg`, `playbook.yml`
- **Command**: `ansible-galaxy install -r requirements.yml`
- **Precondition**: `requirements.yml` is present
- **Watches**: `requirements.yml`, `ansible.cfg`

### env-copy

Copies an environment template (`.env.example`, `.env.sample`, or `.env.template`) to `.env` if it does not already exist.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `.env.example`, `.env.sample`, `.env.template`
- **Command**: tries `cp -n .env.example .env`, then falls back to `.env.sample` and `.env.template`
- **Completion check**: `.env` file exists
- **Watches**: `.env.example`, `.env.sample`, `.env.template`

### pre-commit-install

Installs pre-commit hook scripts into your Git repository.

- **Platforms**: macOS, Linux
- **Detects**: `.pre-commit-config.yaml`
- **Command**: `pre-commit install`
- **Completion check**: `.git/hooks/pre-commit` file exists
- **Watches**: `.pre-commit-config.yaml`

### nx-build

Initializes an Nx workspace and builds all projects.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `nx.json`
- **Command**: `npx nx reset && npx nx run-many --target=build --all --skip-nx-cache`
- **Completion check**: `node_modules/.cache/nx` directory exists
- **Watches**: `nx.json`, `workspace.json`

### turbo-build

Builds all packages in a Turborepo workspace.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `turbo.json`
- **Command**: `npx turbo build`
- **Completion check**: `node_modules/.cache/turbo` directory exists
- **Watches**: `turbo.json`, `packages/*/package.json`

### lerna-bootstrap

Bootstraps a Lerna monorepo workspace.

- **Platforms**: macOS, Linux, Windows
- **Detects**: `lerna.json`
- **Command**: `npx lerna bootstrap`
- **Watches**: `lerna.json`, `packages/*/package.json`

## version-bump templates

Each language category provides its own `version-bump` template that updates the version in that ecosystem's manifest. They share the same input shape:

- **Input**: `bump` (required) — `patch`, `minor`, `major`, or an explicit semver string like `1.5.0`.

Reference the language-specific template by qualifying it with the category, since the unqualified `version-bump` resolves to the first match the registry finds:

```yaml
steps:
  bump-version:
    template: rust/version-bump
    inputs:
      bump: minor
```

Available variants: `ruby/version-bump`, `node/version-bump`, `python/version-bump`, `php/version-bump`, `rust/version-bump`, `go/version-bump`, `swift/version-bump`, `java/version-bump`, `kotlin/version-bump`, `elixir/version-bump`, `dotnet/version-bump`.

Two ways to provide the `bump` value:

1. **Template input** — set `inputs: { bump: minor }` directly on the step.
2. **Interactive prompt** — add a `prompts:` block on the step.

Inputs are case-sensitive map keys, not environment variables, so something like `BUMP=minor bivvy run` does not feed the input.

```yaml
steps:
  bump-version:
    template: rust/version-bump
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

The Rust variant requires [cargo-edit](https://github.com/killercup/cargo-edit) (`cargo install cargo-edit`); the Python variant requires [bump-my-version](https://github.com/callowayproject/bump-my-version); the Java variant uses the Maven [versions plugin](https://www.mojohaus.org/versions/versions-maven-plugin/); the Swift variant uses `agvtool`. The Go variant computes the next semver Git tag and prints it to stdout — wire it into your release workflow to actually create the tag.

## Install Templates

Install templates are used by the requirements system to install missing tools. They are triggered automatically when a requirement gap is detected and the user accepts the install prompt.

| Template | Description | Platforms |
|----------|-------------|-----------|
| `brew-install` | Install Homebrew package manager | macOS, Linux |
| `brew-elixir` | Install Elixir using Homebrew | macOS, Linux |
| `brew-go` | Install Go using Homebrew | macOS, Linux |
| `brew-node` | Install Node.js using Homebrew | macOS, Linux |
| `brew-php` | Install PHP using Homebrew | macOS, Linux |
| `brew-python` | Install Python using Homebrew | macOS, Linux |
| `brew-ruby` | Install Ruby using Homebrew | macOS, Linux |
| `mise-install` | Install mise version manager | macOS, Linux |
| `mise-elixir` | Install Elixir using mise | macOS, Linux |
| `mise-node` | Install Node.js using mise | macOS, Linux |
| `mise-php` | Install PHP using mise | macOS, Linux |
| `mise-python` | Install Python using mise | macOS, Linux |
| `mise-ruby` | Install Ruby using mise | macOS, Linux |
| `asdf-elixir` | Install Elixir using asdf | macOS, Linux |
| `asdf-node` | Install Node.js using asdf | macOS, Linux |
| `asdf-php` | Install PHP using asdf | macOS, Linux |
| `asdf-python` | Install Python using asdf | macOS, Linux |
| `asdf-ruby` | Install Ruby using asdf | macOS, Linux |
| `fnm-node` | Install Node.js using fnm | macOS, Linux, Windows |
| `nvm-node` | Install Node.js using nvm | macOS, Linux |
| `volta-node` | Install Node.js using Volta | macOS, Linux, Windows |
| `pyenv-python` | Install Python using pyenv | macOS, Linux |
| `rbenv-ruby` | Install Ruby using rbenv | macOS, Linux |
| `rust-install` | Install Rust toolchain via rustup | macOS, Linux, Windows |
| `docker-install` | Install Docker Desktop or Docker Engine | macOS, Linux, Windows |
| `postgres-install` | Install PostgreSQL server and client tools | macOS, Linux |
| `redis-install` | Install Redis in-memory data store | macOS, Linux |

### brew-install

Installs Homebrew package manager.

- **Platforms**: macOS, Linux
- **Command**: `/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"`
- **Completion check**: `brew --version`

### docker-install

Installs Docker Desktop (macOS) or Docker Engine (Linux). On macOS, the template prints download instructions and exits with an error rather than installing automatically.

- **Platforms**: macOS, Linux, Windows
- **Command**: Platform-specific (manual on macOS, `curl -fsSL https://get.docker.com | sh` on Debian/Ubuntu)
- **Completion check**: `docker info`

### mise-install

Installs the mise version manager.

- **Platforms**: macOS, Linux
- **Command**: `curl https://mise.run | sh`
- **Completion check**: `mise --version`

### mise-node / mise-python / mise-ruby / mise-php / mise-elixir

Installs the named language using mise. Requires `mise` to be installed first.

- **Platforms**: macOS, Linux
- **Requires**: `mise`
- **Command**: `mise install <language>`
- **Completion check**: `mise where <language>`

### asdf-node / asdf-python / asdf-ruby / asdf-php / asdf-elixir

Adds the asdf plugin (if missing) and installs the project's pinned version. Requires `asdf` to be installed first.

- **Platforms**: macOS, Linux
- **Requires**: `asdf`
- **Command**: `asdf plugin add <language> 2>/dev/null; asdf install <language>`
- **Completion check**: `asdf where <language>`

### brew-node / brew-python / brew-ruby / brew-go / brew-php / brew-elixir

Installs the named language using Homebrew. Requires `brew` to be installed first.

- **Platforms**: macOS, Linux
- **Requires**: `brew`
- **Command**: `brew install <language>`

### fnm-node

Installs the project's Node.js version using fnm.

- **Platforms**: macOS, Linux, Windows
- **Requires**: `fnm`
- **Command**: `fnm install && fnm use`

### nvm-node

Installs the Node.js version pinned by `.nvmrc` using nvm.

- **Platforms**: macOS, Linux
- **Requires**: `nvm`
- **Command**: `nvm install`

### volta-node

Installs the project's pinned Node.js version using Volta.

- **Platforms**: macOS, Linux, Windows
- **Requires**: `volta`
- **Command**: `volta install node`

### pyenv-python

Installs the Python version pinned by `.python-version` using pyenv (skipping if already present).

- **Platforms**: macOS, Linux
- **Requires**: `pyenv`
- **Command**: `pyenv install --skip-existing`

### rbenv-ruby

Installs the Ruby version pinned by `.ruby-version` using rbenv (skipping if already present).

- **Platforms**: macOS, Linux
- **Requires**: `rbenv`
- **Command**: `rbenv install --skip-existing`

### postgres-install

Installs PostgreSQL database server and client tools.

- **Platforms**: macOS, Linux
- **Command**: Platform-specific (Homebrew on macOS, `apt-get` on Debian/Ubuntu)
- **Completion check**: `pg_isready -q`

### redis-install

Installs Redis in-memory data store.

- **Platforms**: macOS, Linux
- **Command**: Platform-specific (Homebrew on macOS, `apt-get` on Debian/Ubuntu)
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
    template: bundle-install
    env:
      BUNDLE_WITHOUT: "production"
    command: "bundle install --jobs=4"
```

The template provides the base configuration, and your overrides take precedence.
