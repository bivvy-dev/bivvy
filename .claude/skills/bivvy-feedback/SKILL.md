# Bivvy Feedback Skill

Process and act on bivvy feedback captured during dogfooding.

## Triggers

Use this skill when:
- User says "/bivvy-feedback" or "check bivvy feedback"
- User asks to "fix bivvy issues" or "triage feedback"
- User mentions "what's annoying about bivvy"

## Context

Bivvy captures feedback via `bivvy feedback "message"` which stores entries in
`~/.local/share/bivvy/feedback.jsonl`. Each entry links to a session that
contains full context: command run, output, errors, timing.

## Workflow

### 1. List Open Feedback

Run:
```bash
bivvy feedback list
```

This shows all open feedback entries with IDs, messages, tags, and session references.

### 2. Get Session Context

For each feedback entry, get the linked session details:
```bash
# Find the session file
ls ~/.local/share/bivvy/sessions/ | grep <session_id>

# Read session context
cat ~/.local/share/bivvy/sessions/<session_id>.json
```

The session contains:
- `command`: What command was run (run, list, status, etc.)
- `args`: Command arguments
- `stdout`/`stderr`: Captured output
- `exit_code`: Success/failure
- `context.step_results`: For run command, which steps passed/failed
- `context.errors`: Any errors that occurred

### 3. Analyze and Prioritize

Group feedback by:
- **UX issues**: Error messages, confusing output, missing information
- **Bugs**: Actual incorrect behavior
- **Performance**: Slow operations
- **Missing features**: Things that should exist but don't

Prioritize by:
1. Frequency (multiple feedback about same issue)
2. Severity (blocks usage vs. minor annoyance)
3. Ease of fix (quick wins vs. architectural changes)

### 4. Fix Issues

For each issue to fix:

1. Read the session context to understand exactly what happened
2. Find the relevant code using the error messages or command path
3. Implement the fix following bivvy's TDD workflow
4. Mark feedback as resolved:
   ```bash
   bivvy feedback resolve <fb_id> --note "Fixed in commit <hash>"
   ```

### 5. Summary Report

After processing, provide a summary:
- How many issues triaged
- How many fixed
- What remains open and why

## Commands

| Command | Description |
|---------|-------------|
| `bivvy feedback list` | List open feedback |
| `bivvy feedback list --all` | List all feedback including resolved |
| `bivvy feedback list --tag <tag>` | Filter by tag |
| `bivvy feedback resolve <id> --note "..."` | Mark as resolved |
| `bivvy feedback session <id>` | Show feedback for a session |

## File Locations

| Path | Contents |
|------|----------|
| `~/.local/share/bivvy/feedback.jsonl` | Feedback entries (JSONL) |
| `~/.local/share/bivvy/sessions/*.json` | Session records |

## Example Session

```json
{
  "id": "sess_1706789012345_a1b2c3d4e5f6g7h8",
  "metadata": {
    "command": "run",
    "args": ["--verbose"],
    "cwd": "/Users/brenna/myproject",
    "start_time": "2024-02-01T10:30:12Z",
    "end_time": "2024-02-01T10:30:45Z",
    "exit_code": 1,
    "stdout": "...",
    "stderr": "Error: bundle install failed\n...",
    "context": {
      "workflow": "default",
      "step_results": [
        {"name": "brew", "status": "success", "duration_ms": 1200},
        {"name": "ruby_deps", "status": "failed", "error": "bundle install failed"}
      ],
      "errors": ["Step ruby_deps failed: bundle install returned exit code 1"]
    }
  }
}
```

## Example Feedback Entry

```json
{
  "id": "fb_a1b2c3d4e5f6",
  "timestamp": "2024-02-01T10:31:00Z",
  "message": "the error didn't tell me WHICH gem failed to install",
  "tags": ["ux", "error-messages"],
  "session_id": "sess_1706789012345_a1b2c3d4e5f6g7h8",
  "status": "open"
}
```

## Tips

- Session files contain the full stdout/stderr - use these to see exactly what the user saw
- The `config_hash` in sessions helps identify if config changed between runs
- Multiple feedback entries may point to the same root cause
- Check git history of the relevant code to understand recent changes
