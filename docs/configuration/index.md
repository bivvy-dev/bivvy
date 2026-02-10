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
  default_output: verbose  # verbose | quiet | silent

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

### Built-in Variables

| Variable | Description |
|----------|-------------|
| `${project_name}` | Directory name of the project |
| `${project_root}` | Absolute path to project root |

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
- [Workflows Configuration](workflows.md)
- [Completed Checks](completed-checks.md)
