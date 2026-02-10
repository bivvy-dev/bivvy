---
title: Releasing Bivvy
description: How to create and publish new Bivvy releases
---

# Releasing Bivvy

## Quick Release

```bash
./scripts/release.sh 1.0.0
git push && git push --tags
```

## Manual Release Process

1. Update version numbers:
   ```bash
   ./scripts/update-versions.sh 1.0.0
   ```

2. Update CHANGELOG.md

3. Commit and tag:
   ```bash
   git add -A
   git commit -m "Bump version to 1.0.0"
   git tag -a v1.0.0 -m "Release v1.0.0"
   git push && git push --tags
   ```

4. GitHub Actions will automatically:
   - Build binaries for all platforms
   - Create GitHub release with binaries
   - Publish to npm
   - Publish to PyPI

5. After release, update Homebrew SHA256 hashes:
   ```bash
   ./scripts/update-homebrew-sha.sh 1.0.0
   ```

## Supported Platforms

| Platform | Architecture |
|----------|--------------|
| Linux | x64, arm64 |
| macOS | x64, arm64 |
| Windows | x64 |

## Secrets Required

Set these in GitHub repository settings:
- `NPM_TOKEN` - npm access token
- `PYPI_TOKEN` - PyPI API token
