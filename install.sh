#!/bin/sh
set -eu

REPO="${SLATE_REPO:-MoonMao42/slate-dev}"
VERSION="${SLATE_VERSION:-latest}"
BIN_NAME="${SLATE_BIN_NAME:-slate}"

detect_arch() {
  ARCH="${SLATE_ARCH_OVERRIDE:-$(uname -m)}"
  case "$ARCH" in
    arm64|aarch64) printf '%s\n' "aarch64" ;;
    x86_64|amd64) printf '%s\n' "x86_64" ;;
    *)
      echo "Error: unsupported architecture: $ARCH" >&2
      exit 1
      ;;
  esac
}

detect_platform() {
  OS_NAME="${SLATE_OS_OVERRIDE:-$(uname -s)}"
  case "$OS_NAME" in
    Darwin) printf '%s\n' "apple-darwin" ;;
    Linux) printf '%s\n' "unknown-linux-gnu" ;;
    *)
      echo "Error: unsupported operating system: $OS_NAME" >&2
      exit 1
      ;;
  esac
}

resolve_install_dir() {
  if [ -n "${SLATE_INSTALL_DIR:-}" ]; then
    printf '%s\n' "$SLATE_INSTALL_DIR"
    return
  fi

  if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
    printf '%s\n' "/usr/local/bin"
    return
  fi

  if [ -n "${HOME:-}" ]; then
    printf '%s\n' "${HOME}/.local/bin"
    return
  fi

  printf '%s\n' "/usr/local/bin"
}

sha256_hash() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    echo "" # signal: no hasher available
  fi
}

resolve_download_base() {
  if [ "$VERSION" = "latest" ]; then
    printf '%s\n' "https://github.com/${REPO}/releases/latest/download"
  else
    printf '%s\n' "https://github.com/${REPO}/releases/download/${VERSION}"
  fi
}

ARCH="$(detect_arch)"
PLATFORM="$(detect_platform)"
TARGET="${ARCH}-${PLATFORM}"
ASSET="${BIN_NAME}-${TARGET}.tar.gz"
URL="$(resolve_download_base)/${ASSET}"
INSTALL_DIR="$(resolve_install_dir)"

if [ "${SLATE_INSTALL_DRY_RUN:-0}" = "1" ]; then
  echo "TARGET_TRIPLE=${TARGET}"
  echo "ASSET=${ASSET}"
  echo "URL=${URL}"
  echo "INSTALL_DIR=${INSTALL_DIR}"
  exit 0
fi

echo "Installing slate for ${TARGET}..."

WORK_TMPDIR="$(mktemp -d "${TMPDIR:-/tmp}/slate-install.XXXXXX")"
trap 'rm -rf "$WORK_TMPDIR"' EXIT

if ! curl -fsSL "$URL" -o "$WORK_TMPDIR/$ASSET"; then
  echo "Error: failed to download $URL" >&2
  echo "Check https://github.com/${REPO}/releases for available binaries." >&2
  exit 1
fi

if ! curl -fsSL "${URL}.sha256" -o "$WORK_TMPDIR/$ASSET.sha256"; then
  echo "Error: failed to download checksum ${URL}.sha256" >&2
  echo "Checksum files are published alongside each release asset; cannot continue without integrity verification." >&2
  exit 1
fi

EXPECTED_HASH="$(awk '{print $1}' "$WORK_TMPDIR/$ASSET.sha256")"
if [ -z "$EXPECTED_HASH" ]; then
  echo "Error: checksum file for $ASSET was empty or malformed." >&2
  exit 1
fi

ACTUAL_HASH="$(sha256_hash "$WORK_TMPDIR/$ASSET")"
if [ -z "$ACTUAL_HASH" ]; then
  echo "Error: no SHA-256 hasher found (need shasum or sha256sum). Cannot verify download." >&2
  exit 1
fi

if [ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]; then
  echo "Error: checksum mismatch for $ASSET" >&2
  echo "  expected: $EXPECTED_HASH" >&2
  echo "  actual:   $ACTUAL_HASH" >&2
  echo "Refusing to install a tampered or corrupted archive." >&2
  exit 1
fi

EXTRACT_DIR="$WORK_TMPDIR/extracted"
mkdir -p "$EXTRACT_DIR"
if ! tar xzf "$WORK_TMPDIR/$ASSET" -C "$EXTRACT_DIR" --no-same-owner --no-same-permissions; then
  echo "Error: failed to extract $ASSET" >&2
  exit 1
fi

# cargo-dist archives contain a single top-level dir named after the asset; binary lives one level deep.
# Constrain the search so a crafted archive can't smuggle a sibling binary earlier in find order.
EXPECTED_DIR="${ASSET%.tar.gz}"
CANDIDATE="$EXTRACT_DIR/$EXPECTED_DIR/$BIN_NAME"
if [ -f "$CANDIDATE" ]; then
  BIN_PATH="$CANDIDATE"
else
  # Fallback for archives without the conventional prefix: only accept a match exactly one dir deep.
  BIN_PATH="$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 2 -type f -name "$BIN_NAME" | head -n 1)"
fi
if [ -z "$BIN_PATH" ] || [ ! -f "$BIN_PATH" ]; then
  echo "Error: archive did not contain a ${BIN_NAME} binary at the expected path." >&2
  exit 1
fi
chmod +x "$BIN_PATH" 2>/dev/null || true

if [ -w "$INSTALL_DIR" ] || { [ ! -e "$INSTALL_DIR" ] && mkdir -p "$INSTALL_DIR" 2>/dev/null; }; then
  install -m 755 "$BIN_PATH" "$INSTALL_DIR/$BIN_NAME"
elif [ "$INSTALL_DIR" = "${HOME:-}/.local/bin" ]; then
  mkdir -p "$INSTALL_DIR"
  install -m 755 "$BIN_PATH" "$INSTALL_DIR/$BIN_NAME"
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo mkdir -p "$INSTALL_DIR"
  sudo install -m 755 "$BIN_PATH" "$INSTALL_DIR/$BIN_NAME"
fi

echo "Installed slate to $INSTALL_DIR/$BIN_NAME"
case ":${PATH:-}:" in
  *:"$INSTALL_DIR":*) ;;
  *)
    echo "Note: $INSTALL_DIR is not on PATH. Add it, or set SLATE_INSTALL_DIR=/usr/local/bin to install system-wide (uses sudo)."
    ;;
esac
echo "Run 'slate setup' to get started."
