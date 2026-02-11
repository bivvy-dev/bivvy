#!/bin/bash
# Create and publish a new release
#
# Usage: ./scripts/release.sh 1.2.0
#
# This script handles the entire release process:
# 1. Updates version numbers across all packages
# 2. Updates CHANGELOG.md (moves [Unreleased] to new version)
# 3. Runs full verification (fmt, clippy, tests, build)
# 4. Commits, tags, and pushes
# 5. GitHub Actions handles the rest (build, publish, Homebrew update)

set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.2.0"
    exit 1
fi

# Validate version format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Error: Version must be in format X.Y.Z"
    exit 1
fi

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Error: You have uncommitted changes"
    exit 1
fi

echo "Preparing release $VERSION"
echo ""

# Update versions
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/update-versions.sh" "$VERSION"

# Update CHANGELOG
CHANGELOG="CHANGELOG.md"
TODAY=$(date +%Y-%m-%d)
PREV_VERSION=$(grep -oP '(?<=\[)[0-9]+\.[0-9]+\.[0-9]+(?=\])' "$CHANGELOG" | head -1)

# Replace [Unreleased] section header, add new version, update links
sed -i '' "s/## \[Unreleased\]/## [Unreleased]\n\n## [$VERSION] - $TODAY/" "$CHANGELOG"

# Update the [Unreleased] compare link and add new version link
if [ -n "$PREV_VERSION" ]; then
    sed -i '' "s|\[Unreleased\]: .*|[Unreleased]: https://github.com/bivvy-dev/bivvy/compare/$VERSION...HEAD|" "$CHANGELOG"
    # Add version link before the previous version link
    sed -i '' "/\[$PREV_VERSION\]: /i\\
[$VERSION]: https://github.com/bivvy-dev/bivvy/compare/$PREV_VERSION...$VERSION" "$CHANGELOG"
else
    sed -i '' "s|\[Unreleased\]: .*|[Unreleased]: https://github.com/bivvy-dev/bivvy/compare/$VERSION...HEAD\n[$VERSION]: https://github.com/bivvy-dev/bivvy/releases/tag/$VERSION|" "$CHANGELOG"
fi

echo "Updated CHANGELOG.md"

# Verify build
echo ""
echo "Running verification..."
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --all-targets --all-features

# Commit, tag, and push
echo ""
echo "Committing and pushing..."
git add -A
git commit -m "Release $VERSION"
git tag "$VERSION"
git push && git push --tags

echo ""
echo "Release $VERSION published!"
echo "GitHub Actions will now build, publish packages, and update Homebrew."
