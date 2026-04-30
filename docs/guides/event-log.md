---
title: Event Log
description: How Bivvy records every session as structured JSONL on disk
---

# Event Log

Every Bivvy session can record what happened as a structured event log: one
JSON object per line, one file per run. The log is meant for after-the-fact
debugging, audits, and tooling that wants to inspect a run without scraping
terminal output.

It is enabled by default. Reading the log requires nothing more than `cat`,
`tail`, or `jq`.

## Where logs live

Logs are written to `~/.bivvy/logs/` (the user's home directory). One file is
created per session. Filenames have the form:

```
2026-04-25T10-00-00_<session-suffix>.jsonl
```

The leading timestamp is the session start time in UTC (with `:` replaced by
`-` so the name is filesystem-safe). The trailing suffix uniquely identifies
the session.

Each file is plain JSONL — every line is an independent JSON object. Files
remain after the session ends and are cleaned up later according to the
[retention policy](#retention).

## Enabling and disabling

Event logging is on by default. Toggle it in `.bivvy/config.yml`:

```yaml
settings:
  logging: true   # default — write JSONL logs to ~/.bivvy/logs/
```

To turn it off entirely:

```yaml
settings:
  logging: false  # no log files are written
```

When `logging: false`, no log files are created and the retention sweep does
not run. You can still see real-time output in the terminal; only the
on-disk record is suppressed.

## Retention

Bivvy expires old logs automatically. Cleanup runs at the start of each
session, before the new log file is created.

```yaml
settings:
  logging: true
  log_retention_days: 30   # default — delete files older than this
  log_retention_mb: 500    # default — total cap; oldest deleted first
```

| Field | Default | Behavior |
|-------|---------|----------|
| `log_retention_days` | `30` | Files older than this many days are deleted. |
| `log_retention_mb` | `500` | If the total size of the log directory exceeds this many megabytes, the oldest files are deleted until the total is back under the limit. |

Both limits apply on every run: age first, then size. Non-`.jsonl` files in
`~/.bivvy/logs/` are never touched.

## What a log line looks like

Every line has the same outer shape:

```json
{
  "ts": "2026-04-25T10:00:00.123Z",
  "session": "sess_1745575200_abcdef01",
  "type": "step_completed",
  "...": "type-specific fields"
}
```

| Field | Description |
|-------|-------------|
| `ts` | ISO 8601 UTC timestamp with milliseconds, when Bivvy emitted the event. |
| `session` | Unique session ID. The same session ID appears on every line of one file. |
| `type` | The event variant — see [event categories](#event-categories) below. |
| Other fields | Vary by `type`. Each event carries enough context to be read on its own. |

Two real lines from a small run:

```json
{"ts":"2026-04-25T10:00:00.012Z","session":"sess_1745575200_abcdef01","type":"session_started","command":"run","args":["--verbose"],"version":"1.10.0","working_directory":"/Users/me/code/myapp"}
{"ts":"2026-04-25T10:00:01.456Z","session":"sess_1745575200_abcdef01","type":"step_completed","name":"bundle_install","success":true,"exit_code":0,"duration_ms":1444}
```

## Event categories

Bivvy emits events for every meaningful moment of a run. The exact set is
defined in `src/logging/events.rs`; the broad shape is:

| Category | Example `type` values | When emitted |
|----------|----------------------|--------------|
| Session | `session_started`, `session_ended`, `config_loaded` | Once per run, plus when config is parsed. |
| Workflow | `workflow_started`, `workflow_completed` | Only when a workflow is executing (not for `lint`, `status`, etc.). |
| Step lifecycle | `step_planned`, `step_filtered_out`, `step_decided`, `step_starting`, `step_output`, `step_completed`, `step_skipped` | The full life of every step in the plan. |
| Decision signals | `check_evaluated`, `precondition_evaluated`, `satisfaction_evaluated`, `rerun_detected`, `dependency_blocked`, `requirement_gap` | The signals the [decision engine](auto-run.md) used to decide what to do with each step. |
| User interaction | `user_prompted`, `user_responded` | Any interactive prompt and its answer. |
| Snapshots | `baseline_established`, `baseline_updated`, `snapshot_captured` | Change-check baselines and explicit `bivvy snapshot` calls. |
| Recovery | `recovery_started`, `recovery_action_taken` | Failure analysis and the option the user picked from the recovery menu. |

You don't need to memorize the list. Reading a log file end-to-end will walk
you through exactly what the decision engine saw, what it decided, and what
ran — in order.

### Sensitive steps

Steps marked `sensitive: true` have their command output and any error
messages replaced with `"[REDACTED]"` or `"[SENSITIVE]"` in the log. The
event is still emitted (so timing and success are preserved), but the
content does not land on disk.

## Inspecting logs

Find the most recent log:

```bash
ls -t ~/.bivvy/logs/ | head -1
```

Tail it as it grows:

```bash
tail -f ~/.bivvy/logs/2026-04-25T10-00-00_abcdef01.jsonl
```

Pretty-print every event with `jq`:

```bash
jq . ~/.bivvy/logs/2026-04-25T10-00-00_abcdef01.jsonl
```

Show just the step results:

```bash
jq 'select(.type == "step_completed") | {name, success, exit_code, duration_ms}' \
  ~/.bivvy/logs/2026-04-25T10-00-00_abcdef01.jsonl
```

Find every failed step across every recent log:

```bash
jq -c 'select(.type == "step_completed" and .success == false)' \
  ~/.bivvy/logs/*.jsonl
```

Because each line is independent JSON, any tool that can read JSONL — `jq`,
`grep`, log shippers, your own scripts — works without setup.

## Logs and project scoping

`bivvy last` and `bivvy history` read this same log directory. They scope
their output to the current project by matching the `working_directory`
field on each file's `session_started` event. If you delete or move a
project's log files, `bivvy last` and `bivvy history` simply see a shorter
history; nothing else breaks.

`bivvy history --clear` removes only the log files belonging to the current
project — logs from other projects are left alone.

## Related

- [Settings](../configuration/settings.md) — the `logging`,
  `log_retention_days`, and `log_retention_mb` toggles.
- [Configuration Reference](../reference/config-reference.md) — full field
  table.
- [Failure Diagnostics](diagnostics.md) — the recovery flow whose decisions
  show up as `recovery_*` events in the log.
- [Auto-Run and the Decision Engine](auto-run.md) — explains the
  `*_evaluated` and `step_decided` events you'll see in the log.
