---
title: Failure Diagnostics
description: How Bivvy analyzes step failures and suggests fixes
---

# Failure Diagnostics

When a step fails during `bivvy run`, Bivvy does more than show the raw error
output. It analyzes the failure, identifies what went wrong, and suggests
concrete fixes you can apply without leaving the terminal.

## What Happens When a Step Fails

When a step exits with a non-zero status, Bivvy:

1. Displays the command output in a bordered error block
2. Analyzes the output to identify the type of failure
3. Shows a hint below the error block when it recognizes the problem
4. Presents a recovery menu with ranked fix options

Here is an example of what this looks like when a database step fails due to a
version mismatch:

```
  ✗ db_prepare failed

    ┌─ Command ──────────────────────────
    │ rails db:prepare
    ├─ Output ───────────────────────────
    │ pg_dump: error: server version: 16.13; pg_dump version: 14.21
    │ pg_dump: error: aborting because of server version mismatch
    └────────────────────────────────────

    Hint: pg_dump version 14.21 does not match server version 16.13 — update PATH

  ? How do you want to proceed?
  > Fix — add postgresql@16/bin to PATH
    Fix — brew install postgresql@16
    Suggestion — update PATH to point to the correct version
    Fix (custom) — enter your own command
    Skip
    Shell
    Abort
```

## Error Categories

Bivvy recognizes these categories of failure:

| Category | Examples |
|----------|----------|
| **Not found** | Missing commands, packages, modules, databases |
| **Connection refused** | Database or service not running |
| **Version mismatch** | Tool version does not match server/runtime |
| **Sync issue** | Lock file out of date with manifest |
| **Permission denied** | Script not executable, file access denied |
| **Port conflict** | Address already in use |
| **Build failure** | Native extension compilation errors |
| **System constraint** | Externally managed Python environment (PEP 668) |
| **Auth failure** | SSH key issues, expired tokens |
| **Resource limit** | Disk full, rate limit exceeded |

Bivvy does not require you to configure these categories. It detects them
automatically from the command output, regardless of which tool produced the
error.

## Recovery Menu

When Bivvy identifies a failure, the recovery menu offers several options:

- **Fix** -- A concrete command Bivvy is confident will resolve the issue.
  Up to three fix options may appear, ranked by confidence.
- **Suggestion** -- A lower-confidence fix that may help. Up to two
  suggestions appear below the fixes.
- **Fix (custom)** -- Enter your own command to run before retrying.
- **Retry** -- Re-run the step as-is.
- **Skip** -- Skip this step and continue with the workflow.
- **Shell** -- Open a debug shell in the project directory to investigate
  manually. When you exit the shell, the recovery menu appears again.
- **Abort** -- Stop the workflow entirely.

### Where Fixes Come From

Bivvy draws fix suggestions from two sources:

1. **The tool's own output.** Many tools print resolution hints like
   "Try \`brew install postgresql@16\`" or "Run \`bundle update\` to fix this."
   Bivvy extracts these commands and ranks them based on how well they align
   with the diagnosed problem.

2. **Heuristic deduction.** When the tool output does not include a fix, Bivvy
   generates suggestions based on the error category and your step configuration.
   For example, if a step that requires `postgres-server` gets a "connection
   refused" error, Bivvy suggests `brew services start postgresql` (macOS) or
   `systemctl start postgresql` (Linux).

Fixes from the tool's own output rank higher than heuristic suggestions, because
the tool knows its own error conditions best.

### Fix History

If you apply a fix and the step still fails, Bivvy remembers what you tried.
The same fix will not be offered again in the current run. Instead, you will see
a warning:

```
    ⚠ Previous fix `brew install postgresql@16` did not resolve this error.
```

This prevents you from running the same unsuccessful fix in a loop.

## Workflow-Aware Suggestions

Bivvy uses your workflow context to make smarter suggestions:

- If an **install step was skipped** earlier and a later step fails with "not
  found," Bivvy suggests re-running the skipped install step.
- If an **install step succeeded** but a dependency is still missing, Bivvy
  suggests checking your dependency manifest rather than re-installing.
- If a **service-start step succeeded** but a connection is refused, Bivvy
  notes that the service may have crashed and suggests checking its logs.

This context comes from the `depends_on` and `requires` fields in your
configuration. The more structure you provide, the better the suggestions.

## Supported Ecosystems

Bivvy includes built-in error patterns for the following ecosystems. These
patterns fire automatically based on the step command — no configuration is
needed.

| Ecosystem | Tools covered | Example diagnostics |
|-----------|---------------|---------------------|
| **Ruby** | Bundler, Gem | Native extension failures, version conflicts, missing gems |
| **Node.js** | npm, Yarn, Corepack | Missing modules, peer dependency conflicts, OpenSSL errors |
| **Python** | pip, Poetry, venv | Missing modules, externally-managed environments (PEP 668) |
| **Rust** | Cargo, rustup | Linker not found, pkg-config, lock file out of date, missing toolchains |
| **Go** | go build, go mod | Missing go.sum entries, checksum mismatches |
| **Java** | Gradle, Maven | JAVA_HOME not set, wrapper permissions |
| **.NET** | dotnet CLI, NuGet | SDK not found, restore failures |
| **Elixir** | Mix | Missing dependencies, compilation errors |
| **Docker** | Docker, Compose | Daemon not running, port conflicts, missing networks |
| **PostgreSQL** | psql, pg_dump | Connection refused, missing roles, missing databases |
| **Redis** | redis-cli | Connection refused |
| **Rails** | rails CLI | Pending migrations, database not created, credentials |

General patterns (command not found, permission denied, SSL certificate errors,
Git SSH authentication) apply to all steps regardless of ecosystem.

## Configuration

The diagnostic system is enabled by default. You can control it in two ways:

### In Your Config File

```yaml
settings:
  diagnostic_funnel: false   # Disable diagnostics, use legacy pattern matching
```

### With CLI Flags

```bash
# Force diagnostics on (overrides config)
bivvy run --diagnostic-funnel

# Force diagnostics off (overrides config)
bivvy run --no-diagnostic-funnel
```

### Using `requires` for Better Diagnostics

Adding `requires` to steps that depend on external services helps Bivvy produce
more targeted fix suggestions:

```yaml
steps:
  db_setup:
    command: "rails db:create"
    requires:
      - postgres-server
```

With this configuration, a "connection refused" error will prompt Bivvy to
suggest starting PostgreSQL specifically, rather than a generic "start the
required service" message.

## Non-Interactive Mode

In non-interactive mode (`--non-interactive`), Bivvy cannot present the recovery
menu. Instead, it:

- Displays the error block with the command output
- Shows the diagnostic hint below the error block (if one was identified)
- Exits with code 1

This makes the diagnostic hint useful in CI logs even when interactive recovery
is not available.

## Verbose Output

With `--verbose`, Bivvy streams step output in real time. When a step fails,
the error block is not repeated (since you already saw the output), but the
diagnostic hint and recovery menu still appear.
