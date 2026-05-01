---
title: Secret Masking
description: Configuring automatic secret masking in Bivvy output
---

# Secret Masking

Bivvy automatically masks sensitive values in all output to prevent
accidental exposure of secrets. Whenever an environment variable's
**name** matches a secret pattern, its **value** is registered with
the output masker and replaced with `[REDACTED]` everywhere it
appears in command output, logs, status messages, and error reports.

## Built-in Patterns

By default, Bivvy treats environment variables as secrets when their
name matches any of the following case-insensitive patterns. The
`*` placeholder represents zero or more characters (including an
optional underscore separator).

| Pattern name | Matches names like (case-insensitive) |
|---|---|
| `api_key` | `API_KEY`, `APIKEY`, `GITHUB_API_KEY`, `MY_APIKEY` |
| `secret` | `SECRET`, `SECRET_KEY`, `AWS_SECRET`, `MY_APP_SECRET` |
| `token` | `TOKEN`, `ACCESS_TOKEN`, `AUTH_TOKEN`, `GITHUB_TOKEN` |
| `password` | `PASSWORD`, `PASSWD`, `PWD`, `DB_PASSWORD`, `MYSQL_PWD` |
| `credential` | `CREDENTIAL`, `VENDOR_CREDENTIAL` |
| `private_key` | `PRIVATE_KEY`, `SSH_PRIVATE_KEY` |
| `connection_string` | `CONNECTION_STRING`, `DATABASE_URL`, `*_DATABASE_URL` |

Match is on the variable **name** only — Bivvy does not inspect the
value or attempt entropy analysis. Each pattern matches case-
insensitively, with an optional underscore between any prefix and
the keyword (so both `API_KEY` and `APIKEY` match).

## Custom Secret Patterns

Add additional environment variable names to treat as secrets via
`settings.secret_env`:

```yaml
settings:
  secret_env:
    - MY_CUSTOM_SECRET
    - INTERNAL_API_TOKEN
    - VENDOR_CREDENTIALS
```

Custom entries are matched as **exact names** — no wildcards, globs,
or substring matching. `MY_CUSTOM_SECRET` matches only the literal
variable named `MY_CUSTOM_SECRET`. To match a family of names, add
each one explicitly or rely on the built-in patterns above.

## How Masking Works

When Bivvy executes a step, it walks the merged environment for the
step, identifies any keys matching the patterns above (built-in or
custom), and registers their **values** with the output masker. The
masker then scans every byte of subsequent output and replaces every
occurrence of those values with `[REDACTED]`:

```
$ bivvy run
Setting DATABASE_URL=[REDACTED]
Running migration with API_KEY=[REDACTED]
```

Masking operates on the value, not the variable name, so a secret
that leaks into command output (for example, by being echoed,
written to a log line, or included in a stack trace) is still
redacted.

## Masking in Different Contexts

Secrets are masked in:

1. **Command output** — stdout and stderr from executed steps
2. **Log files** — when JSONL event logging is enabled
3. **Status messages** — progress and status output
4. **Error and telemetry reports** — failure summaries and
   diagnostic-funnel input

## Sensitive Steps

For steps that handle particularly sensitive data, you can mark them
as sensitive to receive additional protection. See the
[Sensitive Steps](steps.md#sensitive-steps) section of the steps
configuration page.
