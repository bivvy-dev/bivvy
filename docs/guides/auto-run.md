---
title: Auto-Run and the Decision Engine
description: How Bivvy decides whether to run, skip, or prompt for each step
---

# Auto-Run and the Decision Engine

When you run `bivvy` or `bivvy run`, the decision engine evaluates every step
in the workflow before executing it. For each step, it decides one of four
outcomes:

| Decision | What happens |
|----------|-------------|
| **Auto-run** | The step executes immediately, no prompt |
| **Skip** | The step is skipped (already satisfied) |
| **Prompt** | You are asked whether to run or skip |
| **Block** | The step cannot proceed (dependency failed or precondition failed) |

When a step is blocked, Bivvy names the specific dependency or precondition
that caused it, so you can see exactly what to fix. For example:

```
Blocked (dependency 'release-preflight' failed)
Blocked (precondition failed: git branch is main)
```

This page explains how those decisions are made and how to configure them.

## How satisfaction is determined

The decision engine checks whether a step is already "satisfied" -- meaning
its work has already been done. It evaluates three signals, in order. The
first match wins:

1. **`satisfied_when` conditions** -- If the step defines explicit
   satisfaction conditions, they are evaluated first. If all conditions pass,
   the step is satisfied. If any condition fails, the step is _not_ satisfied
   and the remaining signals are not checked.

2. **`check` / `checks`** -- If the step has a completed check and it passes,
   the step is satisfied.

3. **Rerun window** -- If the step ran successfully within its rerun window,
   the step is satisfied based on execution history.

If none of these signals match, the step is not satisfied and needs to run.

## The decision flow

Once satisfaction is determined, the decision engine follows this logic:

1. **Dependencies** -- If a dependency failed or was skipped (and is not
   satisfied), the step is **blocked**.
2. **Precondition** -- If the step has a precondition and it fails, the step
   is **blocked**.
3. **Satisfied** -- If the step is satisfied:
   - With `prompt_on_rerun: false` (default): the step is **skipped** silently.
   - With `prompt_on_rerun: true`: you are **prompted** to re-run
     or skip.
4. **Not satisfied** -- If the step needs to run:
   - With `auto_run: true` (default): the step **auto-runs**.
   - With `auto_run: false`: you are **prompted** before execution.

## Configuration

### `auto_run`

Controls whether unsatisfied steps run automatically or prompt first.

```yaml
steps:
  install_deps:
    command: npm install
    auto_run: true  # default -- runs without asking
```

```yaml
steps:
  deploy:
    command: ./scripts/deploy.sh
    auto_run: false  # always ask before running
```

Set the global default under `settings.defaults`:

```yaml
settings:
  defaults:
    auto_run: true  # default for all steps
```

### `prompt_on_rerun`

Controls what happens when a step is already satisfied. When `false` (the
default), satisfied steps are silently skipped. When `true`, Bivvy asks
if you want to re-run it.

```yaml
steps:
  deploy:
    command: ./scripts/deploy.sh
    prompt_on_rerun: true  # ask before re-running
```

Set the global default:

```yaml
settings:
  defaults:
    prompt_on_rerun: false  # default for all steps
```

### `confirm`

Some steps are destructive enough that you always want an explicit
confirmation before they run, even when `auto_run` would otherwise let them
proceed silently. Set `confirm: true` to require a yes/no confirmation
before execution:

```yaml
steps:
  database_reset:
    command: rails db:reset
    confirm: true   # always ask before running this step
```

`confirm` does not change satisfaction. If the step is satisfied it is still
skipped (or prompted on rerun, depending on `prompt_on_rerun`); the
confirmation only fires when Bivvy would actually execute the command.

### `rerun_window`

How long a previous successful run counts as "recent enough" to consider a
step satisfied by execution history alone. This is the third signal in the
satisfaction hierarchy -- it only applies when the step has no
`satisfied_when` conditions and no passing `check`.

The default is `4h` (4 hours).

```yaml
steps:
  install_deps:
    command: npm install
    rerun_window: "24h"  # trust a successful run for 24 hours

  compile_assets:
    command: npm run build
    rerun_window: "never"  # always re-run, ignore history

  one_time_setup:
    command: ./scripts/initial-setup.sh
    rerun_window: "forever"  # once it succeeds, never re-run
```

