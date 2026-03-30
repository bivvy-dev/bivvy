---
title: bivvy last
description: Show last run information
---

# bivvy last

Shows details about the most recent execution, including workflow name, timing, status, and per-step results.

## Usage

```bash
bivvy last
bivvy last --json
bivvy last --step <name>
bivvy last --all
bivvy last --output
```

## Flags

| Flag | Description |
|------|-------------|
| `--json` | Output run data as JSON instead of styled terminal output |
| `--step <name>` | Show details for a specific step only. Errors if the step was not part of the run |
| `--all` | Show all recorded runs, not just the most recent |
| `--output` | Include captured command output (stdout/stderr) for each step, if available |

## Example Output

```
  ⛺ Last Run

  Workflow:  default
  When:      3 hours ago (2024-01-15 14:32:05)
  Duration:  2m 15s
  Status:    ✓ Success

  Steps:
    ✓ brew                 1m 02s
    ✓ mise                 28s
    ✓ ruby_deps            45s
    ○ seeds                skipped
```

## JSON Output

With `--json`, the run data is printed as a JSON object:

```bash
bivvy last --json
```

```json
{
  "timestamp": "2024-01-15T14:32:05Z",
  "workflow": "default",
  "duration_ms": 135000,
  "status": "Success",
  "steps_run": ["brew", "mise", "ruby_deps"],
  "steps_skipped": ["seeds"],
  "error": null
}
```

## Filtering by Step

Use `--step` to show only a single step's result:

```bash
bivvy last --step brew
```

If the step was not part of the last run, an error is shown:

```
Step 'unknown' was not part of the last run.
```

## Showing All Runs

Use `--all` to display every recorded run (not just the most recent):

```bash
bivvy last --all
```

Each run is displayed with its own header (e.g., "Run 1 of 5", "Run 2 of 5"). Combine with `--json` to get all runs as a JSON array.

## Including Command Output

Use `--output` to request captured command output for each step:

```bash
bivvy last --output
```

If output was not captured during the run, a note is shown for each step indicating that no captured output is available in the run history.

## No History

If no runs have been recorded:

```
No runs recorded for this project.
```

## Failed Run

If the last run failed, the error message is displayed:

```
  Status:    ✗ Failed

  Error: Step 'database' failed with exit code 1
```
