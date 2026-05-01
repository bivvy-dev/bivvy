---
title: bivvy list
description: List steps and workflows
---

# bivvy list

Lists all available steps and workflows in your configuration.

## Usage

```bash
bivvy list
```

```bash
bivvy list <workflow>
```

```bash
bivvy list --all
```

```bash
bivvy list --steps-only
```

```bash
bivvy list --workflows-only
```

```bash
bivvy list --json
```

```bash
bivvy list --env ci
```

```bash
bivvy list <workflow-name>
```

```bash
bivvy list --all
```

## Example Output

```
  Environment: development (fallback)

  Steps:
    install (template: yarn-install)
    build — npm run build
      Compiles the project
      └── depends on: install
    deploy — bin/deploy
      └── depends on: install, build

  Workflows:
    default: install → build → deploy
      Full development setup
```

When using `--env ci`, steps restricted to other environments are shown as skipped:

```
  Environment: ci (--env flag)

  Steps:
    dev_only (skipped in ci)
    always_run — echo always
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<workflow>` | Optional. When given, only that workflow is parsed (from `.bivvy/workflows/<name>.yml`) and shown alongside its bundled steps. Without a target, Bivvy lists everything from discovery + headers without deep-merging. |

## Flags

| Flag | Description |
|------|-------------|
| `--all` | Show every step and workflow from the fully merged configuration (legacy behavior). Without this flag, output is built from filesystem discovery and lightweight workflow headers, and does not deep-merge. |
| `--steps-only` | Show only the steps section |
| `--workflows-only` | Show only the workflows section |
| `--json` | Output as JSON instead of styled text |
| `--env <ENV>` | Target environment (e.g., `development`, `ci`, `staging`) |

## Discovery vs Full Merge

`bivvy list` defaults to a fast **discovery-based** view of your configuration:

- Step names come from `.bivvy/config.yml` plus the filename stems under `.bivvy/steps/`.
- Workflow names come from `.bivvy/config.yml` plus the filenames under `.bivvy/workflows/`. Each workflow file's `description` and step list are read from a lightweight header — the file is not fully parsed against the schema.
- `~/.bivvy/`, remote `extends:` URLs, and `.bivvy/config.local.yml` are not loaded.

Pass `--all` to opt into the full merged view: every file the loader can find is parsed and merged together, exactly like `bivvy run` sees it. Use this when you want to inspect the final merged result, including overrides from `config.local.yml` or user-global config.

Passing a positional `<workflow>` switches to a per-workflow detail view: the project file plus the named workflow file are loaded together, and other workflow files are skipped.

## Output Format

### Steps

Each step shows:
- Step name with template reference (e.g., `step_name (template: xxx)`) or inline command (e.g., `step_name — command_text`)
- Description or title on an indented line below
- Dependencies shown as `└── depends on: dep1, dep2` on an indented line
- Environment constraints: steps restricted to other environments appear as `step_name (skipped in <env>)`

### Workflows

Each workflow shows:
- Workflow name followed by step names joined with Unicode arrows: `workflow_name: step1 → step2 → step3`
- Description on an indented line below (if present)

### JSON Output

When using `--json`, output is a JSON object with `environment`, `steps`, and `workflows` fields. The `--steps-only` and `--workflows-only` flags control which sections are included. Steps that are excluded by the active environment include `"skipped": true`.

```json
{
  "environment": "development",
  "steps": [
    {
      "name": "ci_only",
      "command": "echo ci",
      "title": "CI only",
      "skipped": true
    },
    {
      "name": "install",
      "template": "yarn-install"
    },
    {
      "name": "build",
      "command": "npm run build",
      "title": "Compiles the project",
      "depends_on": ["install"]
    }
  ],
  "workflows": [
    {
      "name": "default",
      "steps": ["install", "build"],
      "description": "Full development setup"
    }
  ]
}
```
