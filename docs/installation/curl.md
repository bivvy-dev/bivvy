---
title: Install via curl
description: Installing Bivvy with the curl installer
---

# Install via curl

The quickest way to install Bivvy on macOS or Linux.

## Usage

```bash
curl -fsSL https://bivvy.dev/install | sh
```

## What the Script Does

1. Detects your OS and CPU architecture
2. Downloads the matching binary from GitHub Releases
3. Installs to `~/.local/bin` (or your custom directory)
4. Verifies the installation

## Supported Platforms

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |
| Windows (WSL) | x64 |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BIVVY_VERSION` | `latest` | Version to install (e.g., `v1.0.0`) |
| `BIVVY_INSTALL_DIR` | `~/.local/bin` | Installation directory |

## Examples

Install latest:

```bash
curl -fsSL https://bivvy.dev/install | sh
```

Install specific version:

```bash
BIVVY_VERSION=v1.0.0 curl -fsSL https://bivvy.dev/install | sh
```

Install to custom directory:

```bash
BIVVY_INSTALL_DIR=/usr/local/bin curl -fsSL https://bivvy.dev/install | sh
```

## PATH Setup

If `~/.local/bin` is not in your PATH, the script will tell you. Add this
to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.):

```bash
export PATH="$PATH:$HOME/.local/bin"
```

## Uninstalling

```bash
rm ~/.local/bin/bivvy
```

## Troubleshooting

**"Unsupported operating system"** or **"Unsupported architecture"**
: Your platform is not supported. See [Supported Platforms](#supported-platforms).

**"Could not find release for platform"**
: The version you requested may not exist. Check [GitHub Releases](https://github.com/bivvy-dev/bivvy/releases).

**Permission denied**
: You may need to create the install directory first: `mkdir -p ~/.local/bin`
