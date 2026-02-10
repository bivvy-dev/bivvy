---
title: Install via Homebrew
description: Installing Bivvy with Homebrew
---

# Install via Homebrew

The recommended way to install Bivvy on macOS.

## Installation

```bash
brew install https://raw.githubusercontent.com/bivvy-dev/bivvy/main/dist/homebrew/bivvy.rb
```

## Supported Platforms

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |

## Updating

```bash
brew upgrade bivvy
```

## Uninstalling

```bash
brew uninstall bivvy
```

## Shell Completions

Homebrew automatically installs shell completions. They should work
immediately in new shell sessions.

If completions aren't working, ensure your shell is configured to
load Homebrew completions. See `brew info` for details.
