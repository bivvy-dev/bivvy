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

## Flags

| Flag | Description |
|------|-------------|
| `--steps-only` | Show only the steps section |
| `--workflows-only` | Show only the workflows section |
| `--json` | Output as JSON instead of styled text |
| `--env <ENV>` | Target environment (e.g., `development`, `ci`, `staging`) |

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

When using `--json`, output is a JSON object with `environment`, `steps`, and `workflows` fields. The `--steps-only` and `--workflows-only` flags control which sections are included.

```json
{
  "environment": "development",
  "steps": [
    {
      "name": "install",
      "template": "yarn-install"
    },
    {
      "name": "build",
      "command": "npm run build",
      "description": "Compiles the project",
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
