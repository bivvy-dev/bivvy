---
title: bivvy feedback
description: Capture and manage friction points during development
---

# bivvy feedback

Captures and manages feedback entries with automatic session correlation.

## Usage

```bash
bivvy feedback "something feels off here"
```

```bash
bivvy feedback --tag ux,perf "slow step output"
```

```bash
bivvy feedback
```

```bash
bivvy feedback list
```

```bash
bivvy feedback list --all
```

```bash
bivvy feedback resolve <id>
```

```bash
bivvy feedback session
```

## Options

| Flag | Short | Description |
|------|-------|-------------|
| `--tag` | `-t` | Tags for categorization (comma-separated) |
| `--session` | | Session ID to attach (defaults to most recent) |

## Subcommands

### `feedback list`

| Flag | Description |
|------|-------------|
| `--status` | Filter by status: open, resolved, wontfix, inprogress |
| `--tag` | Filter by tag |
| `--all` | Show all entries including resolved |

### `feedback resolve <id>`

| Flag | Short | Description |
|------|-------|-------------|
| `--note` | `-n` | Resolution note |

### `feedback session [id]`

Shows feedback for a session. Defaults to the most recent session.

## Interactive Mode

Running `bivvy feedback` without a message opens an interactive prompt:

1. Select a category (bug, ux, feature, other)
2. Describe the feedback
3. Add optional tags

## Session Correlation

Every feedback entry is automatically linked to the most recent bivvy session,
making it easy to trace what you were doing when you noticed the issue.
