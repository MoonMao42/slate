#!/usr/bin/env bash
# No compiler, installed CLI, or personal backup is invoked by these checks.
set -euo pipefail
slate_test_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
slate_fixture=$(mktemp -d /tmp/slate-space-test.XXXXXX)
trap 'if [[ -L "$slate_fixture/linked cache" ]]; then unlink "$slate_fixture/linked cache"; fi; rmdir "$slate_fixture/empty cache" "$slate_fixture" 2>/dev/null || true' EXIT
mkdir "$slate_fixture/empty cache"

# A fake reader makes paths observable, without traversing the user's caches.
du() { printf 'SIZE %s\n' "$*"; return "${slate_du_exit:-0}"; }
export -f du
cd /tmp
slate_output=$(HOME="$slate_fixture" CARGO_TARGET_DIR="$slate_fixture/empty cache" bash "$slate_test_root/scripts/dev-space.sh")
[[ "$slate_output" == *"SIZE -sh -- $slate_fixture/empty cache"* ]]
[[ "$slate_output" == *"Not present: $slate_fixture/.local/bin/slate"* ]]
[[ "$slate_output" == *'No files deleted, no compilation'* ]]
[[ "$slate_output" == *"SIZE -sh -- $slate_test_root"* ]]
[[ "$slate_output" == *'Workspace total includes the caches and Git history below; do not add these rows.'* ]]
[[ "$slate_output" == *'Git history (not a build cache; keep)'* ]]

# An externally located cache may be linked into the workspace. Do not follow it.
ln -s "$slate_fixture/empty cache" "$slate_fixture/linked cache"
slate_output=$(HOME="$slate_fixture" CARGO_TARGET_DIR="$slate_fixture/linked cache" bash "$slate_test_root/scripts/dev-space.sh")
[[ "$slate_output" == *"Symlink; not followed: $slate_fixture/linked cache"* ]]
[[ "$slate_output" != *"SIZE -sh -- $slate_fixture/linked cache"* ]]

slate_output=$(HOME=relative CARGO_TARGET_DIR=. bash "$slate_test_root/scripts/dev-space.sh")
[[ "$slate_output" == *"SIZE -sh -- $slate_test_root/."* ]]
[[ "$slate_output" == *'default installation paths were not inspected'* ]]

slate_output=$(HOME='' CARGO_TARGET_DIR='' bash "$slate_test_root/scripts/dev-space.sh")
[[ "$slate_output" == *'default installation paths were not inspected'* ]]
[[ "$slate_output" != *'CARGO_TARGET_DIR ('* ]]

# Never treat an unreadable size as zero or claim the inspection succeeded.
slate_status=0
HOME="$slate_fixture" CARGO_TARGET_DIR="$slate_fixture/empty cache" slate_du_exit=17 \
  bash "$slate_test_root/scripts/dev-space.sh" >/dev/null || slate_status=$?
[[ "$slate_status" -eq 17 ]]
slate_status=0
bash "$slate_test_root/scripts/dev-space.sh" --clean >/dev/null 2>&1 || slate_status=$?
[[ "$slate_status" -eq 2 ]]

slate_ignored=0
git -C "$slate_test_root" check-ignore -q --no-index -- scripts/dev-space.sh || slate_ignored=$?
[[ "$slate_ignored" -eq 1 ]]
echo 'dev-space checks passed (no compilation, installation or cleanup)'
