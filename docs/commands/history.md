---
title: bivvy history
description: Show execution history
---

# bivvy history

Shows the execution history for your project.

## Usage

```bash
bivvy history
```

```bash
bivvy history --limit=20
```

```bash
bivvy history --step=name
```

```bash
bivvy history --json
```

## Example Output

```
Run History:
  2024-01-15 14:32 | default | 2m 15s | Success
  2024-01-14 09:15 | default | 5m 30s | Success
  2024-01-13 16:45 | ci      | 45s    | Failed
  2024-01-12 11:20 | default | 3m 10s | Success
```

## Step History

```bash
bivvy history --step=ruby_deps
```

```
Step History: ruby_deps
  2024-01-15 14:32 | Success | default
  2024-01-14 09:15 | Success | default
  2024-01-13 16:45 | Failed  | ci
```

## Limiting Results

By default, the last 10 runs are shown. Use `--limit` to show more:

```bash
bivvy history --limit=50
```
