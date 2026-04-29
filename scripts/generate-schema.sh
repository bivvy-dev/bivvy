#!/bin/bash
# Regenerate the embedded JSON Schema from the current config types.
#
# Run this whenever you modify config types in src/config/schema.rs,
# or it will be run automatically by scripts/release.sh.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cargo run --quiet -- schema -o "$PROJECT_ROOT/generated/schema.json"
echo "Regenerated generated/schema.json"
