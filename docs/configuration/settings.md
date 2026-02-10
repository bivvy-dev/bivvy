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
