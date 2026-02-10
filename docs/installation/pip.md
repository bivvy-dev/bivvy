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
| Windows | x64 |

## How It Works

The pip package downloads the native Bivvy binary on first run and
provides a Python wrapper that executes it. The binary is cached in
the package directory for subsequent runs.

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

## Uninstalling

```bash
pip uninstall bivvy
```
