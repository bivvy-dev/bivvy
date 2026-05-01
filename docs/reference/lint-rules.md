---
title: Lint Rules Reference
description: Complete reference for Bivvy's configuration validation rules
---

# Lint Rules Reference

Bivvy includes built-in lint rules to validate your configuration.
Run `bivvy lint` to check your configuration for issues.

## Severity Levels

| Level | Description |
|-------|-------------|
| Error | Prevents execution, must be fixed |
| Warning | Should be addressed, but won't block |
| Hint | Informational suggestion |

## Output Formats

The lint command supports multiple output formats:

Human-readable (default):

```bash
bivvy lint
```

JSON output:

```bash
bivvy lint --format=json
```

SARIF output (for IDE/CI integration):

```bash
bivvy lint --format=sarif
```

## Auto-Fix

Some rules support automatic fixes:

```bash
bivvy lint --fix
```

## Strict Mode

Treat warnings as errors:

```bash
bivvy lint --strict
```

## Built-in Rules

### app-name-format

**Severity:** Warning (Error if empty)
**Auto-fix:** Yes

Validates the `app_name` field follows naming conventions.

**Checks:**
- `app_name` is not empty (Error)
- `app_name` does not contain spaces (Warning)

**Example - Invalid:**
```yaml
app_name: "My App Name"  # Warning: contains spaces
```

**Example - Valid:**
```yaml
app_name: my-app-name
```

**Suggestion:** When spaces are detected, suggests kebab-case alternative.

---

### required-fields

**Severity:** Error
**Auto-fix:** No

Ensures required configuration fields are present.

**Checks:**
- `app_name` field must be present (Error)
- At least one workflow should be defined (Warning)

**Example - Invalid:**
```yaml
# Missing app_name
steps:
  hello:
    command: echo hello
```

**Example - Valid:**
```yaml
app_name: my-app
steps:
  hello:
    command: echo hello
workflows:
  default:
    steps: [hello]
```

---

### circular-dependency

**Severity:** Error
**Auto-fix:** No

Detects circular dependencies between steps in the `depends_on` field.

**Checks:**
- No step can be part of a dependency cycle

**Example - Invalid:**
```yaml
steps:
  a:
    command: echo a
    depends_on: [b]
  b:
    command: echo b
    depends_on: [c]
  c:
    command: echo c
    depends_on: [a]  # Creates cycle: a -> b -> c -> a
```

**Example - Valid:**
```yaml
steps:
  a:
    command: echo a
    depends_on: [b]
  b:
    command: echo b
    depends_on: [c]
  c:
    command: echo c
```

**Diagnostic:** Reports the full cycle path (e.g., "a -> b -> c -> a").

---

### self-dependency

**Severity:** Error
**Auto-fix:** No

Detects steps that depend on themselves.

**Checks:**
- A step cannot list itself in `depends_on`

**Example - Invalid:**
```yaml
steps:
  build:
    command: make build
    depends_on: [build]  # Error: self-dependency
```

**Example - Valid:**
```yaml
steps:
  build:
    command: make build
    depends_on: [setup]
```

---

### undefined-dependency

**Severity:** Error
**Auto-fix:** No

Ensures all `depends_on` references point to existing steps.

**Checks:**
- Every step name in `depends_on` must exist in `steps`

**Example - Invalid:**
```yaml
steps:
  build:
    command: make build
    depends_on: [nonexistent]  # Error: step doesn't exist
```

**Example - Valid:**
```yaml
steps:
  setup:
    command: npm install
  build:
    command: npm run build
    depends_on: [setup]
```

---

### undefined-workflow-force

**Severity:** Error
**Auto-fix:** No

Ensures every step name in a workflow's `force:` list refers to a defined step.

**Checks:**
- Each entry in `workflows.<name>.force` must exist in `steps`

**Example - Invalid:**
```yaml
steps:
  build:
    command: cargo build
workflows:
  release:
    steps: [build]
    force: [nonexistent]  # Error: step doesn't exist
```

**Example - Valid:**
```yaml
steps:
  build:
    command: cargo build
workflows:
  release:
    steps: [build]
    force: [build]
```

