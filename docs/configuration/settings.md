# Settings

Global settings control Bivvy's behavior across all workflows.

## Basic Settings

```yaml
settings:
  default_output: verbose  # verbose | quiet | silent
  logging: false
  log_path: "logs/bivvy.log"
```

## Output Modes

| Mode | Description |
|------|-------------|
| `verbose` | Show all output (default) |
| `quiet` | Show only step names and errors |
| `silent` | Show only errors |

Override per-run:

```bash
bivvy --verbose
```

```bash
bivvy --quiet
```

```bash
bivvy --silent
```

## Global Environment Variables

Set environment variables for all steps:

```yaml
settings:
  env:
    RAILS_ENV: development
    DEBUG: "true"
```

## Parallel Execution

```yaml
settings:
  parallel: true      # Enable parallel execution
  max_parallel: 4     # Maximum concurrent steps (default: 4)
```

## History Retention

```yaml
settings:
  history_retention: 50  # Keep last 50 runs (default)
```

## Fail Fast

Stop workflow on first failure:

```yaml
settings:
  fail_fast: true  # Stop on first error (default: true)
```

## Skip Behavior

How to handle dependencies of skipped steps:

```yaml
settings:
  skip_behavior: skip_with_dependents  # Also skip dependent steps
  # skip_behavior: run_dependents      # Still try to run dependents
```

## Default Environment

Set the environment used when `--env` is not provided and no environment
is auto-detected:

```yaml
settings:
  default_environment: staging
```

If omitted, Bivvy falls back to auto-detection and then to `development`.

## Environments

Define custom environments with detection rules, default workflows, and
provided requirements:

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

| Field | Type | Description |
|-------|------|-------------|
| `detect` | list | Environment variable rules for auto-detection |
| `default_workflow` | string | Workflow to use when this environment is active |
| `provided_requirements` | list | Requirements assumed satisfied (skip gap checks) |

Each detect rule checks a single env var. Omit `value` to match on
presence alone.

See [Environments](environments.md) for the full guide.

## Auto-Update

Bivvy checks for new versions in the background after each run and
installs updates automatically. This is enabled by default.

```yaml
settings:
  auto_update: true  # Enable automatic background updates (default)
```

To disable:

```yaml
settings:
  auto_update: false
```

This setting is typically placed in the system config (`~/.bivvy/config.yml`)
since it applies to bivvy itself, not to a specific project. You can also
toggle it from the command line:

```bash
bivvy update --disable-auto-update
bivvy update --enable-auto-update
```

When disabled, you can still update manually with `bivvy update`.

See [`bivvy update`](../commands/update.md) for details on how background
updates work.
