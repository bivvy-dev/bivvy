# Completed Checks

Completed checks determine if a step has already run successfully. When a check passes, Bivvy skips the step (unless `--force` is used).

## Check Types

### file_exists

Check if a file or directory exists:

```yaml
steps:
  deps:
    command: "yarn install"
    completed_check:
      type: file_exists
      path: "node_modules"
```

### command_succeeds

Check if a command exits with status 0:

```yaml
steps:
  db:
    command: "rails db:create"
    completed_check:
      type: command_succeeds
      command: "rails db:version"
```

### marker

Check for a marker in Bivvy's state (set automatically after step runs):

```yaml
steps:
  one_time_setup:
    command: "./scripts/migrate-legacy-data.sh"
    completed_check:
      type: marker
```

## Combinators

### all

All checks must pass:

```yaml
completed_check:
  type: all
  checks:
    - type: file_exists
      path: "node_modules"
    - type: file_exists
      path: "yarn.lock"
```

### any

At least one check must pass:

```yaml
completed_check:
  type: any
  checks:
    - type: file_exists
      path: "node_modules"
    - type: file_exists
      path: "vendor/bundle"
```

## Forcing Re-run

Skip the completed check and always run:

```bash
bivvy --force deps
```

```bash
bivvy --force-all
```

## Behavior Options

```yaml
steps:
  deps:
    command: "yarn install"
    completed_check:
      type: file_exists
      path: "node_modules"
    prompt_if_complete: true   # Ask before re-running (default: true)
    skippable: true            # Allow skipping (default: true)
```
