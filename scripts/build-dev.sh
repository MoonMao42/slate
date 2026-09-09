#!/usr/bin/env bash
# Reuse the disk-conscious local profile; never install or clean implicitly.
set -euo pipefail

slate_network=(--offline)
case "${1:-}" in
  "") [[ $# -eq 0 ]] || exit 2 ;;
  --online) [[ $# -eq 1 ]] || exit 2; slate_network=() ;;
  *) echo "Usage: bash scripts/build-dev.sh [--online]" >&2; exit 2 ;;
esac

slate_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd -- "$slate_root"
# Preserve Cargo's target/toolchain environment, including custom target dirs.
# The profile is deliberately fixed to avoid a new cache for each iteration.
cargo build --locked ${slate_network[@]+"${slate_network[@]}"} --profile local-check --bin slate
