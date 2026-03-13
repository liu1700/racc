#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARIES_DIR="$SCRIPT_DIR/../src-tauri/binaries"

mkdir -p "$BINARIES_DIR"

# Detect current platform and build only for it (dev mode)
# Cross-platform builds happen in CI
ARCH=$(uname -m)
OS=$(uname -s)

if [ "$OS" = "Linux" ]; then
  if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    TARGET="bun-linux-arm64"
    SUFFIX="aarch64-unknown-linux-gnu"
  else
    TARGET="bun-linux-x64"
    SUFFIX="x86_64-unknown-linux-gnu"
  fi
elif [ "$OS" = "Darwin" ]; then
  if [ "$ARCH" = "arm64" ]; then
    TARGET="bun-darwin-arm64"
    SUFFIX="aarch64-apple-darwin"
  else
    TARGET="bun-darwin-x64"
    SUFFIX="x86_64-apple-darwin"
  fi
else
  echo "Unsupported platform: $OS"
  exit 1
fi

echo "Building racc-assistant for $TARGET..."
cd "$SCRIPT_DIR"
bun build --compile --target="$TARGET" src/index.ts --outfile "$BINARIES_DIR/racc-assistant-$SUFFIX"
echo "Built: $BINARIES_DIR/racc-assistant-$SUFFIX"
