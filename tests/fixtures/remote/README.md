# Remote Workflow Acceptance Test Fixtures

These fixtures test `extends:` and `template_sources:` against the real
`bivvy-dev/shared-configs` repo on GitHub.

## Prerequisites

- Network access to GitHub
- The `bivvy-dev/shared-configs` repo must exist with `base.yml` and `templates.yml`

## Fixtures

| Fixture | Tests |
|---------|-------|
| `extends-basic` | Base steps from remote config appear and execute |
| `extends-override` | Local step with same name as base step wins |
| `extends-local-override` | `config.local.yml` overrides both base and project config |
| `extends-bad-url` | Unreachable extends URL produces clear error |
| `template-sources` | Steps using remote templates resolve and execute |
| `full-story` | Extends + template_sources + local steps all work together |

## Running

Build the gallant-lumiere branch, then run from each fixture directory:

```bash
BIVVY=target/debug/bivvy

# Basic extends
cd tests/fixtures/remote/extends-basic && $BIVVY list && $BIVVY run --non-interactive

# Local override
cd tests/fixtures/remote/extends-override && $BIVVY list

# config.local.yml override
cd tests/fixtures/remote/extends-local-override && $BIVVY status

# Bad URL error
cd tests/fixtures/remote/extends-bad-url && $BIVVY lint  # expect error

# Template sources
cd tests/fixtures/remote/template-sources && $BIVVY run --non-interactive

# Full story
cd tests/fixtures/remote/full-story && $BIVVY list && $BIVVY run --non-interactive
```
