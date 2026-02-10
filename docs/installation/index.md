---
title: Installation
description: How to install Bivvy
---

# Installation

Bivvy can be installed via several methods. All methods install the same
native binary -- choose whichever fits your workflow.

## Quick Install (Recommended)

```bash
curl -fsSL https://bivvy.dev/install.sh | sh
```

See [curl install docs](./curl.md) for options and troubleshooting.

## Other Methods

| Method | Command | Notes |
|--------|---------|-------|
| [Homebrew](./homebrew.md) | `brew install bivvy-dev/bivvy/bivvy` | macOS/Linux, auto-updates |
| [Cargo](./cargo.md) | `cargo install bivvy` | Requires Rust 1.93+ |
| [npm](./npm.md) | `npm install -g bivvy` | Node.js 14+ |
| [gem](./gem.md) | `gem install bivvy` | Ruby 2.7+ |
| [pip](./pip.md) | `pip install bivvy` | Python 3.8+ |

## Supported Platforms

| Platform | Architecture |
|----------|--------------|
| Linux | x64, arm64 |
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Windows | x64 |

## Verify Installation

After installing, verify it works:

```bash
bivvy --version
```

## Next Steps

Initialize configuration in your project:

```bash
bivvy init
```

Run setup:

```bash
bivvy run
```
