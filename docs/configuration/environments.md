---
title: Environments
description: Adapting Bivvy behavior to different environments
---

# Environments

Environments let Bivvy adapt its behavior based on where setup is running.
A CI server can skip interactive tools, a Docker container can skip services
that are already running externally, and a Codespace can use pre-installed
runtimes instead of installing from scratch.

## How environments are resolved

Bivvy picks a single active environment using this priority chain:

1. **`--env` flag** — explicit override on the command line
2. **`default_environment`** — setting in config
3. **Auto-detection** — inspects environment variables and filesystem
4. **Fallback** — defaults to `development`

The first match wins.

## Built-in environments

Bivvy recognizes four built-in environments:

| Environment | Detection signals |
|-------------|-------------------|
| `ci` | `CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, `CIRCLECI`, `JENKINS_URL`, `BUILDKITE`, `TRAVIS`, `TF_BUILD=True` |
| `codespace` | `CODESPACES`, `GITPOD_WORKSPACE_ID` |
| `docker` | `DOCKER_CONTAINER`, `/.dockerenv` file exists |
| `development` | Fallback when nothing else matches |

Auto-detection checks custom rules first (alphabetically), then CI,
Codespace, and Docker — in that order.

## Using the `--env` flag

Override auto-detection with an explicit environment:

```bash
bivvy run --env staging
```

```bash
bivvy status --env ci
```

## Default environment

Set a default in your config so you don't need `--env` every time:

```yaml
settings:
  default_environment: staging
```

The flag still takes precedence if both are set.

## Custom environments

Define project-specific environments under `settings.environments`:

```yaml
settings:
  environments:
    staging:
      detect:
        - env: DEPLOY_ENV
          value: staging
      default_workflow: staging
      provided_requirements:
        - postgres-server
        - redis-server

    review_app:
      detect:
        - env: REVIEW_APP
      provided_requirements:
        - docker
```

Each custom environment supports:

| Field | Type | Description |
|-------|------|-------------|
| `detect` | list of rules | Environment variable conditions for auto-detection |
| `default_workflow` | string | Workflow to run by default in this environment |
| `provided_requirements` | list | Requirements to skip (assumed already satisfied) |

### Detect rules

Each detect rule checks a single environment variable:

```yaml
detect:
  - env: DEPLOY_ENV          # Check if DEPLOY_ENV is set
    value: staging            # Optional: also check that value matches
  - env: STAGING_HOST         # Presence check only (no value)
```

If `value` is omitted, the rule matches when the variable is set to any
value. All rules in the list are checked independently — any single match
triggers the environment.

## Step filtering with `only_environments`

Restrict a step to specific environments:

```yaml
steps:
  seed_data:
    command: rails db:seed
    only_environments:
      - development
      - staging
```

Steps with an empty `only_environments` list (the default) run in all
environments. Steps whose `only_environments` does not include the active
environment are skipped.

## Per-environment step overrides

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
          DATABASE_URL: postgres://localhost/test
      docker:
        env:
          DATABASE_HOST: db
          RAILS_ENV: null  # Remove RAILS_ENV from this environment
```

Only fields you specify are overridden. Everything else inherits from the
base step. Set an env var to `null` to remove it in that environment.

Overridable fields:

| Field | Description |
|-------|-------------|
| `title` | Display title |
| `description` | Human-readable description |
| `command` | Shell command |
| `env` | Environment variables (`null` value removes a key) |
| `completed_check` | Completion detection |
| `skippable` | Whether user can skip |
| `allow_failure` | Continue on failure |
| `requires_sudo` | Needs elevated permissions |
| `sensitive` | Hide command and output |
| `before` | Pre-step hooks |
| `after` | Post-step hooks |
| `depends_on` | Step dependencies |
| `requires` | System requirements |
| `watches` | Files triggering re-run |
| `retry` | Retry attempts |

## Provided requirements

Environments can declare requirements as already satisfied, skipping gap
detection and install prompts:

```yaml
settings:
  environments:
    ci:
      provided_requirements:
        - docker
        - postgres-server
        - redis-server
```

This is useful for CI where services are managed by the pipeline, or for
containers where tools are pre-installed in the image.

See [Requirements](requirements.md) for more on gap detection.
