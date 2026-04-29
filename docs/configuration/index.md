# Configuration

Bivvy uses YAML configuration files to define your project setup.

## File Locations

Configuration is loaded and merged in this order (later overrides earlier):

1. Remote base configs (from `extends:`)
2. User global config (`~/.bivvy/config.yml`)
3. Project config (`.bivvy/config.yml`)
4. Local overrides (`.bivvy/config.local.yml`) - gitignored

## Basic Structure

```yaml
app_name: "MyApp"

settings:
  defaults:
    output: verbose  # verbose | quiet | silent

steps:
  install_deps:
    command: "npm install"

workflows:
  default:
    steps: [install_deps]
```

## Variable Interpolation

Use `${VAR}` syntax to interpolate values:

```yaml
steps:
  setup:
    command: "echo Setting up ${project_name}"
```

### Resolution Priority

Variables are resolved in this order (highest to lowest):

1. Prompt values from current run
2. Saved preferences from previous runs
3. User-defined variables (`vars:`)
4. Environment variables
5. Built-in variables

### Built-in Variables

| Variable | Description |
|----------|-------------|
| `${project_name}` | Directory name of the project |
| `${project_root}` | Absolute path to project root |
| `${bivvy_version}` | Current Bivvy version |

### User-Defined Variables

Define reusable values — static strings or shell commands — under
the top-level `vars:` key. See [Variables](variables.md) for details.

```yaml
vars:
  version:
    command: "cat VERSION"

steps:
  tag:
    command: "git tag v${version}"
```

### Environment Variables

Environment variables are available for interpolation:

```yaml
steps:
  deploy:
    command: "deploy --env ${RAILS_ENV}"
```

### Escaping

Use `$${` to escape (outputs literal `${`):

```yaml
steps:
  example:
    command: "echo '$${NOT_INTERPOLATED}'"  # outputs: ${NOT_INTERPOLATED}
```

## Deep Merge Behavior

When multiple config files define the same key:

- **Objects**: Recursively merged (nested keys combined)
- **Arrays**: Replaced entirely (not concatenated)
- **Scalars**: Later value wins

```yaml
# .bivvy/config.yml
steps:
  deps:
    command: "yarn install"
    env:
      NODE_ENV: development

# .bivvy/config.local.yml
steps:
  deps:
    env:
      NODE_ENV: production
      # command is preserved from base
```

## Next Steps

- [Steps Configuration](steps.md)
- [Variables](variables.md)
- [Workflows Configuration](workflows.md)
- [Completed Checks](completed-checks.md)
