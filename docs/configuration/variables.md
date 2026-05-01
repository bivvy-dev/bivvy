# Variables

User-defined variables let you share values across multiple steps
without repeating yourself. Define them once under the top-level
`vars:` key, then reference them with `${var_name}` in any command.

## Static Variables

A plain string value, available for interpolation:

```yaml
vars:
  app_name: "bivvy"
  deploy_target: "production"

steps:
  greet:
    command: "echo Setting up ${app_name} for ${deploy_target}"
```

## Computed Variables

Use `command:` to run a shell command at the start of a workflow.
The command's stdout (trimmed) becomes the variable value:

```yaml
vars:
  version:
    command: "cat VERSION"
  git_sha:
    command: "git rev-parse --short HEAD"

steps:
  tag:
    command: "echo Deploying ${version} (${git_sha})"
```

Computed variables run once when the workflow starts, not once per
step. If a computed variable's command exits non-zero, the workflow
fails immediately with an error naming the variable.

## Mixing Static and Computed

```yaml
vars:
  app_name: "myapp"
  version:
    command: "grep '^version' Cargo.toml | head -1 | cut -d'\"' -f2"

steps:
  build:
    command: "docker build -t ${app_name}:${version} ."
  push:
    command: "docker push ${app_name}:${version}"
```

## Resolution Priority

When the same name exists in multiple sources, Bivvy resolves it
using this priority order (highest to lowest):

1. **Prompt values** from the current run
2. **Saved preferences** from previous runs
3. **User-defined variables** (`vars:`)
4. **Template inputs** — the resolved `inputs:` map for the
   currently-running step (only visible while that step is being
   prepared and executed)
5. **Environment variables**
6. **Built-in variables** (`project_name`, `project_root`,
   `bivvy_version`)

This means a prompt answer always wins over a `vars:` definition,
and a `vars:` definition wins over a template input or environment
variable with the same name.

## Escaping

Use `$${` to output a literal `${` without interpolation:

```yaml
steps:
  example:
    command: "echo '$${NOT_INTERPOLATED}'"  # outputs: ${NOT_INTERPOLATED}
```
