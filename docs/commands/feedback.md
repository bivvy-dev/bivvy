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
| `--no-deliver` | | Skip the delivery prompt (save locally only) |

## Subcommands

### `feedback list`

| Flag | Description |
|------|-------------|
| `--status` | Filter by status: open, resolved, wontfix, inprogress (or in_progress) |
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

## Delivery Workflow

After capturing feedback, bivvy offers to deliver it by opening a GitHub issue or sending an email. Use `--no-deliver` to skip this prompt and save the feedback locally only.

## Session Correlation

Every feedback entry is automatically linked to the most recent bivvy session,
making it easy to trace what you were doing when you noticed the issue.

## Examples

### Save feedback without the delivery prompt

```bash
bivvy feedback --no-deliver "The install step is confusing"
```

### Attach feedback to a specific session

```bash
bivvy feedback --session abc123 "Config merge was unexpected"
```

### List in-progress feedback

```bash
bivvy feedback list --status inprogress
```
