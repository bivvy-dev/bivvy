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

The generated schema enables autocompletion, validation, and hover documentation in editors that support YAML language server features. See the [IDE Integration guide](/guides/ide-integration/) for setup instructions for VS Code, JetBrains IDEs, and other editors.

### Quick Setup (VS Code)

```bash
bivvy schema --output bivvy-schema.json
```

Then add to `.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "./bivvy-schema.json": ".bivvy/config.yml"
  }
}
```

Or add an inline schema comment to the top of your config file, pointing at either a local file or the hosted schema:

```yaml
# yaml-language-server: $schema=https://bivvy.dev/schemas/config.json
app_name: my-app
```
