# State Management

Bivvy tracks execution state to enable smart re-runs, skip completed steps, and detect when watched files have changed.

## How State Works

When you run Bivvy, it:

1. Identifies your project using a hash of the path and git remote
2. Loads any existing state from `~/.bivvy/projects/{hash}/`
3. Tracks which steps run, skip, or fail
4. Saves per-step state after each workflow execution
5. Appends a structured event log of the run to `~/.bivvy/logs/`

## Project Identification

Projects are uniquely identified by combining:

- The absolute path to the project root
- The git remote URL (if available)

This produces a stable hash used as the state directory name, ensuring state persists even if you rename the project folder (as long as the git remote stays the same).

## Step State

Bivvy tracks the following for each step:

| Field | Description |
|-------|-------------|
| `last_run` | When the step last executed |
| `status` | Success, Failed, Skipped, or NeverRun |
| `duration_ms` | How long execution took |

### Step Status Values

- **Success** — Step completed without errors
- **Failed** — Step exited with non-zero code or error
- **Skipped** — Step was skipped (already complete or user choice)
- **NeverRun** — Step has never been executed

## Run History

As of state schema v3, run history is no longer stored alongside
per-step state. Each workflow execution is captured as a JSON Lines
event log in `~/.bivvy/logs/`. Each line is a structured event
record (`WorkflowStarted`, `StepCompleted`, `WorkflowCompleted`,
etc.) tagged with the project, workflow, timestamp, and outcome.

The `bivvy last` and `bivvy history` commands read these JSONL logs
directly.

### Run Status Values

- **Success** — All steps completed successfully
- **Failed** — One or more steps failed
- **Aborted** — Execution was cancelled (Ctrl+C) or interrupted

## Change Detection

When a step has `change` checks configured, Bivvy detects if target files have changed from a stored baseline:

```yaml
steps:
  dependencies:
    command: bundle install
    checks:
      - type: change
        target: Gemfile
        on_change: proceed
      - type: change
        target: Gemfile.lock
        on_change: proceed
```

Change detection computes a SHA-256 hash of the target and compares it to the stored baseline. If the hashes differ, the step re-runs. Baselines are stored in `~/.bivvy/projects/{hash}/snapshots/` and updated after each successful execution.

> Still using the legacy `watches:` field? See the [Migrate to the New Check Schema](../guides/migrate-to-checks.md) guide to convert it (along with `completed_check:` and `prompt_if_complete:`) to the modern check fields.

## Preferences

User choices are saved in `preferences.yml`:

```yaml
prompts:
  db_name: myapp_development
  install_mode: frozen

skip_behavior:
  seeds: skip_only

template_sources:
  rails: builtin
```

### Saved Preferences

| Type | Purpose |
|------|---------|
| `prompts` | Answers to interactive prompts |
| `skip_behavior` | How to handle skipped steps |
| `template_sources` | Which template source to use on collision |

## State File Locations

| File | Location |
|------|----------|
| Per-step state | `~/.bivvy/projects/{hash}/state.yml` |
| Preferences | `~/.bivvy/projects/{hash}/preferences.yml` |
| Project Index | `~/.bivvy/projects/index.yml` |
| Run event logs | `~/.bivvy/logs/*.jsonl` |
| Change-detection snapshots | `~/.bivvy/projects/{hash}/snapshots/` |

## Log Retention and Pruning

Bivvy automatically manages log file size — there is **no manual
prune command**. Two settings under top-level `settings:` control
how aggressively old logs are deleted:

```yaml
settings:
  log_retention_days: 30   # Max age of log files in days (default: 30)
  log_retention_mb: 500    # Max total size of log files in MB (default: 500)
```

Files older than `log_retention_days` are deleted on each run.
When the on-disk total exceeds `log_retention_mb`, the oldest files
are deleted first until the cap is satisfied.

Step-level state retention (the count of stored runs) is governed
by `settings.history_retention` (default: 50).

## Querying State

### Last Run

```bash
bivvy last
```

Shows details of the most recent run including duration, status, and which steps executed.

### Full History

```bash
bivvy history
```

Shows a table of recent runs with timestamps and outcomes. Useful
filters and flags:

| Flag | Effect |
|---|---|
| `--limit N` | Show only the last N runs |
| `--since DURATION` | Show runs newer than `DURATION` (e.g., `7d`, `1h`) |
| `--detail` | Include per-run summary lines |
| `--json` | Emit machine-readable JSON |
| `--clear` | Delete this project's run logs (prompts unless `--force`) |

### Step History

A `--step` flag is accepted by `bivvy history` for forward
compatibility, but it is **not yet implemented** — the JSONL event
schema currently only stores per-step counts in the workflow summary,
not per-step records. When you pass `--step`, the command prints a
note and proceeds as if the flag were absent. Per-step history will
be added in a future release.

## Clearing State

To delete this project's run logs:

```bash
bivvy history --clear
```

To discard all persisted satisfaction records so every step is
re-evaluated from scratch on the next run:

```bash
bivvy run --fresh
```

Note that `--fresh` resets state but does not bypass `check:` blocks —
if a check still passes, the step is still skipped. To also bypass
checks and force every step in the workflow to run, combine `--fresh`
with `--force-all`:

```bash
bivvy run --fresh --force-all
```

To force only specific steps to re-run regardless of their checks:

```bash
bivvy run --force dependencies,build
```

See [Forcing Re-run](completed-checks.md#forcing-re-run) for the full
set of force directives.
