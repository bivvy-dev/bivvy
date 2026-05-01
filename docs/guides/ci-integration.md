---
title: CI Integration
description: Integrating Bivvy lint with CI/CD pipelines
---

# CI Integration

Bivvy's lint command supports SARIF output format, which is widely supported
by CI/CD systems and IDEs for displaying static analysis results.

## GitHub Actions

Upload SARIF results to GitHub Code Scanning:

```yaml
- name: Lint Bivvy config
  run: bivvy lint --format=sarif > bivvy.sarif

- uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: bivvy.sarif
```

## GitLab CI

Export lint results as a report artifact:

```yaml
lint:
  script:
    - bivvy lint --format=json > bivvy-lint.json
  artifacts:
    reports:
      codequality: bivvy-lint.json
```

## VS Code

Use the [SARIF Viewer extension](https://marketplace.visualstudio.com/items?itemName=MS-SarifVSCode.sarif-viewer)
to view lint results directly in your editor:

1. Install the SARIF Viewer extension
2. Run `bivvy lint --format=sarif > .bivvy/lint.sarif`
3. Open the SARIF file to see issues in the editor

## Auto-fixing simple lint issues

`bivvy lint --fix` rewrites the file in place to correct issues that have a
mechanical fix (renaming deprecated keys, normalizing fields, and similar).
Bigger structural problems still need manual review.

In a workflow that just lints the configuration, you can fail the job if
the file would have been changed:

```yaml
- name: Bivvy lint (no auto-fix in CI)
  run: bivvy lint --strict
```

If you'd rather have CI open a pull request with the fixes, run
`bivvy lint --fix` and let your usual "create-pr-on-diff" tooling handle the
rest:

```yaml
- name: Apply Bivvy lint auto-fixes
  run: bivvy lint --fix

- name: Open PR if anything changed
  uses: peter-evans/create-pull-request@v6
  with:
    title: "chore: apply bivvy lint --fix"
```

`--fix` is only safe to run on a clean working tree -- run it before tests,
not after, so any rewrites are obvious in the diff.

## Running Bivvy in CI

Beyond linting, you can run Bivvy itself in CI to set up your test
environment. Bivvy auto-detects CI environments (via `CI`, `GITHUB_ACTIONS`,
and other common variables) and automatically forces non-interactive mode:

```yaml
# GitHub Actions
- name: Setup environment
  run: bivvy run --env ci
```

In CI mode, the workflow progress bar is suppressed to avoid noisy output
in log-based environments. If you ever need to force this behavior outside
auto-detected CI, pass `--non-interactive`. (An older `--ci` flag exists
for backwards compatibility but is deprecated and hidden -- prefer
`--non-interactive --env ci`.)

### Provided requirements

CI pipelines typically manage services (databases, caches) outside of
Bivvy. Use `provided_requirements` to skip gap checks for tools that
are already available:

```yaml
# .bivvy/config.yml
settings:
  environments:
    ci:
      provided_requirements:
        - postgres-server
        - redis-server
        - docker
```

This prevents Bivvy from trying to install or start services that the
pipeline already provides.

### Environment-specific workflows

You can also set a `default_workflow` for CI to run a different set of
steps:

```yaml
settings:
  environments:
    ci:
      default_workflow: ci
      provided_requirements:
        - postgres-server

workflows:
  ci:
    description: "CI setup (no prompts)"
    steps: [deps, database, migrations]
    settings:
      non_interactive: true
```

See [Environments](../configuration/environments.md) for the full
environment configuration reference.

## Exit Codes

The lint command returns:
- `0` - No errors (warnings allowed)
- `1` - One or more errors found
- `2` - Configuration loading error

Use `--strict` to treat warnings as errors.
