#!/bin/bash
# Update version numbers across all distribution packages

set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

echo "Updating versions to $VERSION"

# Cargo.toml
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
echo "Updated Cargo.toml"

# npm package
if [ -f dist/npm/package.json ]; then
    cd dist/npm
    npm version "$VERSION" --no-git-tag-version --allow-same-version 2>/dev/null
    cd ../..
    echo "Updated dist/npm/package.json"
fi

# Python package
if [ -f dist/pip/bivvy/__init__.py ]; then
    sed -i '' "s/__version__ = \".*\"/__version__ = \"$VERSION\"/" dist/pip/bivvy/__init__.py
    sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" dist/pip/pyproject.toml
    echo "Updated dist/pip/"
fi

# Ruby gem (VERSION file)
if [ -f dist/gem/VERSION ]; then
    printf '%s\n' "$VERSION" > dist/gem/VERSION
    echo "Updated dist/gem/VERSION"
fi

# Homebrew formula
if [ -f dist/homebrew/bivvy.rb ]; then
    sed -i '' "s/version \".*\"/version \"$VERSION\"/" dist/homebrew/bivvy.rb
    echo "Updated dist/homebrew/bivvy.rb"
fi

echo ""
echo "Done! Don't forget to:"
echo "  1. Update CHANGELOG.md"
echo "  2. Commit changes"
echo "  3. Create git tag: git tag $VERSION"
echo "  4. Push: git push && git push --tags"
echo "  5. Update Homebrew SHA256 hashes after release"
