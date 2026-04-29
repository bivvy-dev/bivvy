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
| `--step <name>` | Filter to runs containing the named step |
| `--detail` | Show steps, skipped steps, and errors for each run |
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

## Step History

Filter to only runs that included a specific step:

```bash
bivvy history --step ruby_deps
```

This shows only runs where the named step was executed or skipped. When combined with `--detail`, only that step's information is highlighted in context.

## Detailed View

Use `--detail` to see which steps ran, which were skipped, and any errors:

```bash
bivvy history --detail
```

```
  ⛺ Run History

    ✓  3 minutes ago      default      2 steps  2m 15s
        Steps: setup, build
        Skipped: deploy
    ✗  yesterday           ci           1 step   45s
        Steps: setup
        Error: build step failed
```

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

This outputs the filtered run records as a JSON array, suitable for piping to tools like `jq`.

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
