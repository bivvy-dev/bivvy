---
title: Install via gem
description: Installing Bivvy with RubyGems
---

# Install via gem

Install Bivvy using RubyGems.

## Installation

```bash
gem install bivvy
```

## Requirements

- Ruby 2.7 or later

## Supported Platforms

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |

> **Windows is not yet supported via gem.** The gem's native extension
> hook downloads a `.tar.gz` archive during installation, but the
> Windows release artifact is distributed as a `.zip`. Windows users
> should install Bivvy under [WSL](https://learn.microsoft.com/windows/wsl/)
> or use one of the [other installation methods](./index.md).

## How It Works

The gem downloads the native Bivvy binary during installation (via a
native extension hook) and provides a Ruby wrapper at `exe/bivvy` that
exec's the native binary.

## Updating

```bash
gem update bivvy
```

## Uninstalling

```bash
gem uninstall bivvy
```
