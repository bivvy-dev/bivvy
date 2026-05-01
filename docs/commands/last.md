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

The skipped row uses the same dim `○` glyph that `bivvy status` and the run
summary use for any `Skipped` status.

## JSON Output

With `--json`, the run data is printed as a JSON object reconstructed from
the most recent JSONL event log:

```bash
bivvy last --json
```

```json
{
  "timestamp": "2024-01-15T14:32:05Z",
  "workflow": "default",
  "success": true,
  "aborted": false,
  "steps_run_count": 3,
  "steps_skipped_count": 1,
  "duration_ms": 135000,
  "steps_run": ["brew", "mise", "ruby_deps"],
  "steps_skipped": ["seeds"]
}
```

Field summary:

| Field | Type | Notes |
|-------|------|-------|
| `timestamp` | string (RFC 3339) | When the workflow completed |
| `workflow` | string | Workflow name |
| `success` | bool | `true` if every step succeeded |
| `aborted` | bool | `true` when the user interrupted the run |
| `steps_run_count` | integer | Total number of steps that ran |
| `steps_skipped_count` | integer | Total number of steps that were skipped |
| `duration_ms` | integer | Total wall-clock duration in milliseconds |
| `steps_run` | string[] | Names of steps that ran (omitted when empty) |
| `steps_skipped` | string[] | Names of steps that were skipped (omitted when empty) |
| `error` | string | First captured error message (only on failure) |

When combined with `--all`, the output is a JSON array of objects with the
same shape, ordered most-recent first.

There is no top-level `status` field — derive it from `success` and
`aborted` (`success: true` → success; `aborted: true` → interrupted; else
failed). The styled terminal output uses the same logic to decide between
`✓ Success`, `✗ Failed`, and `Interrupted`.

## Filtering by Step

Use `--step` to show only a single step's result:

```bash
bivvy last --step brew
```

If the step was not part of the last run, an error is shown and the command
exits with code 1:

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
