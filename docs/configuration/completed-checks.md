# Checks

Checks determine whether a step has already been satisfied. When a check passes, Bivvy skips the step (unless `--force` is used). Use `check:` for a single check or `checks:` for multiple (treated as an implicit `all`).

## Check Types

### Presence

Confirms that a file, directory, or binary exists:

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: presence
      target: "node_modules"
```

Use `kind: binary` to check for a binary on `$PATH`:

```yaml
steps:
  install_rustup:
    command: "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    check:
      type: presence
      target: "rustup"
      kind: binary
```

Use `kind: custom` with a `command:` for more complex presence detection:

```yaml
steps:
  docker_running:
    command: "open -a Docker"
    check:
      type: presence
      kind: custom
      command: "docker info"
```

When `kind` is omitted, Bivvy infers from context (`file` by default, `binary` if no path separators are present).

### Execution

Runs a command and validates the result:

```yaml
steps:
  db:
    command: "rails db:create"
    check:
      type: execution
      command: "rails db:version"
```

The `validation` field controls how the result is interpreted:

- `success` (default) -- command exits with status 0
- `truthy` -- command exits 0 and produces non-empty stdout
- `falsy` -- command exits non-zero (useful for "not yet installed" checks)

```yaml
steps:
  seed:
    command: "rails db:seed"
    check:
      type: execution
      command: "rails runner 'exit(User.any? ? 0 : 1)'"
      validation: truthy
```

### Change

Detects whether a target has changed since the last run. This replaces the old `watches` field with a richer model:

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: change
      target: "yarn.lock"
      on_change: proceed
```

The `on_change` field controls what happens when a change is detected:

- `proceed` (default) -- the step runs (change means "work needed")
- `fail` -- the step fails immediately (change means "unexpected drift")
- `require` -- flags another step as required via `require_step`

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: change
      target: "yarn.lock"
      on_change: require
      require_step: deps
```

Change checks support different target kinds:

- `file` (default) -- hash a single file
- `glob` -- hash all files matching a pattern
- `command` -- hash the output of a command

```yaml
steps:
  codegen:
    command: "cargo build --build-script"
    check:
      type: change
      target: "src/**/*.proto"
      kind: glob
      on_change: proceed
```

## Combinators

### all

All checks must pass (this is the default when using `checks:`):

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: all
      checks:
        - type: presence
          target: "node_modules"
        - type: execution
          command: "yarn check --integrity"
```

Using `checks:` directly on the step is equivalent to wrapping in `all`:

```yaml
steps:
  deps:
    command: "yarn install"
    checks:
      - type: presence
        target: "node_modules"
      - type: execution
        command: "yarn check --integrity"
```

### any

At least one check must pass:

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: any
      checks:
        - type: presence
          target: "node_modules"
        - type: presence
          target: "vendor/bundle"
```

## Forcing Re-run

Skip checks and always run:

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
    check:
      type: presence
      target: "node_modules"
    prompt_on_rerun: true      # Ask before re-running (default: true)
    skippable: true            # Allow skipping (default: true)
```
