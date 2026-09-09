#!/bin/sh
set -eu

REPO="${SLATE_REPO:-MoonMao42/slate}"
VERSION="${SLATE_VERSION:-latest}"
APP_NAME="${SLATE_APP_NAME:-slate-cli}"
BIN_NAME="${SLATE_BIN_NAME:-slate}"

fail() {
  echo "Error: $*" >&2
  exit 1
}

validate_name() {
  case "$2" in
    ''|[!a-zA-Z0-9]*|*[!a-zA-Z0-9._-]*)
      fail "$1 must be a simple name beginning with a letter or digit."
      ;;
  esac
}

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
    HASH_OUTPUT="$(shasum -a 256 "$1")" || return 1
  elif command -v sha256sum >/dev/null 2>&1; then
    HASH_OUTPUT="$(sha256sum "$1")" || return 1
  else
    return 1
  fi
  printf '%s\n' "$HASH_OUTPUT" | awk '
    NR == 1 && length($1) == 64 && $1 !~ /[^a-fA-F0-9]/ { print tolower($1); next }
    { exit 1 }
    END { if (NR != 1) exit 1 }
  '
}

resolve_download_base() {
  if [ "$VERSION" = "latest" ]; then
    printf '%s\n' "https://github.com/${REPO}/releases/latest/download"
  else
    printf '%s\n' "https://github.com/${REPO}/releases/download/${VERSION}"
  fi
}

