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
| Windows | x64 |

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
