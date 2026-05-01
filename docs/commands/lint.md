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

```bash
bivvy lint --list-rules
```

```bash
bivvy lint --explain parse-error/unknown-field
```

```bash
bivvy lint --rule self-dependency --rule undefined-dependency
```

```bash
bivvy lint --no-rule app-name-format
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
| `--list-rules` | Print every available lint rule with its id, default severity, and human-readable name. Does not load configuration. Exits 0. |
| `--explain <RULE_ID>` | Print the rule's name, severity, and a short description. Does not load configuration. Exits 0 if the rule exists, 1 if it doesn't. |
| `--rule <ID>` | Run only this rule. Repeatable; pass it once per rule to allow. |
| `--no-rule <ID>` | Skip this rule. Repeatable; pass it once per rule to disable. Applied after `--rule` filtering. |

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

When the project file is loaded only for context (e.g. `bivvy lint --workflow ci`), it appears as a one-line trailing note rather than its own card:

```
Loaded for context (not validated): ./.bivvy/config.yml
```

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

## Output

`bivvy lint` prints one card per inspected file. Each card has a bold path
header followed by aligned label/value rows summarizing what's in the file
and how many errors were found.

### Valid Configuration

```
.bivvy/config.yml (project config)
  Steps:     8 defined, 2 referenced from workflows
  Workflows: 2 (default, release)
  Templates: 0 referenced from this file
  Errors:    0

.bivvy/workflows/release.yml (workflow file: release)
  Steps:     4 defined
  Workflow:  release (4 steps, 0 conditionals)
  Templates: 1 (rust/version-bump)
  Errors:    0
```

Path display rules:

- System config: `$HOME/.bivvy/config.yml (system config)`
- Project config: `./.bivvy/config.yml (project config)`
- Local config: `./.bivvy/config.local.yml (local config)`
- Workflow file: `./.bivvy/workflows/<name>.yml (workflow file: <name>)`
- Step file: `./.bivvy/steps/<name>.yml (step file: <name>)`
- Extends URL: `extends: <url>`

### Parse Errors

When a file fails to parse (e.g. an unknown top-level field or invalid YAML),
the diagnostic is itemized under the card's `Errors:` row in rustc/clippy
format:

```
.bivvy/config.yml (project config)
  Errors:    1

    error[parse-error/unknown-field]: unrecognized top-level key `my-settings`
      --> ./.bivvy/config.yml:31:1
       |
    31 | my-settings:
       | ^^^^^^^^^^^ expected one of: app_name, settings, template_sources,
       |             steps, workflows, secrets, extends, requirements, vars
       |
       = help: did you mean `settings`?
```

Other parse-error variants follow the same shape with their own rule ids:
`parse-error/unknown-field`, `parse-error/invalid-type`,
`parse-error/missing-field`, `parse-error/duplicate-key`, and a generic
`parse-error` fallback.

### Listing and Explaining Rules

`bivvy lint --list-rules` prints every available lint rule:

```
Bivvy Lint Rules

  ID                                  Severity  Name
  app-name-format                     warning   App Name Format
  circular-dependency                 error     Circular Dependency
  ...
```

`bivvy lint --explain <RULE_ID>` shows the rule's description:

```
parse-error/unknown-field

  Severity:    error
  Name:        Unrecognized top-level field
  Description: A field appears at the top level of a config file
               that the schema doesn't recognize. Often a typo
               (e.g. `workflow:` for `workflows:`) or a stale
               field name. The diagnostic includes a "did you
               mean?" suggestion when one exists.
```

If a rule isn't found:

```
error[explain]: no such rule: '<RULE_ID>'
   = help: run `bivvy lint --list-rules` to see all available rules
```

Exits 1 in that case.

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
