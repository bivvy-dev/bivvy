#!/bin/bash
# Create a new release

set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.0.0"
    exit 1
fi

# Validate version format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Error: Version must be in format X.Y.Z"
    exit 1
fi

echo "Preparing release $VERSION"
echo ""

# Check for uncommitted changes
if ! git diff --quiet; then
    echo "Error: You have uncommitted changes"
    exit 1
fi

# Update versions
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/update-versions.sh" "$VERSION"

# Verify build
echo ""
echo "Running verification..."
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --all-targets --all-features

# Commit version changes
echo ""
echo "Committing version changes..."
git add -A
git commit -m "Bump version to $VERSION"

# Create tag
echo ""
echo "Creating tag $VERSION..."
git tag -a "$VERSION" -m "Release $VERSION"

echo ""
echo "Release $VERSION prepared!"
echo ""
echo "To publish:"
echo "  git push && git push --tags"
