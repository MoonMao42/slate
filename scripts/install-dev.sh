#!/usr/bin/env bash
# Install an explicitly built local artifact; never download or restart services.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 3 ]]; then
  echo "Usage: bash scripts/install-dev.sh ABSOLUTE_BINARY [ABSOLUTE_BIN_DIR [ABSOLUTE_BACKUP_DIR]]" >&2
  exit 2
fi
slate_candidate=$1
slate_destination=${2:-${HOME:?}/.local/bin}
slate_backups=${3:-${HOME:?}/.local/state/slate-binary-backups}
for slate_path in "$slate_candidate" "$slate_destination" "$slate_backups"; do
  [[ "$slate_path" = /* ]] || { echo "Use absolute paths." >&2; exit 2; }
done
[[ -f "$slate_candidate" && -x "$slate_candidate" && ! -L "$slate_candidate" ]] || {
  echo "Candidate must be a regular executable, not a symlink." >&2; exit 1;
}
mkdir -p "$slate_destination"
slate_target="$slate_destination/slate"
slate_lock="$slate_destination/.slate-dev-install.lock"
mkdir "$slate_lock" || { echo "Another developer install may be active: $slate_lock" >&2; exit 1; }
slate_stage=
cleanup() {
  if [[ -n "$slate_stage" ]]; then
    rm -f "$slate_stage/slate"
    rmdir "$slate_stage"
  fi
  rmdir "$slate_lock"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ -L "$slate_target" || ( -e "$slate_target" && ! -f "$slate_target" ) ]]; then
  echo "Refusing to replace a symlink or non-file: $slate_target" >&2
  exit 1
fi
# Verify the staged executable before touching the installed one. The caller
# supplies a trusted artifact; --version is executed, not an authenticity check.
slate_stage=$(mktemp -d "$slate_destination/.slate-update-XXXXXX")
install -m 755 "$slate_candidate" "$slate_stage/slate"
"$slate_stage/slate" --version
if [[ -f "$slate_target" ]] && cmp -s "$slate_stage/slate" "$slate_target"; then
  echo "Already installed: $slate_target"
  exit 0
fi
slate_backup=
if [[ -f "$slate_target" ]]; then
  mkdir -p "$slate_backups"
  slate_backup=$(mktemp -d "$slate_backups/update-XXXXXX")
  if ! cp -p "$slate_target" "$slate_backup/slate" || ! cmp "$slate_target" "$slate_backup/slate"; then
    echo "Backup was not verified; replacement was not attempted." >&2
    echo "Unverified copy may remain at: $slate_backup/slate" >&2
    exit 1
  fi
  echo "Backup: $slate_backup/slate"
fi
if ! mv -f "$slate_stage/slate" "$slate_target" || ! cmp "$slate_candidate" "$slate_target"; then
  echo "Replacement was not confirmed. Inspect: $slate_target" >&2
  if [[ -n "$slate_backup" ]]; then
    echo "Verified previous binary: $slate_backup/slate" >&2
  fi
  echo "No automatic rollback was attempted; the target may already have changed." >&2
  exit 1
fi
echo "Installed: $slate_target"
echo "Your shell may resolve a different command; check with: command -v slate"
echo "Running sessions and the auto-theme watcher were not restarted."
