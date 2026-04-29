---
title: bivvy run
description: Run setup workflow
---

# bivvy run

The main command to run your setup workflow.

## Usage

```bash
bivvy
```

```bash
bivvy run
```

```bash
bivvy run -w ci
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--workflow` | `-w` | Workflow to run (default: "default") |
| `--only` | | Run only specified steps (comma-separated) |
| `--skip` | | Skip specified steps (comma-separated) |
| `--skip-behavior` | | How to handle skipped dependencies |
| `--force` | `-f` | Force re-run of specified steps (comma-separated) |
| `--force-all` | | Force re-run of every step, bypassing all checks and step-level configuration |
| `--resume` | | Resume interrupted run |
| `--save-preferences` | | Save prompt answers |
| `--dry-run` | | Preview without executing |
| `--env` | `-e` | Set active environment (e.g., `ci`, `staging`) |
| `--diagnostic-funnel` | | Force diagnostic analysis on (overrides config) |
| `--no-diagnostic-funnel` | | Disable diagnostic analysis, use legacy pattern matching |
| `--non-interactive` | | Use defaults, no prompts |
| `--ci` | | Deprecated: use `--non-interactive` and `--env ci` instead |

## Skip Behaviors

When using `--skip`, you can control how dependents are handled:

- `skip_with_dependents` (default): Skip the step and all its dependents
- `skip_only`: Skip only this step, attempt to run dependents
- `run_anyway`: Don't actually skip, run the step anyway

## Examples

Run only database setup:

```bash
bivvy run --only=database
```

Skip seeds step:

```bash
bivvy run --skip=seeds
```

Force re-run of node_deps:

```bash
bivvy run --force=node_deps
```

Force re-run of every step in the workflow, bypassing checks and any
step-level configuration:

```bash
bivvy run --force-all
```

Preview what would run:

```bash
bivvy run --dry-run
```

Run with a specific environment:

```bash
bivvy run --env staging
```

Non-interactive mode:

```bash
bivvy run --non-interactive
```

Run with a specific workflow:

```bash
bivvy run --workflow=production
```

## Failure Recovery

When a step fails, Bivvy analyzes the error output and presents an interactive
recovery menu with fix suggestions, retry, skip, shell, and abort options. See
the [Failure Diagnostics guide](/guides/diagnostics/) for details on how Bivvy
identifies errors and generates fix suggestions.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All steps completed successfully |
| 1 | One or more steps failed |
| 2 | Configuration not found |
| 130 | Interrupted (Ctrl+C) |
