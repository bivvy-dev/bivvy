---
title: Step Configuration
description: Configuring setup steps in Bivvy
---

# Step Configuration

Steps are the building blocks of Bivvy workflows. Each step represents
a task to be executed during setup.

## Basic Step

```yaml
steps:
  install_deps:
    command: npm install
    title: Install dependencies
    description: Install Node.js dependencies from package.json
```

## Sensitive Steps

Mark steps that handle sensitive data:

```yaml
steps:
  fetch-secrets:
    command: vault read secret/myapp
    sensitive: true
    description: Fetch secrets from Vault
```

Sensitive steps receive special treatment:

1. **Confirmation prompt**: In interactive mode, Bivvy asks for
   confirmation before running sensitive steps

2. **Hidden in dry-run**: The actual command is not shown during
   `--dry-run`, displaying `[SENSITIVE - command hidden]` instead

3. **Suppressed output**: Command output is not logged to prevent
   accidental exposure of sensitive data

4. **No history**: Sensitive commands are not recorded in execution
   history

## Completed Checks

Determine if a step is already complete:

```yaml
steps:
  node_modules:
    command: npm install
    completed_check:
      type: file_exists
      path: node_modules

  bundle:
    command: bundle install
    completed_check:
      type: command_succeeds
      command: bundle check
```

### Check Types

- `file_exists`: Check if a file or directory exists
- `command_succeeds`: Check if a command exits with code 0
- `marker`: Use Bivvy's internal marker system
- `all`: All sub-checks must pass
- `any`: At least one sub-check must pass

## Dependencies

Specify step dependencies:

```yaml
steps:
  database:
    command: rails db:setup
    depends_on: [deps, migrations]
```

## Environment Variables

Set step-specific environment variables:

```yaml
steps:
  test:
    command: npm test
    env:
      NODE_ENV: test
      CI: "true"
    env_file: .env.test
```

## Hooks

Run commands before and after the step:

```yaml
steps:
  database:
    command: rails db:setup
    before:
      - echo "Starting database setup..."
    after:
      - echo "Database ready!"
```

## Requirements

Declare system-level prerequisites that must be available before a step
runs:

```yaml
steps:
  bundle_install:
    command: bundle install
    requires:
      - ruby
      - postgres-server
```

When a requirement is missing, Bivvy offers to install it before running
the step. See [Requirements](requirements.md) for the full list of
built-in requirement names and custom requirement definitions.

## Environment Filtering

Restrict a step to specific environments with `only_environments`:

```yaml
steps:
  seed_data:
    command: rails db:seed
    only_environments:
      - development
      - staging
```

Steps with an empty list (the default) run in all environments.

## Per-Environment Overrides

Override step fields for specific environments:

```yaml
steps:
  database_setup:
    command: rails db:setup
    env:
      RAILS_ENV: development

    environments:
      ci:
        command: rails db:schema:load
        env:
          RAILS_ENV: test
      docker:
        env:
          DATABASE_HOST: db
          RAILS_ENV: null  # Removes RAILS_ENV in Docker
```

Only the fields you specify are overridden; everything else inherits from
the base step. Set an env var to `null` to remove it for that environment.

See [Environments](environments.md) for all overridable fields and
detection configuration.
