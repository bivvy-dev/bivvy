---
title: Secret Masking
description: Configuring automatic secret masking in Bivvy output
---

# Secret Masking

Bivvy automatically masks sensitive values in all output to prevent
accidental exposure of secrets.

## Built-in Patterns

By default, Bivvy recognizes environment variables matching these patterns:

- `*_API_KEY`, `*_APIKEY` - API keys
- `*_SECRET`, `*_SECRET_KEY` - Secrets
- `*_TOKEN`, `*_ACCESS_TOKEN`, `*_AUTH_TOKEN` - Tokens
- `*_PASSWORD`, `*_PASSWD`, `*_PWD` - Passwords
- `*_CREDENTIAL` - Credentials
- `*_PRIVATE_KEY` - Private keys
- `*_CONNECTION_STRING`, `*_DATABASE_URL` - Connection strings

## Custom Secret Patterns

Add custom environment variable names to treat as secrets:

```yaml
settings:
  secret_env:
    - MY_CUSTOM_SECRET
    - INTERNAL_API_TOKEN
    - VENDOR_CREDENTIALS
```

## How Masking Works

When Bivvy outputs any text (logs, command output, status messages),
it replaces actual secret values with `[REDACTED]`:

```
$ bivvy run
Setting DATABASE_URL=[REDACTED]
Running migration with API_KEY=[REDACTED]
```

## Masking in Different Contexts

Secrets are masked in:

1. **Command output** - stdout and stderr from executed steps
2. **Log files** - When logging is enabled
3. **Status messages** - Progress and status output
4. **Error messages** - Stack traces and error details

## Sensitive Steps

For steps that handle particularly sensitive data, you can mark them
as sensitive to receive additional protection. See the
[Sensitive Steps](../steps.md#sensitive-steps) documentation.