validate_name SLATE_APP_NAME "$APP_NAME"
validate_name SLATE_BIN_NAME "$BIN_NAME"
ARCH="$(detect_arch)"
PLATFORM="$(detect_platform)"
TARGET="${ARCH}-${PLATFORM}"
ASSET="${APP_NAME}-${TARGET}.tar.xz"
URL="$(resolve_download_base)/${ASSET}"
INSTALL_DIR="$(resolve_install_dir)"
case "$INSTALL_DIR" in
  /*) ;;
  *) INSTALL_DIR="$(pwd)/$INSTALL_DIR" ;;
esac

if [ "${SLATE_INSTALL_DRY_RUN:-0}" = "1" ]; then
  echo "TARGET_TRIPLE=${TARGET}"
  echo "APP_NAME=${APP_NAME}"
  echo "BIN_NAME=${BIN_NAME}"
  echo "ASSET=${ASSET}"
  echo "URL=${URL}"
  echo "INSTALL_DIR=${INSTALL_DIR}"
  exit 0
fi

echo "Installing ${BIN_NAME} for ${TARGET}..."

WORK_TMPDIR="$(mktemp -d "${TMPDIR:-/tmp}/slate-install.XXXXXX")"
STAGE_DIR=""
USE_SUDO=0

run_install() {
  if [ "$USE_SUDO" = 1 ]; then
    sudo "$@"
  else
    "$@"
  fi
}

cleanup() {
  INSTALL_EXIT=$?
  trap - 0 HUP INT TERM
  if [ -n "$STAGE_DIR" ]; then
    # Only these exact, installer-created paths may be removed with privilege.
    # Do not ask for a new sudo password while handling a signal or failure.
    if [ "$USE_SUDO" = 1 ]; then
      sudo -n rm -f "$STAGE_DIR/$BIN_NAME" || :
      sudo -n rmdir "$STAGE_DIR" || :
    else
      rm -f "$STAGE_DIR/$BIN_NAME" || :
      rmdir "$STAGE_DIR" || :
    fi
  fi
  rm -rf "$WORK_TMPDIR" || :
  exit "$INSTALL_EXIT"
}
trap cleanup 0
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

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

if ! EXPECTED_HASH="$(awk -v asset="$ASSET" '
  NR == 1 && (NF == 1 || (NF == 2 && ($2 == asset || $2 == "*" asset))) &&
    length($1) == 64 && $1 !~ /[^a-fA-F0-9]/ { print tolower($1); next }
  { exit 1 }
  END { if (NR != 1) exit 1 }
' "$WORK_TMPDIR/$ASSET.sha256")"; then
  fail "checksum file for $ASSET must contain one SHA-256 record for this asset."
fi

if ! ACTUAL_HASH="$(sha256_hash "$WORK_TMPDIR/$ASSET")"; then
  fail "SHA-256 verification failed (need a working shasum or sha256sum)."
fi

if [ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]; then
  echo "Error: checksum mismatch for $ASSET" >&2
  echo "  expected: $EXPECTED_HASH" >&2
  echo "  actual:   $ACTUAL_HASH" >&2
  echo "Refusing to install a tampered or corrupted archive." >&2
  exit 1
fi

# Select exactly one supported member, including the optional ./ tar prefix.
# Never extract the archive as a tree: unrelated paths and links must not write
# outside the private workspace, even if an archive has a matching checksum.
if ! tar tf "$WORK_TMPDIR/$ASSET" > "$WORK_TMPDIR/members"; then
  fail "failed to list $ASSET."
fi
EXPECTED_DIR="${ASSET%.tar.xz}"
if ! MEMBER="$(awk -v expected="$EXPECTED_DIR/$BIN_NAME" -v binary="$BIN_NAME" '
  { path = $0; sub(/^\.\//, "", path) }
  path == expected || path == binary { count++; member = $0 }
  END { if (count != 1) exit 1; print member }
' "$WORK_TMPDIR/members")"; then
  fail "archive must contain exactly one $EXPECTED_DIR/$BIN_NAME or root-level $BIN_NAME."
fi
# Both BSD tar and GNU tar use '-' for regular files, 'l' for symlinks, and
# 'h' for hardlinks. Reject links instead of following their archive targets.
if ! tar tvf "$WORK_TMPDIR/$ASSET" "$MEMBER" > "$WORK_TMPDIR/member-type" ||
   ! awk 'substr($0, 1, 1) != "-" { exit 1 } END { if (NR != 1) exit 1 }' "$WORK_TMPDIR/member-type"; then
  fail "archive binary must be a single regular file, not a link or directory."
fi
BIN_PATH="$WORK_TMPDIR/binary"
if ! tar xOf "$WORK_TMPDIR/$ASSET" "$MEMBER" > "$BIN_PATH" || [ ! -s "$BIN_PATH" ]; then
  fail "failed to read a nonempty binary from $ASSET."
fi

# Do not overwrite package-manager symlinks or special files. A successful
# replacement updates only this directory entry, never an existing link target.
DESTINATION="$INSTALL_DIR/$BIN_NAME"
if [ -L "$DESTINATION" ] || { [ -e "$DESTINATION" ] && [ ! -f "$DESTINATION" ]; }; then
  fail "$DESTINATION is a link or non-regular file; use its package manager or another SLATE_INSTALL_DIR."
fi

if [ -w "$INSTALL_DIR" ] || { [ ! -e "$INSTALL_DIR" ] && mkdir -p "$INSTALL_DIR" 2>/dev/null; }; then
  :
elif [ "$INSTALL_DIR" = "${HOME:-}/.local/bin" ]; then
  fail "$INSTALL_DIR is not writable."
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  USE_SUDO=1
  run_install mkdir -p "$INSTALL_DIR"
fi

# Stage on the destination filesystem: even a partial install cannot truncate
# the working binary. The final same-filesystem rename replaces it atomically.
STAGE_DIR="$(run_install mktemp -d "$INSTALL_DIR/.slate-install.XXXXXX")"
if ! run_install install -m 755 "$BIN_PATH" "$STAGE_DIR/$BIN_NAME"; then
  fail "could not stage the new binary; the previous version was not replaced."
fi
if ! run_install mv -f "$STAGE_DIR/$BIN_NAME" "$INSTALL_DIR/"; then
  # An interrupted mv may have completed rename before its exit was observed.
  fail "could not confirm the final replacement; inspect $DESTINATION before retrying."
fi

echo "Installed slate to $INSTALL_DIR/$BIN_NAME"
case ":${PATH:-}:" in
  *:"$INSTALL_DIR":*) ;;
  *)
    echo "Note: $INSTALL_DIR is not on PATH. Add it, or set SLATE_INSTALL_DIR=/usr/local/bin to install system-wide (uses sudo)."
    ;;
esac
echo "Run 'slate setup' to get started."
