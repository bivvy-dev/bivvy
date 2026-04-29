---
title: IDE Integration
description: Set up autocomplete and validation for .bivvy/config.yml
---

# IDE Integration

Bivvy ships a JSON Schema describing every field in `.bivvy/config.yml`. With it
wired up, your editor provides autocomplete, hover documentation, and inline
errors for unknown or mistyped fields.

## How it works

Every time `bivvy` runs, it writes the current schema to `~/.bivvy/schema.json`.
That file always reflects the version of bivvy you have installed — no manual
regeneration step. Editors that support the
[YAML language server](https://github.com/redhat-developer/yaml-language-server)
read it to validate your config.

`bivvy init` writes a `# yaml-language-server` directive at the top of new
`.bivvy/config.yml` files automatically, so brand-new projects work with zero
extra setup. The methods below cover existing configs and editors that need
explicit configuration.

Pick the method that matches your environment. They're listed easiest first.

## Method 1: Inline directive (per-file)

Add this comment to the top of `.bivvy/config.yml`:

```yaml
# yaml-language-server: $schema=/Users/you/.bivvy/schema.json
app_name: my-app
```

Use the absolute path — `~` is not reliably expanded by every YAML language
server. On Linux that's `/home/you/.bivvy/schema.json`; on macOS it's
`/Users/you/.bivvy/schema.json`.

This method requires no editor configuration. It's the fastest way to get
validation working in an existing config and travels with the file when shared.

## Method 2: User-level editor settings (every Bivvy project)

Configure your editor once and every `.bivvy/config.yml` you open is validated.

### VS Code, Cursor, Windsurf, and other VS Code forks

Install the
[Red Hat YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml)
if it isn't already. Then open the user settings JSON
(`Cmd+Shift+P` → "Preferences: Open User Settings (JSON)") and add:

```json
{
  "yaml.schemas": {
    "/Users/you/.bivvy/schema.json": "**/.bivvy/config.yml"
  }
}
```

The `**/` glob prefix ensures the schema matches the file no matter how deeply
nested the project is in your workspace.

After saving, reload the window (`Cmd+Shift+P` → "Developer: Reload Window") so
the YAML extension picks up the change.

### JetBrains IDEs (IntelliJ, RubyMine, PyCharm, GoLand, etc.)

1. Open **Preferences → Languages & Frameworks → Schemas and DTDs → JSON Schema Mappings**
2. Click **+** to add a new mapping:
   - **Name:** `Bivvy`
   - **Schema file or URL:** `/Users/you/.bivvy/schema.json`
   - **Schema version:** Auto-detect (or JSON Schema Version 7)
3. Add a file path pattern: `*/.bivvy/config.yml`

### Neovim with `yaml-language-server`

```lua
require('lspconfig').yamlls.setup({
  settings = {
    yaml = {
      schemas = {
        ['/Users/you/.bivvy/schema.json'] = '**/.bivvy/config.yml',
      },
    },
  },
})
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.yaml-language-server.config.yaml.schemas]
"/Users/you/.bivvy/schema.json" = "**/.bivvy/config.yml"
```

## Method 3: Workspace settings (one project)

When you want validation only in a specific project — or you'd like the schema
mapping to live in the repo so contributors get it automatically — commit a
`.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "/Users/you/.bivvy/schema.json": ".bivvy/config.yml"
  }
}
```

The hard-coded user path won't be portable across machines. For a portable
setup, generate a project-local schema copy and reference it relatively:

```bash
bivvy schema --output .vscode/bivvy-schema.json
```

```json
{
  "yaml.schemas": {
    "./.vscode/bivvy-schema.json": ".bivvy/config.yml"
  }
}
```

The local copy will go stale after upgrading bivvy. Re-run `bivvy schema
--output` after each upgrade, or stick with `~/.bivvy/schema.json` (Method 2),
which refreshes itself.

## Verify it's working

Open `.bivvy/config.yml` and add a deliberately invalid field:

```yaml
not_a_real_field: true
```

You should see an error like:

> Property `not_a_real_field` is not allowed.

If nothing happens, check:

- The YAML extension or language server is installed and active.
- `~/.bivvy/schema.json` exists. Run any bivvy command (e.g., `bivvy --version`)
  to create it.
- The path in your settings is **absolute** and exists on disk.
- For VS Code forks, you reloaded the window after editing settings.
- Status bar (VS Code) shows the schema title — usually `BivvyConfig` — when
  the config file is open. If it doesn't, the mapping didn't match; double-check
  the file pattern (try `**/.bivvy/config.yml`).

## What gets validated

The schema enforces:

- **Field names** — unknown fields anywhere in the config produce errors,
  including under nested objects like `settings`, `steps.*`, and `workflows.*`.
- **Types** — strings vs. booleans vs. numbers, plus enum values.
- **Required fields** — missing required keys (e.g., `command` on a check).
- **Structure** — nested objects, arrays, and tagged unions like `check`/`checks`.

You'll get autocomplete for every property, hover documentation drawn from the
field's rustdoc, and snippet completion for common structures (steps, workflows,
prompts).

## Updating the schema

`~/.bivvy/schema.json` is rewritten on every `bivvy` invocation, so just run any
command after upgrading bivvy (`bivvy --version` is enough) and your editor
will pick up the new schema on its next reload.

If you maintain a project-local copy (Method 3, second variant), regenerate it
with:

```bash
bivvy schema --output path/to/schema.json
```

See [`bivvy schema`](/commands/schema/) for more options.
