# Workflows

Workflows define which steps run and in what order.

## Defining Workflows

```yaml
workflows:
  default:
    steps: [deps, db, assets]

  ci:
    steps: [deps, test, lint]

  reset:
    steps: [db_drop, db_create, db_migrate, db_seed]
```

## Running Workflows

```bash
bivvy
```

```bash
bivvy run ci
```

```bash
bivvy run reset
```

## Step Dependencies

Use `depends_on` to ensure steps run in order:

```yaml
steps:
  tools:
    template: asdf

  deps:
    template: yarn
    depends_on: [tools]

  db:
    template: postgres
    depends_on: [tools]

  assets:
    command: "yarn build"
    depends_on: [deps]

workflows:
  default:
    steps: [tools, deps, db, assets]
```

Bivvy resolves dependencies automatically. Steps without dependencies between them may run in parallel (future feature).

## Before/After Hooks

Run commands before or after a step:

```yaml
steps:
  db_migrate:
    command: "rails db:migrate"
    before:
      - "echo 'Starting migration...'"
    after:
      - "rails db:seed"
```

## Skipping Steps

```bash
bivvy --skip deps
```

```bash
bivvy --only db,assets
```

### Skip Behavior

When a step is skipped, its dependents can be handled differently:

```yaml
settings:
  skip_behavior: skip_with_dependents  # Also skip steps that depend on skipped step
  # OR
  skip_behavior: run_dependents        # Still try to run dependent steps
```

## Workflow Environment Variables

A workflow can declare environment variables that apply to every step
it runs:

```yaml
workflows:
  ci:
    steps: [deps, test]
    env:
      CI: "true"
      RAILS_ENV: test
    env_file: .env.ci
```

`env_file` is loaded relative to the project root. Values in `env:`
win over `env_file`. Workflow-level values override
`settings.env:` / `settings.env_file:` from the project-wide settings,
and step-level `env:` / `env_file:` override workflow-level values.
Shell-exported variables (`FOO=bar bivvy run`) win over all of them —
see [Environment Variable Precedence](steps.md#environment-variable-precedence).

## Workflow Settings

Override global settings per workflow:

```yaml
workflows:
  ci:
    steps: [deps, test]
    settings:
      defaults:
        output: quiet
      fail_fast: true
```

## Forcing Steps in a Workflow

Workflows can opt specific steps — or every step — out of check
evaluation, the same way `--force` and `--force-all` do on the CLI.
Useful when a workflow exists specifically to refresh state (a
`fresh-start` workflow that always reinstalls, a `migrate` workflow
that should never trust the satisfaction cache, etc.).

Force specific steps every time the workflow runs:

```yaml
workflows:
  partial-refresh:
    steps: [install, build, migrate]
    force: [migrate]
```

Force every step in the workflow:

```yaml
workflows:
  fresh-start:
    steps: [install, build, migrate]
    force_all: true
```

Force is monotonic — workflow-level directives are unioned with CLI
flags and step-level `force: true`. Any source can opt a step in;
nothing turns force off. Preconditions are still never bypassed. See
[Forcing Re-run](completed-checks.md#forcing-re-run) for the full
matrix.

## Portable Workflow Files

A file at `.bivvy/workflows/<name>.yml` can carry its own step
definitions and variables alongside the workflow declaration. Drop
one of these into a project and `bivvy run <name>` works end-to-end —
no need to split steps across multiple files.

```yaml
# .bivvy/workflows/release-prepare.yml
description: "Prepare a release branch"

vars:
  current_version:
    command: "bin/rails runner 'puts MyApp::VERSION'"

steps:
  fetch-working:
    title: "Fetch working branch"
    command: "git fetch origin ${working_branch}"

  finalize-changelog:
    title: "Finalize changelog"
    command: "bundle exec rake reissue:finalize"
    depends_on: [fetch-working]

workflow:
  steps:
    - fetch-working
    - finalize-changelog
  env_file: .bivvy/release.env
```

The `workflow:` block is the workflow declaration itself (ordering,
env, force directives) — exactly the same shape as inline workflows
under `workflows:` in `.bivvy/config.yml`. The file's top-level
`steps:` and `vars:` blocks are bundled with this workflow but can be
overridden by the project file or `.bivvy/config.local.yml`.

### Resolution Order

When `bivvy run <name>` executes, configuration merges in this order
(later overrides earlier):

1. Remote `extends:` URLs
2. `~/.bivvy/config.yml`
3. `.bivvy/config.yml`
4. `.bivvy/steps/*.yml`
5. `.bivvy/workflows/<the-named-one>.yml`
6. `.bivvy/config.local.yml`

Only the workflow file matching the requested name participates —
sibling workflow files are not parsed, so a malformed neighbor cannot
break a run of an unrelated workflow.

### Legacy Workflow Files

Files written in the legacy shape (`description` + `steps:` as an
ordered list of names, no `workflow:` block) continue to work
unchanged. The new format is detected by the presence of a top-level
`workflow:` mapping.
