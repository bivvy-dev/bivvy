---
title: Install via pip
description: Installing Bivvy with pip
---

# Install via pip

Install Bivvy using pip.

## Installation

```bash
pip install bivvy
```

## Requirements

- Python 3.8 or later

## Supported Platforms

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |

> **Windows is not yet supported via pip.** The pip wrapper downloads
> a `.tar.gz` archive on first run, but the Windows release artifact is
> distributed as a `.zip`. Windows users should install Bivvy under
> [WSL](https://learn.microsoft.com/windows/wsl/) or use one of the
> [other installation methods](./index.md).

## How It Works

The pip package downloads the native Bivvy binary on first run and
provides a Python wrapper that executes it. The binary is cached in
the package directory for subsequent runs.

> **Version pinning.** The pip wrapper currently pins the binary version
> it downloads to a specific release embedded in the package, which may
> lag behind the latest Bivvy release. Running `pip install --upgrade
> bivvy` will pick up a newer wrapper (and the binary it pins) only when
> a new wrapper is published. If you need the very latest Bivvy, use the
> [curl installer](./curl.md), [Cargo](./cargo.md), or download a
> [release binary](https://github.com/bivvy-dev/bivvy/releases) directly.

## Virtual Environments

You can install Bivvy in a virtual environment:

```bash
python -m venv .venv
source .venv/bin/activate
pip install bivvy
```

## Updating

```bash
pip install --upgrade bivvy
```

This upgrades the wrapper package. The Bivvy binary version that gets
downloaded is determined by the wrapper, not by `pip` (see [How It
Works](#how-it-works)).

## Uninstalling

```bash
pip uninstall bivvy
```
