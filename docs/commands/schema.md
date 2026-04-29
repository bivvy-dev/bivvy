---
title: bivvy schema
description: Generate JSON Schema for config validation
---

# bivvy schema

Print or save the JSON Schema for `.bivvy/config.yml`. Use this to enable autocompletion and validation in your editor.

## Usage

```bash
bivvy schema
```

```bash
bivvy schema --output bivvy-schema.json
```

## Options

| Option | Description |
|--------|-------------|
| `-o`, `--output <path>` | Write the schema to a file instead of printing to stdout. Creates parent directories if they don't exist. |

## What It Does

Outputs the JSON Schema that describes the structure of Bivvy configuration files. The schema follows the [JSON Schema Draft-07](http://json-schema.org/draft-07/schema#) specification and covers all config properties including `app_name`, `settings`, `steps`, and `workflows`.

By default the schema is printed to stdout so you can pipe it to a file or another tool. With `--output`, Bivvy writes the schema directly to the specified path and confirms with a success message.

## Examples

Print schema to stdout:

```bash
bivvy schema
```

Save schema to a file:

```bash
bivvy schema --output bivvy-schema.json
```

Save to a nested path (directories are created automatically):

```bash
bivvy schema --output .vscode/bivvy-schema.json
```

Pipe to a file:

```bash
bivvy schema > bivvy-schema.json
```

## IDE Integration

The generated schema enables autocompletion, validation, and hover documentation in editors that support YAML language server features. See the [IDE Integration guide](/guides/ide-integration/) for full setup instructions covering VS Code, JetBrains IDEs, Neovim, and Helix.

In most cases you don't need to run `bivvy schema` at all: every bivvy invocation refreshes `~/.bivvy/schema.json`, which editors can point at directly.

### Quick Setup (VS Code)

Add to your user settings JSON:

```json
{
  "yaml.schemas": {
    "/Users/you/.bivvy/schema.json": "**/.bivvy/config.yml"
  }
}
```

Or add an inline schema directive to the top of your config file:

```yaml
# yaml-language-server: $schema=/Users/you/.bivvy/schema.json
app_name: my-app
```

`bivvy init` writes this directive automatically on new configs.
