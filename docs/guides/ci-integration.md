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

## Running Bivvy in CI

Beyond linting, you can run Bivvy itself in CI to set up your test
environment. Use `--env ci --non-interactive` so Bivvy auto-detects the
CI environment and skips interactive prompts:

```yaml
# GitHub Actions
- name: Setup environment
  run: bivvy run --env ci --non-interactive
```

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
