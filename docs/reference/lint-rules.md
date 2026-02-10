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
