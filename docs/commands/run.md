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
| `--force` | `-f` | Force re-run of specified steps |
| `--resume` | | Resume interrupted run |
| `--save-preferences` | | Save prompt answers |
| `--dry-run` | | Preview without executing |
| `--env` | `-e` | Set active environment (e.g., `ci`, `staging`) |
| `--non-interactive` | | Use defaults, no prompts |
| `--ci` | | Deprecated: use `--non-interactive` instead |

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

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All steps completed successfully |
| 1 | One or more steps failed |
| 2 | Configuration not found |
| 130 | Interrupted (Ctrl+C) |
