---
title: bivvy lint
description: Validate configuration files
---

# bivvy lint

Validates your Bivvy configuration without executing anything.

## Usage

```bash
bivvy lint
```

```bash
bivvy lint <name>
```

```bash
bivvy lint --workflow ci
```

```bash
bivvy lint --step bundle-install
```

```bash
bivvy lint --config-only
```

```bash
bivvy lint --all
```

```bash
bivvy lint --format=json  # default is "human" if not specified
```

```bash
bivvy lint --format=sarif
```

```bash
bivvy lint --fix
```

```bash
bivvy lint --strict
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<name>` | Optional positional. Resolves to `.bivvy/workflows/<name>.yml` first, then `.bivvy/steps/<name>.yml`. If neither exists, lint exits with an error and lists the available workflows and steps. |

## Flags

| Flag | Description |
|------|-------------|
| `--workflow <NAME>` | Force lookup as a workflow file: `.bivvy/workflows/<NAME>.yml`. |
| `--step <NAME>` | Force lookup as a step file: `.bivvy/steps/<NAME>.yml`. |
| `--config-only` | Lint `.bivvy/config.yml` only. This is the default when no target is given. Named `--config-only` rather than `--config` to avoid collision with the global `-c, --config <PATH>` option. |
| `--all` | Lint every file in the merged state — the legacy "lint everything" behavior, now opt-in. |
| `--format <FORMAT>` | Output format: `human` (default), `json`, or `sarif`. |
| `--fix` | Auto-fix simple issues. |
| `--strict` | Treat warnings as errors. |

`--workflow`, `--step`, `--config-only`, and `--all` are mutually exclusive — pass at most one.

## Scope and Load Profile

By default, `bivvy lint` validates only `.bivvy/config.yml` (project-only load, no merge with `~/.bivvy/`, split files, or `config.local.yml`). The other scoping flags change which files participate:

| Selection | What's loaded |
|-----------|---------------|
| No flags / `--config-only` | `.bivvy/config.yml` only |
| Positional `<name>` | The matching `.bivvy/workflows/<name>.yml` or `.bivvy/steps/<name>.yml`, plus the project file for context (settings, templates, custom requirements) |
| `--workflow <name>` | `.bivvy/workflows/<name>.yml` plus the project file for context |
| `--step <name>` | `.bivvy/steps/<name>.yml` plus the project file for context |
| `--all` | Full merged config — every file the loader can find, including `~/.bivvy/`, `extends:`, `.bivvy/steps/*.yml`, every `.bivvy/workflows/*.yml`, and `.bivvy/config.local.yml` |

Targeted lint (`<name>`, `--workflow`, `--step`) does not parse sibling workflow files, so a malformed neighbor cannot block linting of an unrelated workflow.

### Examples

Lint just the project config (the default):

```bash
bivvy lint
```

Lint a single workflow file:

```bash
bivvy lint --workflow ci
# or, by positional resolution:
bivvy lint ci
```

Lint a single step file:

```bash
bivvy lint --step bundle-install
```

Lint everything in the merged config (legacy behavior):

```bash
bivvy lint --all
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No errors (warnings OK) |
| 1 | Errors found |
| 2 | No configuration found |

With `--strict`, warnings also cause exit code 1.

## Example Output

### Valid Configuration

```
Configuration is valid!
```

### Invalid Configuration

```
error [default]: Workflow references unknown step 'nonexistent'

Found 1 error(s)
```

## Integration

### VS Code

Use the SARIF Viewer extension:

```bash
bivvy lint --format=sarif > bivvy.sarif
```

### GitHub Actions

```yaml
- name: Lint Bivvy config
  run: bivvy lint --format=sarif > bivvy.sarif
- uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: bivvy.sarif
```
