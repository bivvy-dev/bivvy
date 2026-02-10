---
title: Install via npm
description: Installing Bivvy with npm
---

# Install via npm

Install Bivvy using npm, yarn, or pnpm.

## Installation

npm:

```bash
npm install -g bivvy
```

yarn:

```bash
yarn global add bivvy
```

pnpm:

```bash
pnpm add -g bivvy
```

## Requirements

- Node.js 14 or later
- curl (used to download the native binary)

## Supported Platforms

| OS | Architecture |
|----|--------------|
| macOS | x64 (Intel), arm64 (Apple Silicon) |
| Linux | x64, arm64 |
| Windows | x64 |

## How It Works

The npm package is a thin wrapper that:
1. Downloads the native Bivvy binary on `npm install` (via a postinstall script)
2. Provides a Node.js wrapper at `bin/bivvy` that spawns the native binary

This gives you the performance of a native binary with the convenience of npm.

## Updating

```bash
npm update -g bivvy
```

## Uninstalling

```bash
npm uninstall -g bivvy
```
