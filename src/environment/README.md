# Environment Subsystem

Developer-facing design doc for the environment detection and resolution system.

## Purpose

The environment subsystem determines where Bivvy is running (CI, Docker,
Codespace, development, or a custom environment) and adapts step behavior
accordingly — filtering steps, applying overrides, and skipping
requirements that are already provided.

## Module Responsibilities

| Module | Purpose |
|--------|---------|
| `detection.rs` | Built-in detection signals (env vars, filesystem checks) |
| `resolver.rs` | `ResolvedEnvironment` — priority-based resolution chain |

## Resolution Priority

```
--env flag
    │  (if set)
    ▼
ResolvedEnvironment { name, source: Flag }
    │  (if not set)
    ▼
settings.default_environment
    │  (if set)
    ▼
ResolvedEnvironment { name, source: ConfigDefault }
    │  (if not set)
    ▼
Auto-detection: custom rules → CI → Codespace → Docker
    │  (if matched)
    ▼
ResolvedEnvironment { name, source: AutoDetected(var_name) }
    │  (if nothing matched)
    ▼
ResolvedEnvironment { name: "development", source: Fallback }
```

## Built-in Detection Signals

Detection order within auto-detection:

1. **Custom rules** (from `settings.environments`, alphabetical order)
2. **CI** — broadest classifier, checked second
3. **Codespace** — GitHub Codespaces and Gitpod
4. **Docker** — container detection

### CI signals

| Variable | Notes |
|----------|-------|
| `CI` | Generic CI indicator |
| `GITHUB_ACTIONS` | GitHub Actions |
| `GITLAB_CI` | GitLab CI |
| `CIRCLECI` | CircleCI |
| `JENKINS_URL` | Jenkins |
| `BUILDKITE` | Buildkite |
| `TRAVIS` | Travis CI |
| `TF_BUILD` | Azure Pipelines (must equal `"True"`) |

All are presence checks except `TF_BUILD` which requires an exact value.

### Codespace signals

| Variable | Notes |
|----------|-------|
| `CODESPACES` | GitHub Codespaces |
| `GITPOD_WORKSPACE_ID` | Gitpod |

### Docker signals

| Signal | Notes |
|--------|-------|
| `DOCKER_CONTAINER` env var | Explicit container marker |
| `/.dockerenv` file exists | Docker filesystem marker |

## Custom Rule Handling

Custom rules from `settings.environments` are checked before built-in
signals. Each rule specifies an env var name and an optional expected
value:

```yaml
settings:
  environments:
    staging:
      detect:
        - env: DEPLOY_ENV
          value: staging
```

If multiple custom environments match, the first one alphabetically wins
and a warning is emitted about ambiguous detection.

## How Environments Affect Step Resolution

Once the active environment is resolved:

1. **Step filtering** — steps with `only_environments` that don't include
   the active environment are removed from the workflow
2. **Field overrides** — step fields are patched from the matching
   `environments.<name>` block (if present)
3. **Provided requirements** — requirements listed in the environment's
   `provided_requirements` are marked as Satisfied without checking

## Known Environments

The `known_environments()` function collects all recognized environment
names from:

1. Built-in: `ci`, `docker`, `codespace`, `development`
2. Custom: keys from `settings.environments`
3. Referenced: environment names from step `environments` keys
4. Referenced: names from step `only_environments` values

This set is used by lint rules to warn about unknown environment names.

## Testing

- Unit tests for detection use `temp_env` to set/unset env vars
- Resolver tests verify the full priority chain with combinations of
  flag, config default, and env vars
- Integration tests verify step filtering and override application
  end-to-end using config fixtures