**Diagnostic:** Reports the workflow name and the undefined step (e.g., "Workflow 'release' force list references undefined step 'nonexistent'").

---

### undefined-template

**Severity:** Error
**Auto-fix:** No

Validates that template references resolve to actual templates.

**Checks:**
- Template names in `template` field must exist in the registry

**Example - Invalid:**
```yaml
steps:
  deps:
    template: nonexistent-template  # Error: template not found
```

**Example - Valid:**
```yaml
steps:
  deps:
    template: brew  # Built-in template
    inputs:
      packages: [git, node]
```

---

### template-inputs

**Severity:** Error/Warning
**Auto-fix:** No

Validates that template inputs match their contracts.

**Checks:**
- Required inputs without defaults must be provided (Error)
- Input types must match the template contract (Error)
- Unknown inputs produce a warning (Warning)

**Example - Invalid (missing required):**
```yaml
steps:
  deps:
    template: my-template
    # Missing required input 'packages'
```

**Example - Invalid (wrong type):**
```yaml
steps:
  deps:
    template: my-template
    inputs:
      enabled: "not a boolean"  # Error: expected boolean
```

**Example - Valid:**
```yaml
steps:
  deps:
    template: my-template
    inputs:
      packages: [git, node]
      enabled: true
```

---

### check-fields-exclusive

**Severity:** Error
**Auto-fix:** No

Ensures a step does not set both `check` (singular) and `checks` (list) at the
same time. Pick one form per step.

**Checks:**
- A step with `check` set must not also have a non-empty `checks` list

**Example - Invalid:**
```yaml
steps:
  build:
    command: cargo build
    check:
      type: presence
      target: target/debug/myapp
    checks:
      - type: execution
        command: cargo build --offline
```

**Example - Valid:**
```yaml
steps:
  build:
    command: cargo build
    checks:
      - type: presence
        target: target/debug/myapp
      - type: execution
        command: cargo build --offline
```

**Diagnostic:** "Step '<name>' has both 'check' and 'checks' fields. Use only one."

---

### deprecated-fields

**Severity:** Warning
**Auto-fix:** No

Detects deprecated YAML field names and `type:` values. Because old field
names have been removed from the typed schema, serde would silently ignore
them — this rule scans the raw YAML text so renamed fields do not become
silent no-ops.

**Detected fields:**

| Deprecated | Replacement |
|------------|-------------|
| `completed_check:` | `check:` or `checks:` |
| `type: marker` | Use a specific check type, or remove the check |
| `type: file_exists` | `type: presence` |
| `type: command_succeeds` | `type: execution` |
| `watches:` | `check: { type: change, target: ... }` |
| `prompt_if_complete:` | `prompt_on_rerun:` |
| `log_path:` (in output settings) | Removed — JSONL logs are written to `~/.bivvy/logs/` automatically |

**Example - Invalid:**
```yaml
steps:
  setup:
    command: bundle install
    completed_check:        # Warning: deprecated, use 'check' or 'checks'
      type: file_exists     # Warning: use 'type: presence' instead
      path: Gemfile.lock
    prompt_if_complete: true  # Warning: use 'prompt_on_rerun' instead
```

**Example - Valid:**
```yaml
steps:
  setup:
    command: bundle install
    check:
      type: presence
      target: Gemfile.lock
    prompt_on_rerun: true
```

**Diagnostic:** Each deprecated field is reported with its file, line number, and the suggested replacement.

---

### unknown-requirement

**Severity:** Warning
**Auto-fix:** No

Requirement name in a step's `requires` list is not in the built-in registry
or the config's `requirements` map.

**Checks:**
- Every name in `requires` must be a known requirement

**Example - Invalid:**
```yaml
steps:
  build:
    command: make build
    requires: [nonexistent-tool]  # Warning: unknown requirement
```

**Example - Valid:**
```yaml
steps:
  build:
    command: make build
    requires: [ruby, node]
```

---

### circular-requirement-dep

**Severity:** Error
**Auto-fix:** No

Circular dependency chain detected in requirement install dependencies.

**Example - Invalid:**
A requirement whose install template depends on itself through a chain.

---

### unknown-environment-in-step

