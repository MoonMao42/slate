#!/usr/bin/env bash
# Read-only accounting for known Slate development locations, not a disk scan.
set -euo pipefail
if [[ $# -ne 0 ]]; then
  echo 'Usage: bash scripts/dev-space.sh' >&2
  exit 2
fi
slate_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)

slate_size() {
  local slate_label=$1 slate_path=$2
  printf '\n%s\n' "$slate_label"
  if [[ -L "$slate_path" ]]; then
    printf '  Symlink; not followed: %s\n' "$slate_path"
  elif [[ -e "$slate_path" ]]; then
    du -sh -- "$slate_path"
  else
    printf '  Not present: %s\n' "$slate_path"
  fi
}

printf 'Slate storage · read-only\n'
printf 'Workspace total includes the caches and Git history below; do not add these rows.\n'
slate_size 'Workspace total (source, Git history and generated files)' "$slate_root"
slate_size 'Git history (not a build cache; keep)' "$slate_root/.git"
slate_size 'Workspace build cache (rebuildable)' "$slate_root/target"
slate_size 'Workspace Clang cache (rebuildable)' "$slate_root/.clang-module-cache"
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  slate_target=$CARGO_TARGET_DIR
  # The build wrapper runs Cargo from the repository root, not the caller cwd.
  [[ "$slate_target" == /* ]] || slate_target="$slate_root/$slate_target"
  slate_size 'CARGO_TARGET_DIR (may overlap workspace cache; do not add totals)' "$slate_target"
fi
if [[ -n "${HOME:-}" && "$HOME" == /* ]]; then
  slate_size 'Installed CLI (default installer location)' "$HOME/.local/bin/slate"
  slate_size 'Previous binary backups (kept for recovery)' "$HOME/.local/state/slate-binary-backups"
else
  printf '\nHOME is unavailable or relative; default installation paths were not inspected.\n'
fi
printf '\nKnown locations only; Cargo config and custom installer paths may differ.\n'
printf 'No files deleted, no compilation, and no Slate or tool commands launched.\n'
