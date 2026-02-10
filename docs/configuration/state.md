# State Management

Bivvy tracks execution state to enable smart re-runs, skip completed steps, and detect when watched files have changed.

## How State Works

When you run Bivvy, it:

1. Identifies your project using a hash of the path and git remote
2. Loads any existing state from `~/.bivvy/projects/{hash}/`
3. Tracks which steps run, skip, or fail
4. Saves state after each workflow execution

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
| `watches_hash` | Hash of watched files at last run |

### Step Status Values

- **Success** - Step completed without errors
- **Failed** - Step exited with non-zero code or error
- **Skipped** - Step was skipped (already complete or user choice)
- **NeverRun** - Step has never been executed

## Run History

Each workflow execution is recorded with:

```yaml
timestamp: 2024-01-15T10:30:00Z
workflow: default
duration_ms: 45000
status: Success
steps_run:
  - dependencies
  - database
steps_skipped:
  - seeds
error: null
```

### Run Status Values

- **Success** - All steps completed successfully
- **Failed** - One or more steps failed
- **Interrupted** - Execution was cancelled (Ctrl+C)

## Change Detection

When a step has `watches` configured, Bivvy detects if those files have changed since the step last ran:

```yaml
steps:
  dependencies:
    command: bundle install
    watches:
      - Gemfile
      - Gemfile.lock
```

Change detection checks:

1. File modification time vs. step last run time
2. Content hash for files under 1MB

If watched files have changed, the step is marked as "stale" and will re-run even if previously successful.

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
| State | `~/.bivvy/projects/{hash}/state.yml` |
| Preferences | `~/.bivvy/projects/{hash}/preferences.yml` |
| Project Index | `~/.bivvy/projects/index.yml` |

## History Pruning

Bivvy automatically manages state file size:

- Default retention: 50 runs
- Orphaned step state (not in recent runs) is cleaned up
- Use `bivvy history --prune` to manually clean old history

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

Shows a table of recent runs with timestamps and outcomes.

### Step History

```bash
bivvy history --step dependencies
```

Shows execution history for a specific step across all runs.

## Clearing State

To reset state and force all steps to re-run:

```bash
bivvy run --force
```

Or clear state for a specific step:

```bash
bivvy run --force --step dependencies
```
