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
