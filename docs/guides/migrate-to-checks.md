---
title: Migrate to the New Check Schema
description: Convert legacy completed_check, watches, and prompt_if_complete fields to the modern check schema
---

# Migrate to the New Check Schema

Older Bivvy configs used a handful of separate fields to express "is this step
already done?" -- `completed_check:` (with subtypes like `file_exists`,
`command_succeeds`, and `marker`), a top-level `watches:` list, and the
boolean `prompt_if_complete:`. These have been replaced by a single, uniform
`check:` / `checks:` model that supports `presence`, `execution`, and `change`
types, the `all` / `any` combinators, and explicit `satisfied_when` conditions.
The new schema composes cleanly across steps and gives Bivvy enough
information to make smarter skip and rerun decisions.

If you still have the old fields in your config, Bivvy continues to load them
where possible (with deprecation warnings), but they will be removed in a
future release. Use the table below to update your config now.

## Field-by-Field Migration

### `completed_check:` with a command → `check: { type: execution }`

**Before:**

```yaml
steps:
  db:
    command: "rails db:create"
    completed_check:
      type: command_succeeds
      command: "rails db:version"
```

**After:**

```yaml
steps:
  db:
    command: "rails db:create"
    check:
      type: execution
      command: "rails db:version"
```

### `completed_check:` for a file/marker → `check: { type: presence }`

A `completed_check` of `type: file_exists` (or the catch-all `type: marker`,
which was used to point at a marker file) becomes a `presence` check:

**Before:**

```yaml
steps:
  deps:
    command: "yarn install"
    completed_check:
      type: file_exists
      path: "node_modules"
```

**After:**

```yaml
steps:
  deps:
    command: "yarn install"
    check:
      type: presence
      target: "node_modules"
```

`type: marker` had no specific behavior of its own beyond "this file's
existence means the step is done" -- migrate it the same way as
`file_exists`, pointing `target:` at the marker path. If the marker file
served no real purpose, drop the check entirely.

### `watches:` paths → `check: { type: change }`

The top-level `watches:` list has been replaced by `change` checks, which
hash the target and compare against a stored baseline. A list of watched
paths becomes one `change` check per path (combined under `checks:` for
multiple targets):

**Before:**

```yaml
steps:
  deps:
    command: "bundle install"
    watches:
      - Gemfile
      - Gemfile.lock
```

**After:**

```yaml
steps:
  deps:
    command: "bundle install"
    checks:
      - type: change
        target: "Gemfile"
        on_change: proceed
      - type: change
        target: "Gemfile.lock"
        on_change: proceed
```

For a single watched path, use `check:` instead of `checks:`. See
[Change checks](../configuration/completed-checks.md#change) for `on_change`,
`baseline`, and glob/command target kinds.

### `prompt_if_complete:` → `prompt_on_rerun:`

Pure rename. The default also changed: the old `prompt_if_complete` defaulted
to `true` (always ask before re-running a satisfied step), while
`prompt_on_rerun` defaults to `false` (silently skip satisfied steps). If you
relied on the old default, set `prompt_on_rerun: true` explicitly.

**Before:**

```yaml
steps:
  deploy:
    command: "./scripts/deploy.sh"
    prompt_if_complete: true
```

**After:**

```yaml
steps:
  deploy:
    command: "./scripts/deploy.sh"
    prompt_on_rerun: true
```

## Quick Reference

| Legacy field                        | Modern replacement                              |
| ----------------------------------- | ----------------------------------------------- |
| `completed_check: { type: command_succeeds, command: ... }` | `check: { type: execution, command: ... }` |
| `completed_check: { type: file_exists, path: ... }`         | `check: { type: presence, target: ... }`   |
| `completed_check: { type: marker, ... }`                    | `check: { type: presence, target: ... }` (or remove) |
| `watches: [path, ...]`              | `checks: [{ type: change, target: path, on_change: proceed }, ...]` |
| `prompt_if_complete: true`          | `prompt_on_rerun: true`                         |
| `prompt_if_complete: false`         | `prompt_on_rerun: false` (now the default)      |

## Find What Needs Migrating

Run `bivvy lint` to surface every legacy field in your config along with the
file and line number:

```bash
bivvy lint
```

You will see warnings like:

```
.bivvy/config.yml (line 12): 'completed_check' is deprecated, use 'check' or 'checks' instead.
.bivvy/config.yml (line 19): 'watches' is deprecated. Use 'check: { type: change, target: ... }' instead.
.bivvy/config.yml (line 26): 'prompt_if_complete' is deprecated, use 'prompt_on_rerun' instead.
```

The same warnings appear at the top of `bivvy run` output. Apply the
mappings from the table above to each warning, then re-run `bivvy lint` to
confirm a clean config.

## See Also

- [Checks](../configuration/completed-checks.md) -- the canonical reference
  for `presence`, `execution`, `change`, combinators, and `satisfied_when`.
- [Auto-Run and the Decision Engine](auto-run.md) -- how `prompt_on_rerun`
  fits into Bivvy's skip/run/prompt decisions.
