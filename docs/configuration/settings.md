# Settings

Global settings control Bivvy's behavior across all workflows.

## Basic Settings

```yaml
settings:
  defaults:
    output: verbose        # verbose | quiet | silent
  logging: true            # Enable JSONL event logging (default: true)
  log_retention_days: 30   # Max age of log files in days (default: 30)
  log_retention_mb: 500    # Max total size of log files in MB (default: 500)
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

Or load from a file:

```yaml
settings:
  env_file: .env.bivvy
```

Both forms can be combined; values in `env:` win over `env_file`. These
project-wide values are the lowest layer in the env stack — workflow
and step values override them, and shell-exported variables override
everything. See
[Environment Variable Precedence](steps.md#environment-variable-precedence).

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

## Diagnostic Funnel

Control the step failure recovery pipeline:

```yaml
settings:
  diagnostic_funnel: true  # Use diagnostic funnel for failure recovery (default: true)
```

When enabled, step failures are analyzed by a multi-stage pipeline that
produces ranked resolution candidates. When disabled, the legacy pattern
registry is used (single fix per error).

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

## Step Defaults

Set default behavior for all steps:

```yaml
settings:
  defaults:
    auto_run: true          # Auto-run unsatisfied steps (default: true)
    prompt_on_rerun: false  # Skip satisfied steps silently (default: false)
    rerun_window: "4h"      # How long a successful run counts as satisfied (default: "4h")
```

See [Auto-Run and the Decision Engine](../guides/auto-run.md) for details
on how these settings interact.

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
