# Templates

Templates define reusable setup steps. Bivvy ships dozens of built-in templates covering system package managers, version managers, language-specific dependency tools, database migrations, containers, infrastructure as code, and monorepo tools. You can also create your own.

## Using Templates

Reference a template in your step configuration by its name:

```yaml
steps:
  deps:
    template: yarn-install
```

When you run `bivvy init`, templates are auto-detected based on files in your project (e.g. `Gemfile` triggers `bundle-install`, `package-lock.json` triggers `npm-install`, `yarn.lock` triggers `yarn-install`).

> Template names are the long form, like `yarn-install`, `bundle-install`, `cargo-build`, `mise-tools`, `nextjs-build`. There is no alias mechanism — short names like `yarn` or `cargo` will not resolve.

## Available Built-in Templates

| Category | Templates |
|----------|-----------|
| System | `brew-bundle`, `apt-install`, `yum-install`, `pacman-install` |
| Windows | `choco-install`, `scoop-install` |
| Version managers | `mise-tools`, `asdf-tools`, `volta-setup`, `fnm-setup`, `nvm-node`, `rbenv-ruby`, `pyenv-python` |
| Ruby | `bundle-install`, `rails-db`, `ruby/version-bump` |
| Node.js | `yarn-install`, `npm-install`, `pnpm-install`, `bun-install`, `nextjs-build`, `vite-build`, `remix-build`, `prisma-migrate`, `node/version-bump` |
| Python | `pip-install`, `poetry-install`, `uv-sync`, `django-migrate`, `alembic-migrate`, `python/version-bump` |
| PHP | `composer-install`, `laravel-setup`, `php/version-bump` |
| Rust | `cargo-build`, `diesel-migrate`, `rust/version-bump` |
| Go | `go-mod-download`, `go/version-bump` |
| Swift | `swift-resolve`, `swift/version-bump` |
| Java | `maven-resolve`, `java/version-bump` |
| Kotlin / Gradle | `gradle-deps`, `spring-boot-build`, `kotlin/version-bump` |
| Elixir | `mix-deps-get`, `elixir/version-bump` |
| .NET | `dotnet-restore`, `dotnet/version-bump` |
| Dart / Flutter | `dart-pub-get`, `flutter-pub-get` |
| Deno | `deno-install` |
| Database migrations | `rails-db`, `prisma-migrate`, `diesel-migrate`, `alembic-migrate`, `django-migrate` |
| Containers | `docker-compose-up`, `helm-deps` |
| Infrastructure as Code | `terraform-init`, `cdk-synth`, `pulumi-install`, `ansible-install` |
| Artifact audits | `node-artifact-audit`, `rust-artifact-audit`, `python-artifact-audit`, `go-artifact-audit`, `java-artifact-audit`, `dotnet-artifact-audit`, `docker-artifact-audit`, `ruby-artifact-audit`, `php-artifact-audit`, `elixir-artifact-audit`, `swift-artifact-audit` |
| Cross-cutting | `env-copy`, `pre-commit-install` |
| Monorepo / Workspace | `nx-build`, `turbo-build`, `lerna-bootstrap` |

See [Built-in Templates](builtin.md) for full details on each template.

> Several languages define their own `version-bump` template. When referencing one, qualify it with the category (e.g. `template: rust/version-bump`) so the registry resolves the correct one. An unqualified `version-bump` returns the first match across categories.

## Template Resolution Order

Templates are resolved in this order (first match wins):

1. **Project templates** — `.bivvy/templates/steps/`
2. **User templates** — `~/.bivvy/templates/steps/`
3. **Remote templates** — fetched from configured sources
4. **Built-in templates** — bundled with Bivvy

References can be unqualified (`yarn-install`) or qualified by category (`node/yarn-install`). A qualified reference must match both the name and the category.

## Overriding Template Values

Override any template field in your step config:

```yaml
steps:
  deps:
    template: yarn-install
    command: "yarn install --frozen-lockfile"  # overrides the template's default command
```

## Template Inputs

Some templates accept inputs for customization. Inputs are case-sensitive map keys under `inputs:`:

```yaml
steps:
  release:
    template: rust/version-bump
    inputs:
      bump: minor
```

Inputs can also be supplied interactively by adding a `prompts:` block to the step (see the `version-bump` example in [Built-in Templates](builtin.md#version-bump-templates)).

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
  check:
    type: presence
    target: ".setup-complete"
```

See the annotated [template reference YAML](../reference/template-reference.yml) for every available field.

## Next Steps

- [Built-in Templates](builtin.md)
- [Remote Template Sources](remote-sources.md)
