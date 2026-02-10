---
title: bivvy lint
description: Validate configuration files
---

# bivvy lint

Validates your Bivvy configuration without executing anything.

## Usage

```bash
bivvy lint
```

```bash
bivvy lint --format=json
```

```bash
bivvy lint --format=sarif
```

```bash
bivvy lint --fix
```

```bash
bivvy lint --strict
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No errors (warnings OK) |
| 1 | Errors found |
| 2 | No configuration found |

With `--strict`, warnings also cause exit code 1.

## Example Output

### Valid Configuration

```
Configuration is valid!
```

### Invalid Configuration

```
error [default]: Workflow references unknown step 'nonexistent'

Found 1 error(s)
```

## Integration

### VS Code

Use the SARIF Viewer extension:

```bash
bivvy lint --format=sarif > bivvy.sarif
```

### GitHub Actions

```yaml
- name: Lint Bivvy config
  run: bivvy lint --format=sarif > bivvy.sarif
- uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: bivvy.sarif
```
