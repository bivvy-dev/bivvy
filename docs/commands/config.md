---
title: bivvy config
description: Show resolved configuration
---

# bivvy config

Displays the project configuration. By default, shows only the project-level config (`.bivvy/config.yml`). Use `--merged` to see the fully resolved configuration after merging all sources.

## Usage

```bash
# Show project config only
bivvy config

# Show fully merged config (project + system + local)
bivvy config --merged

# Output as JSON
bivvy config --json

# Explicitly output as YAML (default format)
bivvy config --yaml

# Merged config in JSON format
bivvy config --merged --json
```

## Flags

| Flag | Description |
|------|-------------|
| `--merged` | Show the fully merged config from all sources (extends, global, project, local) instead of just the project config |
| `--json` | Output in JSON format |
| `--yaml` | Output in YAML format (this is the default) |

## Default Behavior (without --merged)

Without `--merged`, `bivvy config` shows only the project-level configuration file (`.bivvy/config.yml`). This is useful for seeing exactly what is defined in the current project without any system-level or local overrides applied.

## Merged Configuration (--merged)

With `--merged`, configuration is merged from all sources in this order:

1. `extends:` (remote base config)
2. `~/.bivvy/config.yml` (user global)
3. `.bivvy/config.yml` (project)
4. `.bivvy/config.local.yml` (local overrides)

Later sources override earlier ones. The output header lists all config files that were merged.

## Example Output

```bash
bivvy config
```

```yaml
# /path/to/project/.bivvy/config.yml

app_name: "MyApp"

settings:
  defaults:
    output: verbose

steps:
  brew:
    template: brew-bundle
  mise:
    template: mise-tools
    depends_on: [brew]
  ruby_deps:
    template: bundle-install
    depends_on: [mise]

workflows:
  default:
    steps: [brew, mise, ruby_deps]
```

## JSON Output

```bash
bivvy config --json
```

```json
{
  "app_name": "MyApp",
  "steps": {
    "brew": {
      "template": "brew"
    }
  },
  "workflows": {
    "default": {
      "steps": ["brew", "mise", "ruby_deps"]
    }
  }
}
```
