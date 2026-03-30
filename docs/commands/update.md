---
title: bivvy update
description: Check for and install updates
---

# bivvy update

Check for new versions and install updates.

## Usage

```bash
bivvy update
```

```bash
bivvy update --check
```

## Flags

| Flag | Description |
|------|-------------|
| `--check` | Check for updates without installing |
| `--enable-auto-update` | Enable automatic background updates |
| `--disable-auto-update` | Disable automatic background updates |

## Examples

Check for updates and install if available:

```bash
bivvy update
```

Check for a new version without installing:

```bash
bivvy update --check
```

Turn off automatic updates:

```bash
bivvy update --disable-auto-update
```

Turn them back on:

```bash
bivvy update --enable-auto-update
```

## Automatic Background Updates

By default, bivvy updates itself automatically in the background. No
commands or prompts are needed — when a new version is released, it is
installed silently and takes effect on your next run.

### How It Works

1. After each command completes, bivvy spawns a lightweight background
   process to check for new releases.
2. If a new version is available, the background process installs it
   using the same method you originally used to install bivvy.
3. On your next run, you're on the latest version. If the update
   required staging a new binary (manual installs), bivvy shows a short
   message confirming the version change.

### Install Method Behavior

How the background update is applied depends on how you installed bivvy:

| Install method | What happens |
|---------------|--------------|
| **Homebrew** | Runs `brew upgrade bivvy` in the background |
| **Cargo** | Runs `cargo install bivvy --force` in the background |
| **Manual download** | Downloads the correct platform binary from GitHub releases and stages it; the swap happens on your next run |

### When Auto-Update Is Skipped

Background updates are skipped when:

- Running in CI (detected via `CI`, `GITHUB_ACTIONS`, etc.)
- Running in non-interactive mode (`--non-interactive`)
- Auto-update is disabled (see below)
- A background update is already in progress
- An update is already staged and waiting to be applied

### Disabling Auto-Update

If you prefer to update manually, disable automatic updates:

```bash
bivvy update --disable-auto-update
```

Or set it directly in your system config (`~/.bivvy/config.yml`):

```yaml
settings:
  auto_update: false
```

You can always update manually with `bivvy update` regardless of this
setting.

## See Also

- [Settings](../configuration/settings.md) — `auto_update` setting reference
