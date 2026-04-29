# Checks

Checks report facts about the external world: "does this file exist?", "does this command succeed?", "has this target changed?" When a check passes, Bivvy skips the step (unless `--force` is used). Use `check:` for a single check or `checks:` for multiple (treated as an implicit `all`).

Checks can also be used in [`satisfied_when`](#satisfied_when) conditions, which declare when a step's purpose is already fulfilled.

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

When `kind` is omitted, Bivvy infers from context: targets containing `/`, `.`, or `~` are treated as files; simple names like `rustc` or `node` are treated as binaries.

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
- `falsy` -- command exits 0 with empty stdout, or exits non-zero

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

### Baselines

Change checks compare the current hash against a stored baseline. By default the baseline updates after each successful run (`each_run`). Use `first_run` to freeze the baseline after the first evaluation:

```yaml
steps:
  check_runtime:
    command: "asdf install"
    check:
      type: change
      target: "ruby --version"
      kind: command
      on_change: fail
      baseline: first_run
```

You can also compare against a named snapshot or a git ref instead of the run-based baseline:

```yaml
check:
  type: change
  target: "schema.rb"
  on_change: fail
  baseline_snapshot: v1.0     # compare against named snapshot "v1.0"

check:
  type: change
  target: "schema.rb"
  on_change: fail
  baseline_git: main          # compare against content at git ref "main"
```

### Size Limits

Change checks refuse to hash targets larger than 50 MB by default. Override with `size_limit` (in bytes) or set to `null` for no limit:

```yaml
check:
  type: change
  target: "data/fixtures.sql"
  on_change: proceed
  size_limit: 104857600   # 100 MB
```

### Snapshot Scope

By default, change check baselines are shared across all workflows in the project. Use `scope: workflow` to isolate baselines per workflow:

```yaml
check:
  type: change
  target: "Gemfile.lock"
  on_change: proceed
  scope: workflow
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

Bypass `check:` and `satisfied_when` evaluation for one or more steps.
Preconditions are not bypassed by either flag.

Force specific steps (comma-separated):

```bash
bivvy run --force deps,build
```

Force every step in the workflow:

```bash
bivvy run --force-all
```

`--force-all` also overrides any step-level configuration that would
otherwise prompt or skip — it means "run it all, don't bother me about
it." `--force` and `--force-all` can be passed together; `--force-all`
is a superset.

## Named Checks

Any check can have a `name` field. Named checks can be referenced from `satisfied_when` conditions on the same step or on other steps:

```yaml
steps:
  install_deps:
    command: "yarn install"
    check:
      type: presence
      name: deps_installed
      target: "node_modules"
```

See [`satisfied_when`](#satisfied_when) below for how to reference named checks.

## `satisfied_when`

`satisfied_when` declares when a step's purpose is already fulfilled. It is a list of conditions that must **all** pass for the step to be considered satisfied. Unlike `check`, which reports whether the step's side effects already exist, `satisfied_when` lets you compose conditions from multiple sources, including checks defined on other steps.

When `satisfied_when` is present and all conditions pass, the step is skipped. When `satisfied_when` is present but any condition fails, the step runs -- even if the step's own `check` would have passed. In other words, `satisfied_when` takes priority over `check`.

### Inline conditions

Each condition can be an inline check definition (same syntax as `check`):

```yaml
steps:
  install_deps:
    command: "yarn install"
    satisfied_when:
      - type: presence
        target: "node_modules"
      - type: execution
        command: "yarn check --integrity"
```

### Referencing named checks

Use `ref` to reference a named check. Unqualified names refer to the same step; use `step_name.check_name` to reference a check on a different step:

```yaml
steps:
  install_deps:
    command: "yarn install"
    check:
      type: presence
      name: deps_present
      target: "node_modules"

  build:
    command: "yarn build"
    depends_on: [install_deps]
    satisfied_when:
      - ref: install_deps.deps_present
      - type: presence
        target: "dist"
```

In this example, the `build` step is satisfied only when both `install_deps`'s `deps_present` check passes **and** the `dist` directory exists.

### Mixing refs and inline checks

You can freely mix `ref` entries and inline check definitions in the same `satisfied_when` list:

```yaml
satisfied_when:
  - ref: deps_installed                     # same-step named check
  - ref: install_deps.deps_present          # cross-step named check
  - type: execution                         # inline check
    command: "yarn check --integrity"
```

### How satisfaction is evaluated

Bivvy evaluates satisfaction in priority order (first match wins):

1. **`satisfied_when`** -- if present and all conditions pass, the step is satisfied. If present and any condition fails, the step is **not** satisfied (no fallthrough to `check` or history).
2. **`check`/`checks`** -- if the step's check passes, the step is satisfied.
3. **Execution history** -- if the step ran successfully within its rerun window, the step is satisfied.

If none of these apply, the step needs to run.

## Behavior Options

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: presence
      target: "node_modules"
    prompt_on_rerun: false     # Skip silently if satisfied (default: false)
    skippable: true            # Allow skipping (default: true)
```
