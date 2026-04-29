---
title: bivvy snapshot
description: Manage named snapshots for change check baselines
---

# bivvy snapshot

Capture, list, and delete named snapshots used as baselines for change checks.

## Usage

```bash
bivvy snapshot <slug>
```

```bash
bivvy snapshot <slug> --step <name>
```

```bash
bivvy snapshot <slug> --workflow <name>
```

```bash
bivvy snapshot list
```

```bash
bivvy snapshot delete <slug>
```

## What It Does

When you configure a step with a `change` check, Bivvy compares the current hash of a target (file, directory, or glob) against a stored baseline to decide whether the step needs to re-run. By default, baselines are updated automatically after each successful run.

The `snapshot` command lets you capture a **named** baseline at a specific point in time. You can then reference that snapshot in your config so a step re-runs only when its target has changed relative to that fixed point -- not relative to the last run.

This is useful for:

- **Release baselines** -- snapshot your lock files after a release so you can detect drift from the known-good state.
- **Branch comparisons** -- capture a baseline on `main` and compare against it on feature branches.
- **Shared team baselines** -- establish a named checkpoint that everyone compares against.

## Subcommands

### Capture (default)

```bash
bivvy snapshot <slug>
```

Hashes every change check target in the default workflow and stores the result under the given name. If no default workflow exists, all steps are included.

| Option | Description |
|--------|-------------|
| `--step <name>` | Capture only for a specific step |
| `--workflow <name>` | Capture only for steps in the given workflow |

### list

```bash
bivvy snapshot list
```

Lists all named snapshots for the current project, showing the slug, step, target, and capture timestamp.

### delete

```bash
bivvy snapshot delete <slug>
```

Deletes all baseline entries stored under the given snapshot name.

## Referencing Snapshots in Config

To compare a change check against a named snapshot instead of the last run, set `baseline_snapshot` on the check:

```yaml
steps:
  bundle_install:
    command: bundle install
    check:
      type: change
      target: Gemfile.lock
      on_change: require
      baseline_snapshot: v1.0
```

With this config, `bundle_install` re-runs only when `Gemfile.lock` differs from the hash captured in the `v1.0` snapshot -- regardless of how many times `bivvy run` has executed since.

## Examples

Capture a snapshot of all change check targets:

```bash
bivvy snapshot v1.0
```

Capture only for the `bundle_install` step:

```bash
bivvy snapshot v1.0 --step bundle_install
```

Capture for all steps in the `ci` workflow:

```bash
bivvy snapshot post-deploy --workflow ci
```

List existing snapshots:

```bash
bivvy snapshot list
```

Delete a snapshot you no longer need:

```bash
bivvy snapshot delete v1.0
```

## Storage

Snapshots are stored per-project in `~/.bivvy/projects/{project_hash}/snapshots/`. They persist across runs and are not checked into version control.

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `2` | Missing snapshot name (no slug or subcommand provided) |
