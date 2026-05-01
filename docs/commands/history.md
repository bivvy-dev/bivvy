---
title: bivvy history
description: Show execution history
---

# bivvy history

Shows the execution history for your project.

## Usage

```bash
bivvy history
```

## Flags

| Flag | Description |
|------|-------------|
| `--limit <N>` | Number of runs to show (default: 10) |
| `--since <duration>` | Show runs since a duration ago (supports `m`, `h`, `d`, `w` suffixes) |
| `--step <name>` | Reserved. Currently prints `Note: --step filter is not yet supported with event log history.` and is otherwise ignored, because `WorkflowCompleted` events only carry step counts, not names. |
| `--detail` | For each run, print an extra line with the run/skipped step counts and the source log filename. |
| `--json` | Output as JSON |
| `--clear` | Delete this project's run history |
| `-f`, `--force` | Skip the confirmation prompt when used with `--clear` |

## Example Output

```
  ⛺ Run History

    ✓  3 minutes ago      default      2 steps  2m 15s
    ✓  yesterday           default      3 steps  5m 30s
    ✗  2 days ago          ci           1 step   45s
    ✓  3 days ago          default      2 steps  3m 10s
```

## Filtering by Time

Use `--since` with a duration suffix to show only recent runs:

```bash
# Runs in the last hour
bivvy history --since 1h

# Runs in the last 7 days
bivvy history --since 7d

# Runs in the last 30 minutes
bivvy history --since 30m

# Runs in the last 2 weeks
bivvy history --since 2w
```

## Step Filter (not yet implemented)

`--step <name>` is accepted by the parser but currently emits the line

```
Note: --step filter is not yet supported with event log history.
```

and then runs as if the flag were not given. The reason: `bivvy history`
is reconstructed from `WorkflowCompleted` events, which only record
`steps_run` and `steps_skipped` counts — not the names of individual steps.
For per-step results from a single run, use `bivvy last --step <name>`.

## Detailed View

Use `--detail` to add a one-line summary under each run with the run/skipped
counts and the source log filename:

```bash
bivvy history --detail
```

```
  ⛺ Run History

    ✓  3 minutes ago      default      2 steps  2m 15s
        2 steps run, 1 skipped
        Log: 2026-04-25T14-32-05_a1b2c3d4.jsonl
    ✗  yesterday           ci           1 step   45s
        1 steps run, 0 skipped (aborted)
        Log: 2026-04-24T08-12-09_deadbeef.jsonl
```

`--detail` does not currently print individual step names or error
messages. To inspect a specific run's steps, open the JSONL log file shown
on the `Log:` line under `~/.bivvy/logs/`, or use `bivvy last --all` for
per-run step lists reconstructed from `StepCompleted` events.

## Limiting Results

By default, the last 10 runs are shown. Use `--limit` to show more:

```bash
bivvy history --limit 50
```

## JSON Output

Use `--json` for machine-readable output:

```bash
bivvy history --json
```

This outputs the filtered run records as a JSON array, suitable for piping to tools like `jq`. Each record has these fields:

```json
[
  {
    "timestamp": "2026-04-25T14:32:05Z",
    "workflow": "default",
    "success": true,
    "aborted": false,
    "steps_run": 2,
    "steps_skipped": 1,
    "duration_ms": 135000
  }
]
```

`steps_run` and `steps_skipped` are integer counts (not arrays of names);
the source log filename used by `--detail` is not included in JSON output.

## Clearing History

Use `--clear` to delete the run history for the current project:

```bash
bivvy history --clear
```

This only removes log files that belong to the current project. Logs from
other projects in `~/.bivvy/logs/` are untouched. You'll be prompted to
confirm before any files are deleted.

To skip the confirmation prompt (e.g., in scripts), add `--force`:

```bash
bivvy history --clear --force
```

Run history is for reporting only — clearing it does not affect step
satisfaction state. To re-evaluate steps from scratch, use `bivvy run --fresh`.
