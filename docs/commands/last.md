---
title: bivvy last
description: Show last run information
---

# bivvy last

Shows details about the last execution.

## Usage

```bash
bivvy last
```

```bash
bivvy last --json
```

```bash
bivvy last --step=name
```

```bash
bivvy last --all
```

```bash
bivvy last --output
```

## Example Output

```
Last Run
─────────────────────────────────────

Date: 2024-01-15 14:32:05
Workflow: default
Duration: 2m 15s
Status: Success

Steps executed:
  - brew: Success
  - mise: Success
  - ruby_deps: Success

Skipped: seeds (--skip)
```

## No History

If no runs have been recorded:

```
No runs recorded for this project.
```

## Failed Run

If the last run failed, the error message is displayed:

```
Status: Failed

Error: Step 'database' failed with exit code 1
```
