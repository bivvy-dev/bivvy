---
title: Install via Homebrew
description: Installing Bivvy with Homebrew
---

# Install via Homebrew

A way to install Bivvy on macOS or Linux using Homebrew.

## Installation

```bash
brew install https://raw.githubusercontent.com/bivvy-dev/bivvy/main/dist/homebrew/bivvy.rb
```

This installs Bivvy directly from a formula URL. There is currently
**no Homebrew tap** for Bivvy.

## Supported Platforms

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |

## Updating

Because Bivvy is installed from a raw formula URL (not a tap), Homebrew
does **not** track it for updates. `brew upgrade` and `brew upgrade bivvy`
will not pick up new releases.

To upgrade, re-run the install command. It will fetch and install the
latest formula:

```bash
brew install --force https://raw.githubusercontent.com/bivvy-dev/bivvy/main/dist/homebrew/bivvy.rb
```

If `brew install` complains that the formula is already installed, run
`brew unlink bivvy` first, or use one of the [other installation
methods](./index.md) for a more conventional upgrade flow.

## Uninstalling

Homebrew may not recognize the formula by name once it has been installed
from a URL. Try:

```bash
brew uninstall bivvy
```

If that fails with "no such keg", remove the binary and any installed
files manually. You can locate them with:

```bash
brew list bivvy 2>/dev/null || which bivvy
```

## Shell Completions

Homebrew installs shell completions alongside the binary. They should
work in new shell sessions.

If completions aren't working, ensure your shell is configured to load
Homebrew completions. See `brew info` for details.
