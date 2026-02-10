---
title: IDE Integration
description: Setting up IDE autocomplete and validation for Bivvy
---

# IDE Integration

Bivvy provides a JSON Schema for configuration files to enable
autocomplete and validation in your IDE.

## VS Code

Add to your `.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "https://bivvy.dev/schemas/config.json": ".bivvy/config.yml"
  }
}
```

Alternatively, add a schema comment at the top of your config file:

```yaml
# yaml-language-server: $schema=https://bivvy.dev/schemas/config.json
app_name: my-app
```

## JetBrains IDEs

1. Open Preferences > Languages & Frameworks > Schemas and DTDs > JSON Schema Mappings
2. Add a new mapping:
   - Schema URL: `https://bivvy.dev/schemas/config.json`
   - File pattern: `.bivvy/config.yml`

## Generate Schema Locally

Generate a local schema file for offline use:

```bash
bivvy lint --schema > bivvy-schema.json
```

Then reference the local file in your IDE settings:

```json
{
  "yaml.schemas": {
    "./bivvy-schema.json": ".bivvy/config.yml"
  }
}
```

## Features

With IDE integration configured, you'll get:

- **Autocomplete** - Suggestions for property names and values
- **Validation** - Errors for invalid property names or types
- **Hover documentation** - Descriptions for each property
- **Snippets** - Quick insertion of common structures