**Severity:** Warning
**Auto-fix:** No

A step's `environments` override block references an environment name that
is not a built-in environment and not defined in `settings.environments`.

**Example - Invalid:**
```yaml
steps:
  build:
    command: make build
    environments:
      nonexistent:  # Warning: unknown environment
        command: make build-fast
```

---

### unknown-environment-in-only

**Severity:** Warning
**Auto-fix:** No

A step's `only_environments` list includes an environment name that is not
a built-in environment and not defined in `settings.environments`.

**Example - Invalid:**
```yaml
steps:
  build:
    command: make build
    only_environments: [nonexistent]  # Warning: unknown environment
```

---

### environment-default-workflow-missing

**Severity:** Error
**Auto-fix:** No

An environment's `default_workflow` references a workflow that doesn't exist
in the config.

---

### unreachable-environment-override

**Severity:** Warning
**Auto-fix:** No

A step has an environment override for an environment that is excluded by
its own `only_environments` list. The override can never take effect.

---

### custom-environment-shadows-builtin

**Severity:** Warning
**Auto-fix:** No

A custom environment name in `settings.environments` matches a built-in
environment name (`ci`, `docker`, `codespace`, `development`).

---

### redundant-environment-override

**Severity:** Hint
**Auto-fix:** No

An environment override specifies field values identical to the base step.

---

### redundant-env-null

**Severity:** Hint
**Auto-fix:** No

An environment override sets an env var to `null` (remove), but that var
is not present in the base step's `env` map.

---

### environment-circular-dependency

**Severity:** Error
**Auto-fix:** No

Circular dependency detected in per-environment `depends_on` overrides.

---

### install-template-missing

**Severity:** Hint
**Auto-fix:** No

A requirement has no install template, so Bivvy cannot offer automatic
installation.

---

### service-requirement-without-hint

**Severity:** Warning
**Auto-fix:** No

A service requirement (e.g., `postgres-server`) lacks an `install_hint`,
making it hard for users to fix the gap manually.

---

### `workflow-shape-shorthand` (error)

What it catches: a workflow value written as a bare YAML sequence
instead of a mapping with a `steps:` key. Inspects raw YAML in
`.bivvy/config.yml` and `.bivvy/workflows/*.yml` so the diagnostic
points at the offending file and line.

Bad:
```yaml
workflows:
  default:
    - build
    - test
```

Good:
```yaml
workflows:
  default:
    steps:
      - build
      - test
```

---

### `workflow-singular-typo` (error)

What it catches: top-level key typos in the workflow files.
`.bivvy/config.yml` must use `workflows:` (plural). A workflow split
file under `.bivvy/workflows/` must use `workflow:` (singular). Either
typo would otherwise be silently ignored by serde defaults.

Bad (in `.bivvy/config.yml`):
```yaml
workflow:
  default:
    steps: [build]
```

Good (in `.bivvy/config.yml`):
```yaml
workflows:
  default:
    steps: [build]
```

---

### `workflow-references-template-not-step` (warning)

What it catches: a workflow's `steps:` list contains a name that
includes a `/`, strongly suggesting a template path was pasted in
place of a step alias. The suggestion shows the matching `bivvy add`
invocation that would register the template under an alias.

Bad:
```yaml
workflows:
  release:
    steps:
      - rust/version-bump
```

Good:
```yaml
steps:
  version-bump:
    template: rust/version-bump
workflows:
  release:
    steps:
      - version-bump
```

---

### `step-name-collision` (warning)

What it catches: the same step name is defined with diverging bodies
in multiple files (e.g., `.bivvy/config.yml` and
`.bivvy/steps/setup.yml`). Merging silently picks one definition and
the other becomes invisible. Walks up from the cwd to find the
project root; skips silently when no `.bivvy/` directory exists.

Bad (`.bivvy/config.yml`):
```yaml
steps:
  setup:
    command: cargo build
```
Bad (`.bivvy/steps/setup.yml`):
```yaml
steps:
  setup:
    command: cargo test
```

Good: define `setup` in exactly one file, or rename one of them.

---

### `unused-step` (hint)

