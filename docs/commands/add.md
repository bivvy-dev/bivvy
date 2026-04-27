---
title: bivvy add
description: Add a template step to configuration
---

# bivvy add

Adds a template step to your existing `.bivvy/config.yml` configuration file.

## Usage

```bash
bivvy add <template>
```

```bash
bivvy add bundle-install --as ruby_deps
```

```bash
bivvy add yarn-install --after bundle-install
```

```bash
bivvy add pre-commit --no-workflow
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<template>` | Name of the template to add (required) |

## Options

| Option | Description |
|--------|-------------|
| `--as <NAME>` | Step name to use in config (defaults to template name) |
| `--workflow <NAME>` | Workflow to add the step to (defaults to `default`) |
| `--after <STEP>` | Insert after this step in the workflow |
| `--no-workflow` | Don't add the step to any workflow |

## What It Does

1. Validates the template exists in the registry
2. Validates a config file exists (run `bivvy init` first if not)
3. Checks the step name doesn't already exist
4. Appends the new step to the end of the `steps:` section
5. Adds the step to the specified workflow's step list
6. Preserves all existing comments and formatting

The generated step block matches the format from `bivvy init` — a template reference with commented-out details:

```yaml
steps:
  # ... existing steps ...

  bundle-install:
    template: bundle-install
    # command: bundle install
    # check:
    #   type: execution
    #   command: "bundle check"
    #   validation: success
```

## Examples

### Add a template with the default step name

```bash
bivvy add bundle-install
```

This creates a step named `bundle-install` using the `bundle-install` template and adds it to the end of the `default` workflow.

### Add with a custom step name

```bash
bivvy add bundle-install --as ruby_deps
```

Creates a step named `ruby_deps` that uses the `bundle-install` template. Useful when you want a more descriptive name, or when you already have a step with the template's default name.

### Insert at a specific position

```bash
bivvy add yarn-install --after bundle-install
```

Adds the `yarn-install` step to the `default` workflow immediately after `bundle-install`, rather than at the end.

### Add to a specific workflow

```bash
bivvy add rails-db --workflow ci
```

Adds the `rails-db` step to the `ci` workflow instead of `default`.

### Add without a workflow

```bash
bivvy add pre-commit --no-workflow
```

Adds the step to the config but doesn't add it to any workflow. Useful for steps you'll reference in custom workflows later.

### Discover templates first

```bash
# See what's available
bivvy templates

# Filter to a category
bivvy templates --category ruby

# Add one
bivvy add rails-db
```

## Error Cases

| Error | Cause |
|-------|-------|
| "No configuration found" | No `.bivvy/config.yml` exists — run `bivvy init` first |
| "Unknown template" | The template name doesn't match any built-in, local, or remote template |
| "Step already exists" | A step with that name already exists — use `--as` to pick a different name |

## See Also

- [`bivvy templates`](./templates.md) — Browse available templates
- [`bivvy init`](./init.md) — Initialize configuration from scratch
- [`bivvy list`](./list.md) — List configured steps and workflows
- [Templates Overview](../templates/index.md) — How the template system works
