#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SCRIPT="${ROOT_DIR}/install.sh"

run_case() {
  local os="$1"
  local arch="$2"
  local expected_target="$3"
  local expected_asset="$4"
  local output

  output="$(
    SLATE_INSTALL_DRY_RUN=1 \
    SLATE_OS_OVERRIDE="$os" \
    SLATE_ARCH_OVERRIDE="$arch" \
    SLATE_INSTALL_DIR="/tmp/slate-bin" \
    sh "$INSTALL_SCRIPT"
  )"

  [[ "$output" == *"TARGET_TRIPLE=${expected_target}"* ]]
  [[ "$output" == *"ASSET=${expected_asset}"* ]]
  [[ "$output" == *"INSTALL_DIR=/tmp/slate-bin"* ]]
}

run_case "Darwin" "x86_64" "x86_64-apple-darwin" "slate-x86_64-apple-darwin.tar.gz"
run_case "Darwin" "arm64" "aarch64-apple-darwin" "slate-aarch64-apple-darwin.tar.gz"
run_case "Linux" "x86_64" "x86_64-unknown-linux-gnu" "slate-x86_64-unknown-linux-gnu.tar.gz"
run_case "Linux" "aarch64" "aarch64-unknown-linux-gnu" "slate-aarch64-unknown-linux-gnu.tar.gz"

versioned_output="$(
  SLATE_INSTALL_DRY_RUN=1 \
  SLATE_OS_OVERRIDE="Linux" \
  SLATE_ARCH_OVERRIDE="x86_64" \
  SLATE_VERSION="v2.1.0" \
  SLATE_INSTALL_DIR="/tmp/slate-bin" \
  sh "$INSTALL_SCRIPT"
)"
[[ "$versioned_output" == *"/releases/download/v2.1.0/slate-x86_64-unknown-linux-gnu.tar.gz"* ]]

if SLATE_INSTALL_DRY_RUN=1 SLATE_OS_OVERRIDE="FreeBSD" sh "$INSTALL_SCRIPT" >/dev/null 2>&1; then
  echo "install.sh unexpectedly accepted an unsupported OS" >&2
  exit 1
fi

if SLATE_INSTALL_DRY_RUN=1 SLATE_OS_OVERRIDE="Linux" SLATE_ARCH_OVERRIDE="sparc64" sh "$INSTALL_SCRIPT" >/dev/null 2>&1; then
  echo "install.sh unexpectedly accepted an unsupported architecture" >&2
  exit 1
fi

echo "install.sh smoke tests passed"
