#!/bin/bash
# Update SHA256 hashes in Homebrew formula after release

set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

# Strip leading 'v' if present
VERSION="${VERSION#v}"

FORMULA="dist/homebrew/bivvy.rb"
GITHUB_REPO="bivvy-dev/bivvy"

sedi() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "$@"
    else
        sed -i "$@"
    fi
}

# Update version
sedi "s/version \".*\"/version \"${VERSION}\"/" "$FORMULA"

for platform in darwin-arm64 darwin-x64 linux-arm64 linux-x64; do
    asset="bivvy-${platform}.tar.gz"
    echo "Downloading $asset..."

    # Use gh CLI to download (works for private repos)
    tmpdir=$(mktemp -d)
    gh release download "v${VERSION}" --repo "${GITHUB_REPO}" --pattern "$asset" --dir "$tmpdir"
    sha=$(shasum -a 256 "$tmpdir/$asset" | cut -d ' ' -f 1)
    rm -rf "$tmpdir"

    # Each platform has a unique placeholder name
    placeholder="PLACEHOLDER_SHA256_$(echo "$platform" | tr '-' '_' | tr '[:lower:]' '[:upper:]')"

    # Try replacing the placeholder first (first release)
    if grep -q "$placeholder" "$FORMULA"; then
        sedi "s|${placeholder}|${sha}|" "$FORMULA"
    else
        # For subsequent releases, find the URL line and update the sha256 on the next line
        url_pattern="bivvy-${platform}.tar.gz"
        line_num=$(grep -n "$url_pattern" "$FORMULA" | head -1 | cut -d: -f1)
        if [ -n "$line_num" ]; then
            sha_line=$((line_num + 1))
            sedi "${sha_line}s/sha256 \".*\"/sha256 \"${sha}\"/" "$FORMULA"
        fi
    fi

    echo "Updated $platform: $sha"
done

echo "Formula updated!"
