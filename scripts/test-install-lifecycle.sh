#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_DIR"

echo "==> isolated lifecycle smoke test"
cargo test test_full_pipeline --test integration_tests -- --exact --nocapture

echo ""
echo "==> plain prompt fallback smoke test"
cargo test test_system_font_switch_uses_plain_starship_profile --test integration_tests -- --exact --nocapture

echo ""
echo "==> clean removes Ghostty config hooks"
cargo test test_clean_removes_ghostty_managed_config_references --test integration_tests -- --exact --nocapture

echo ""
echo "==> clean removes Alacritty managed imports"
cargo test test_clean_removes_alacritty_managed_imports --test integration_tests -- --exact --nocapture

echo ""
echo "Lifecycle checks passed."
