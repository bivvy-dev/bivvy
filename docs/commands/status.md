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
bivvy status --json
```

```bash
bivvy status --step=name
```

```bash
bivvy status --env ci
```

## Example Output

```
Test - Status
Last run: 2024-01-15 14:32 (default)

Steps:
  [ok] hello
  [FAIL] world
  [pending] database

Legend: [ok] passed  [FAIL] failed  [pending] not yet run
```

## Status Indicators

| Symbol | Meaning |
|--------|---------|
| `[ok]` | Passed - step completed successfully |
| `[FAIL]` | Failed - last run failed |
| `[skip]` | User-skipped - step was explicitly skipped |
| `[pending]` | Not yet run - step hasn't been executed |

## Recommendations

When there are steps that haven't been run, the status command will suggest:

```
Run `bivvy run --only=database` to run remaining steps
```

## Environment

The status output shows the active environment:

```
Environment: ci (auto-detected via CI)
```

Use `--env` to check status for a different environment.

## Requirements

When steps have requirements, the status output includes gap indicators:

```
Requirements:
  ✓ ruby (mise)
  ✓ node (mise)
  ✗ postgres-server — not running (start with: pg_ctl start)
  ○ docker — provided by environment
```
