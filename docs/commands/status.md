---
title: bivvy status
description: Show current setup status
---

# bivvy status

Shows the current status of your setup without running anything.

## Usage

```bash
bivvy status
```

```bash
bivvy status <workflow>
```

```bash
bivvy status --json
```

```bash
bivvy status --step=name
```

```bash
bivvy status --env ci
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<workflow>` | Optional. Show status for a specific workflow. When provided, the workflow's portable steps are loaded alongside the project-level ones, so steps bundled inside `.bivvy/workflows/<workflow>.yml` are visible. Without it, only project-level steps are shown. |

## Flags

| Flag | Description |
|------|-------------|
| `--json` | Output status as JSON instead of styled text |
| `--step <name>` | Show status for a specific step only |
| `--env <ENV>` | Check status for a specific environment |

## Scope and Load Profile

`bivvy status` chooses its loader based on what you ask for:

- **No positional**: cheap project-only load (`.bivvy/config.yml` only) — fast enough for a status overview.
- **Positional `<workflow>`**: same loader as `bivvy run`. Walks the full resolution chain (`extends:` → `~/.bivvy/config.yml` → `.bivvy/config.yml` → `.bivvy/steps/*.yml` → the named `.bivvy/workflows/<name>.yml` → `.bivvy/config.local.yml`) so workflow-bundled steps and overrides are visible.

## Example Output

```
  ⛺ MyApp — Status

  Environment: development (default)

  Last activity: 3 hours ago

  Steps:
    ✓ hello                1.2s
    ✗ world                0.8s
    ◌ database
    ⊘ ci_only              (skipped in development)
```

The `Last activity` line is shown only when at least one step in the project
has a recorded `last_run` timestamp. It reflects the most recent step
timestamp regardless of which workflow ran it; no workflow name is printed.

## Status Indicators

| Symbol | Meaning |
|--------|---------|
| `✓` | Success - step completed successfully |
| `✗` | Failed - last run failed |
| `◌` | Pending - step hasn't been executed yet |
| `⊘` | Skipped - step is excluded in the current environment |

## Recommendations

When there are steps that haven't been run, the status command will suggest:

```
Run `bivvy run --only=database` to run remaining steps
```

## Environment

The status output always shows the active environment and how it was resolved:

```
  Environment: ci (detected via CI)
```

Use `--env` to check status for a different environment. Steps with
`only_environments` restrictions will show as skipped when they don't apply.

## Requirements

When steps have `requires` entries, the status output includes a requirements
section:

```
  Requirements:
    ✓ ruby                  available
    ✓ node                  available
    ✗ postgres              not running (start with: brew services start postgresql@16)
    ⚠ python                System Python detected
```

## JSON Output

Use `--json` for machine-readable output:

```bash
bivvy status --json
```

```json
{
  "app_name": "MyApp",
  "environment": {
    "name": "development",
    "source": "default"
  },
  "steps": [
    {
      "name": "hello",
      "status": "success",
      "last_run": "2026-03-30T10:00:00+00:00",
      "duration_ms": 1200
    },
    {
      "name": "world",
      "status": "failed",
      "last_run": "2026-03-30T10:00:00+00:00",
      "duration_ms": 800
    },
    {
      "name": "database",
      "status": "pending"
    },
    {
      "name": "ci_only",
      "status": "skipped",
      "reason": "skipped in development"
    }
  ],
  "requirements": [
    {
      "name": "node",
      "status": "satisfied"
    },
    {
      "name": "ruby",
      "status": "satisfied"
    }
  ]
}
```

The top-level keys are `app_name`, `environment`, `steps`, and (only when
the config declares any `requires:` entries) `requirements`. There is no
top-level `last_run` field — per-step `last_run` and `duration_ms` are only
emitted on steps that have run before.

Step statuses in JSON are: `"success"`, `"failed"`, `"pending"`, or
`"skipped"`. Steps that the active environment excludes are reported with
`"status": "skipped"` and a `"reason"` string explaining the exclusion
(e.g. `"skipped in development"`). Requirement statuses are: `"satisfied"`,
`"warning"`, `"missing"`, or `"unknown"`, with an optional `"detail"` field.
