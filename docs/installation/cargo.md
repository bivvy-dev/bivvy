---
title: Install via Cargo
description: Installing Bivvy with Cargo
---

# Install via Cargo

Install Bivvy using Rust's package manager. Best for Rust developers who
already have the toolchain installed.

## Installation

```bash
cargo install bivvy
```

## Requirements

- Rust 1.93 or later

## From Source

```bash
git clone https://github.com/bivvy-dev/bivvy
cd bivvy
cargo install --path .
```

## Updating

```bash
cargo install bivvy --force
```

## Uninstalling

```bash
cargo uninstall bivvy
```

## Supported Platforms

Any platform supported by Rust, including:

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |
| Windows | x64 |
