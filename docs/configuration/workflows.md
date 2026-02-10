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
      default_output: quiet
      fail_fast: true
```