Accepted values:

| Value | Meaning |
|-------|---------|
| `"30m"` | 30 minutes |
| `"4h"` | 4 hours (default) |
| `"7d"` | 7 days |
| `"never"` or `"0"` | Execution history never satisfies the step |
| `"forever"` | A previous success always satisfies the step |

Set the global default under `settings.defaults`:

```yaml
settings:
  defaults:
    rerun_window: "8h"
```

You can also set the global default with the top-level
`default_rerun_window` key (equivalent):

```yaml
settings:
  default_rerun_window: "8h"
```

Both forms live directly under `settings:`. Bivvy's settings schema is flat
-- there is no nested `settings.execution:` block, and wrapping these keys
in one will fail validation with an "unknown field" error.

### `satisfied_when`

Explicit conditions that declare a step's purpose is fulfilled. When
present, these take priority over `check` and the rerun window.

```yaml
steps:
  install_deps:
    command: npm install
    satisfied_when:
      - type: presence
        target: node_modules
        kind: file
```

You can reference named checks from the same step or other steps:

```yaml
steps:
  install_deps:
    command: npm install
    check:
      name: deps_installed
      type: presence
      target: node_modules
    satisfied_when:
      - ref: deps_installed          # same-step reference
      - ref: build.output_exists     # cross-step reference
```

All conditions must pass for the step to be considered satisfied.

## Workflow-level overrides

Workflows can override `auto_run` for all their steps, and individual step
overrides can fine-tune behavior further:

```yaml
workflows:
  ci:
    steps: [install_deps, test, lint]
    auto_run_steps: true  # auto-run everything in CI

    overrides:
      install_deps:
        auto_run: false          # except this one -- prompt first
        prompt_on_rerun: false   # and skip silently if satisfied
        rerun_window: "1h"       # with a shorter rerun window
```

Override precedence (highest to lowest):

1. Workflow step override (`workflows.<name>.overrides.<step>`)
2. Step-level setting (`steps.<name>.auto_run`)
3. Global default (`settings.defaults.auto_run`)

## Forcing a re-run from the CLI

Sometimes you want to run a step (or every step) regardless of what the
decision engine thinks. Bivvy exposes three CLI flags for this:

| Flag | Effect |
|------|--------|
| `--force <step>[,<step>...]` | Force-run the named steps, bypassing satisfaction checks. Other steps follow the normal decision flow. |
| `--force-all` | Force-run every step in the workflow, bypassing all checks and step-level configuration. |
| `--fresh` | Discard all persisted satisfaction records and re-evaluate every step from scratch. |

Examples:

```bash
# Re-run a single step even if it looks satisfied
bivvy run --force install_deps

# Re-run several steps in one go
bivvy run --force install_deps,build

# Re-run the whole workflow, ignoring checks and step config
bivvy run --force-all

# Throw away cached satisfaction state, then evaluate normally
bivvy run --fresh
```

`--force` / `--force-all` always run the listed steps. `--fresh` clears
prior state but still respects `check`, `satisfied_when`, and `auto_run`
once it re-evaluates -- so a step whose `check` passes will still be
considered satisfied after `--fresh`.

## Examples

### Fast CI workflow

Auto-run everything, skip satisfied steps silently, trust recent runs for
a short window:

```yaml
settings:
  defaults:
    auto_run: true
    prompt_on_rerun: false
    rerun_window: "30m"
```

### Cautious first-time setup

Prompt before every step so new team members can follow along:

```yaml
settings:
  defaults:
    auto_run: false

steps:
  install_deps:
    command: npm install
    title: Install dependencies
    check:
      type: presence
      target: node_modules
```

### Mixed approach

Auto-run safe steps, prompt for dangerous ones:

```yaml
steps:
  install_deps:
    command: npm install
    # auto_run defaults to true

  database_reset:
    command: rails db:reset
    auto_run: false    # always ask first
    confirm: true      # and require explicit confirmation
    rerun_window: "never"  # never skip based on history
```
