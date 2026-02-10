# Templates

Templates define reusable setup steps. Bivvy includes 20 built-in templates covering system package managers, version managers, and language-specific dependency tools. You can also create custom templates.

## Using Templates

Reference a template in your step configuration:

```yaml
steps:
  deps:
    template: yarn
```

When you run `bivvy init`, templates are auto-detected based on files in your project (e.g., `Gemfile` triggers the `bundler` template, `package-lock.json` triggers `npm`).

## Available Built-in Templates

| Category | Templates |
|----------|-----------|
| System | `brew`, `apt`, `yum`, `pacman` |
| Windows | `chocolatey`, `scoop` |
| Version managers | `mise`, `asdf`, `volta` |
| Ruby | `bundler` |
| Node.js | `yarn`, `npm`, `pnpm`, `bun` |
| Python | `pip`, `poetry`, `uv` |
| Rust | `cargo` |
| Go | `go` |
| Swift | `swift` |

See [Built-in Templates](builtin.md) for full details on each template.

## Template Resolution Order

Templates are resolved in this order (first match wins):

1. **Project templates** - `.bivvy/templates/steps/`
2. **User templates** - `~/.bivvy/templates/steps/`
3. **Remote templates** - Fetched from configured sources
4. **Built-in templates** - Bundled with Bivvy

## Overriding Template Values

Override any template field in your step config:

```yaml
steps:
  deps:
    template: yarn
    command: "yarn install --frozen-lockfile"  # override default command
```

## Template Inputs

Some templates accept inputs for customization:

```yaml
steps:
  db:
    template: postgres
    inputs:
      database_name: myapp_dev
      port: 5433
```

## Creating Custom Templates

Create `.bivvy/templates/steps/<name>.yml`:

```yaml
name: my-template
description: "My custom setup step"
category: custom

inputs:
  env:
    description: "Environment name"
    type: string
    default: development

step:
  title: "Run my setup"
  command: "my-setup --env ${env}"
  completed_check:
    type: file_exists
    path: ".setup-complete"
```

## Next Steps

- [Built-in Templates](builtin.md)
- [Custom Templates](custom.md)
