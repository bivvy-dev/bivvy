---
title: CLI Reference
description: Bivvy command-line interface reference
---

# CLI Reference

Bivvy provides the following commands:

| Command | Description |
|---------|-------------|
| [`bivvy run`](./run.md) | Run setup workflow |
| [`bivvy init`](./init.md) | Initialize configuration |
| [`bivvy status`](./status.md) | Show current status |
| [`bivvy list`](./list.md) | List steps and workflows |
| [`bivvy last`](./last.md) | Show last run info |
| [`bivvy history`](./history.md) | Show execution history |
| [`bivvy lint`](./lint.md) | Validate configuration |
| [`bivvy config`](./config.md) | Show configuration |
| [`bivvy cache`](./cache.md) | Manage template cache |
| [`bivvy feedback`](./feedback.md) | Capture feedback |
| [`bivvy completions`](./completions.md) | Generate shell completions |

## Global Flags

These flags work with all commands:

| Flag | Short | Description |
|------|-------|-------------|
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |
| `--verbose` | `-v` | Verbose output |
| `--quiet` | `-q` | Minimal output |
| `--no-color` | | Disable colors |
| `--config` | `-c` | Path to config file |
| `--project` | `-p` | Project root path |
| `--debug` | | Enable debug logging |

## Output Modes

Bivvy supports three output modes:

- **Normal** (default): Standard output with progress indicators
- **Verbose** (`-v`): Detailed output with timestamps and debug info
- **Quiet** (`-q`): Minimal output, errors only

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error or validation failure |
| 2 | Configuration error |
| 130 | Interrupted (Ctrl+C) |
