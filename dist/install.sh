#!/bin/sh
# Bivvy installer script
# Usage: curl -fsSL https://bivvy.dev/install | sh

set -e

BIVVY_VERSION="${BIVVY_VERSION:-latest}"
BIVVY_INSTALL_DIR="${BIVVY_INSTALL_DIR:-$HOME/.local/bin}"
GITHUB_REPO="bivvy-dev/bivvy"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() {
    printf "${GREEN}info${NC}: %s\n" "$1"
}

warn() {
    printf "${YELLOW}warn${NC}: %s\n" "$1"
}

error() {
    printf "${RED}error${NC}: %s\n" "$1"
    exit 1
}

detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux*)  OS="linux" ;;
        Darwin*) OS="darwin" ;;
        MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
        *) error "Unsupported operating system: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64) ARCH="x64" ;;
        arm64|aarch64) ARCH="arm64" ;;
        *) error "Unsupported architecture: $ARCH" ;;
    esac

    PLATFORM="${OS}-${ARCH}"
    info "Detected platform: $PLATFORM"
}

get_download_url() {
    if [ "$BIVVY_VERSION" = "latest" ]; then
        RELEASE_JSON=$(curl -sL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest")

        # Check if the API returned a valid response
        if echo "$RELEASE_JSON" | grep -q '"message".*"Not Found"'; then
            error "Could not access releases. The repository may be private or the URL may be incorrect."
        fi

        RELEASE_URL=$(echo "$RELEASE_JSON" \
            | grep "browser_download_url.*${PLATFORM}" \
            | cut -d '"' -f 4)
    else
        RELEASE_URL="https://github.com/${GITHUB_REPO}/releases/download/${BIVVY_VERSION}/bivvy-${PLATFORM}.tar.gz"
    fi

    if [ -z "$RELEASE_URL" ]; then
        error "Could not find release for platform: $PLATFORM. Check https://github.com/${GITHUB_REPO}/releases for available downloads."
    fi

    info "Download URL: $RELEASE_URL"
}

install() {
    info "Installing to $BIVVY_INSTALL_DIR"

    mkdir -p "$BIVVY_INSTALL_DIR"

    TEMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TEMP_DIR"' EXIT

    info "Downloading bivvy..."
    curl -fsSL "$RELEASE_URL" | tar -xz -C "$TEMP_DIR"

    mv "$TEMP_DIR/bivvy" "$BIVVY_INSTALL_DIR/bivvy"
    chmod +x "$BIVVY_INSTALL_DIR/bivvy"

    info "Installed bivvy to $BIVVY_INSTALL_DIR/bivvy"
}

check_path() {
    case ":$PATH:" in
        *":$BIVVY_INSTALL_DIR:"*) ;;
        *)
            warn "$BIVVY_INSTALL_DIR is not in your PATH"
            warn "Add this to your shell profile:"
            echo ""
            echo "  export PATH=\"\$PATH:$BIVVY_INSTALL_DIR\""
            echo ""
            ;;
    esac
}

verify() {
    if [ -x "$BIVVY_INSTALL_DIR/bivvy" ]; then
        info "Installation successful!"
        "$BIVVY_INSTALL_DIR/bivvy" --version
    else
        error "Installation verification failed"
    fi
}

main() {
    info "Bivvy Installer"
    echo ""

    detect_platform
    get_download_url
    install
    check_path
    verify

    echo ""
    info "Run 'bivvy --help' to get started (use -V for version)"
}

main
