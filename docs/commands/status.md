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
| `<workflow>` | Optional. Show status for a specific workflow. When provided, steps bundled inside the matching `.bivvy/workflows/<name>.yml` are visible alongside the project-level ones. |

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

  Last run: 3 hours ago · default workflow

  Steps:
    ✓ hello                 1.2s
    ✗ world                 0.8s
    ◌ database
    ⊘ ci_only               (skipped in development)
```

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
  "last_run": {
    "timestamp": "2026-03-30T10:00:00Z",
    "workflow": "default"
  },
  "steps": [
    {
      "name": "hello",
      "status": "success",
      "last_run": "2026-03-30T10:00:00Z",
      "duration_ms": 1200
    },
    {
      "name": "world",
      "status": "failed",
      "last_run": "2026-03-30T10:00:00Z",
      "duration_ms": 800
    },
    {
      "name": "database",
      "status": "pending"
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

Step statuses in JSON are: `"success"`, `"failed"`, `"pending"`, or `"skipped"`.
Requirement statuses are: `"satisfied"`, `"warning"`, `"missing"`, or `"unknown"`.
