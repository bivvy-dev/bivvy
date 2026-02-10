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

## Example Output

```
Steps:
  hello (template: greet) ->
  world -> hello

Workflows:
  default: [hello, world]
  ci: [hello, world, ...]
```

## Output Format

### Steps

Each step shows:
- Step name
- Template (if using a template)
- Dependencies (steps it depends on)

### Workflows

Each workflow shows:
- Workflow name
- List of steps (truncated if more than 5)
