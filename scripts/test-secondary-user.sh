#!/bin/bash
# test-secondary-user.sh — Build slate and run setup as a secondary user.
# Usage:
# ./scripts/test-secondary-user.sh # default user: slatetest
# ./scripts/test-secondary-user.sh otheruser # custom user
# Prerequisites:
# The target user account must exist. Create one with:
# sudo sysadminctl -addUser slatetest -password test123
# What it does:
# 1. cargo build --release
# 2. Wipe the test user's slate config (clean slate)
# 3. Run `slate setup --quick` as the test user
# 4. Print diagnostic summary (fonts, config, starship, .zshrc)
set -euo pipefail

USER="${1:-slatetest}"
BINARY="$(cd "$(dirname "$0")/.." && pwd)/target/release/slate"

echo "=== Building release binary ==="
(cd "$(dirname "$0")/.." && cargo build --release)

if ! id "$USER" &>/dev/null; then
  echo "User '$USER' does not exist."
  echo "Create it with:  sudo sysadminctl -addUser $USER -password test123"
  exit 1
fi

USER_HOME=$(eval echo "~$USER")

echo ""
echo "=== Cleaning previous slate state for $USER ==="
sudo -H -u "$USER" rm -rf \
  "$USER_HOME/.config/slate" \
  "$USER_HOME/.config/starship.toml" \
  "$USER_HOME/.config/ghostty" \
  "$USER_HOME/Library/Fonts/"*NerdFont* \
  "$USER_HOME/Library/Fonts/"*Nerd*Font* \
  "$USER_HOME/.zshrc" 2>/dev/null || true

echo ""
echo "=== Running slate setup --quick as $USER ==="
echo "--- (output below) ---"
echo ""
sudo -H -u "$USER" /bin/zsh -lc "$BINARY setup --quick" 2>&1 || true

echo ""
echo "=== Diagnostic Summary ==="

echo ""
echo "--- Fonts in ~/Library/Fonts/ ---"
sudo -H -u "$USER" ls "$USER_HOME/Library/Fonts/" 2>/dev/null | head -20 || echo "(empty)"

echo ""
echo "--- Slate config ---"
sudo -H -u "$USER" ls "$USER_HOME/.config/slate/" 2>/dev/null || echo "(missing)"

echo ""
echo "--- current theme ---"
sudo -H -u "$USER" cat "$USER_HOME/.config/slate/current" 2>/dev/null || echo "(not set)"

echo ""
echo "--- current font ---"
sudo -H -u "$USER" cat "$USER_HOME/.config/slate/current-font" 2>/dev/null || echo "(not set)"

echo ""
echo "--- starship.toml (first 10 lines) ---"
sudo -H -u "$USER" head -10 "$USER_HOME/.config/starship.toml" 2>/dev/null || echo "(missing)"

echo ""
echo "--- .zshrc ---"
sudo -H -u "$USER" cat "$USER_HOME/.zshrc" 2>/dev/null || echo "(missing)"
