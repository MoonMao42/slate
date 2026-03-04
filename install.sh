#!/bin/sh
set -e

REPO="MoonMao42/slate-dev"
INSTALL_DIR="${SLATE_INSTALL_DIR:-/usr/local/bin}"

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
  arm64|aarch64) ARCH="aarch64" ;;
  x86_64)        ARCH="x86_64" ;;
  *)
    echo "Error: unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

PLATFORM="apple-darwin"
ASSET="slate-${ARCH}-${PLATFORM}.tar.gz"
URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

echo "Installing slate for ${ARCH}-${PLATFORM}..."

# Download and extract
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if ! curl -fsSL "$URL" -o "$TMPDIR/$ASSET"; then
  echo "Error: failed to download $URL" >&2
  echo "Check https://github.com/${REPO}/releases for available binaries." >&2
  exit 1
fi

tar xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

# Install
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMPDIR/slate" "$INSTALL_DIR/slate"
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo mv "$TMPDIR/slate" "$INSTALL_DIR/slate"
fi

chmod +x "$INSTALL_DIR/slate"

echo "Installed slate to $INSTALL_DIR/slate"
echo "Run 'slate setup' to get started."