What it catches: a step defined in `steps:` that is not reached by
any workflow — directly through a `steps:` list, transitively through
`depends_on`, or via `force:` / `overrides:` on a workflow. Skipped
when no workflows are defined.

Bad:
```yaml
steps:
  build:
    command: cargo build
  orphan:
    command: cargo install
workflows:
  default:
    steps: [build]
```

Good: reference `orphan` from a workflow or remove the step.

---

### `unused-template-source` (hint)

What it catches: a `template_sources:` entry whose name (last URL
path segment, sans `.git`) doesn't match the prefix of any step's
`template:` field. The heuristic is permissive — the rule is
informational only.

Bad:
```yaml
template_sources:
  - url: https://github.com/acme/postgres-tpl.git
  - url: https://github.com/acme/redis-tpl.git
steps:
  install_pg:
    template: postgres-tpl/server
```

Good: also use `redis-tpl/<name>` from a step, or remove the
`redis-tpl` source.

---

### `dead-environment` (hint)

What it catches: an environment defined in `settings.environments`
that is never referenced by `settings.default_environment`, a step's
`only_environments`, or a step's `environments:` override. Built-in
names (`ci`, `docker`, `codespace`, `development`) are always live.

Bad:
```yaml
settings:
  environments:
    staging:
      provided_requirements: [postgres-server]
```

Good: reference `staging` from a step's `only_environments`, set
`settings.default_environment: staging`, or remove the entry.

---

### `interpolation-syntax-error` (error)

What it catches: malformed `${...}` interpolation in any
string-valued config field. Subsumes the proposed
`var-references-undefined-var` rule. Specifically reports:

- Unterminated `${...` (no closing brace)
- Empty references `${}`
- Dotted references in unknown namespaces (e.g. `${unknown.foo}`)
- Keys missing in known namespaces (`vars`, `secrets`, `prompts`)
- Flat references that don't resolve to any var, secret, prompt,
  built-in, or env-var-shaped name (uppercase + underscores)

Bad:
```yaml
steps:
  greet:
    command: "echo ${typo_var}"
```

Good:
```yaml
vars:
  typo_var: hello
steps:
  greet:
    command: "echo ${typo_var}"
```

---

### `secret-without-handler` (warning)

What it catches: a step references `${secrets.<name>}` but the
secret's `command:` handler is empty (or whitespace-only). The
command field is the only resolution path, so an empty command means
the reference cannot resolve at runtime.

Bad:
```yaml
secrets:
  api_key:
    command: ""
steps:
  fetch:
    command: 'curl -H "Auth: ${secrets.api_key}"'
```

Good:
```yaml
secrets:
  api_key:
    command: op read api_key
```

---

### `local-config-overrides-secret` (hint)

What it catches: `.bivvy/config.local.yml` redefines a
`secrets.<name>.command:` previously declared in `.bivvy/config.yml`.
Local edits are gitignored, so a teammate auditing the committed
config wouldn't see the change. Surfacing it lets reviewers confirm
the override is intentional.

Bad (`.bivvy/config.yml`):
```yaml
secrets:
  api_key:
    command: op read api_key
```
Bad (`.bivvy/config.local.yml`):
```yaml
secrets:
  api_key:
    command: cat ~/.secrets/api_key
```

Good: keep the project handler aligned, or accept the local override
deliberately.

---

## IDE Integration

### VS Code

Use the SARIF Viewer extension to see lint results inline:

```bash
bivvy lint --format=sarif > bivvy.sarif
```

### GitHub Actions

Upload SARIF results to GitHub Code Scanning:

```yaml
- name: Lint Bivvy config
  run: bivvy lint --format=sarif > bivvy.sarif
- uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: bivvy.sarif
```

## JSON Output Schema

The JSON output format includes:

```json
{
  "diagnostics": [
    {
      "rule_id": "circular-dependency",
      "severity": "error",
      "message": "Circular dependency detected: a -> b -> a",
      "file": ".bivvy/config.yml",
      "line": 5,
      "column": 3,
      "suggestion": null
    }
  ],
  "summary": {
    "total": 1,
    "errors": 1,
    "warnings": 0,
    "hints": 0
  }
}
```
