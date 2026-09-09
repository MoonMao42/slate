#!/usr/bin/env bash
# Exercise the actual wrapper without invoking a compiler or creating caches.
set -euo pipefail
slate_test_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
export slate_test_root

# The documented entry points must survive a normal source checkout. Keep this
# read-only: do not stage ignored scripts or alter the caller's Git index.
for slate_test_script in scripts/build-dev.sh scripts/install-dev.sh; do
  slate_test_ignored=0
  git -C "$slate_test_root" check-ignore -q --no-index -- "$slate_test_script" || slate_test_ignored=$?
  [[ "$slate_test_ignored" -eq 1 ]] || {
    echo "Documented development script is ignored or Git inspection failed: $slate_test_script" >&2
    exit 1
  }
done

cargo() {
  [[ "$PWD" == "$slate_test_root" ]] || return 91
  [[ "${CARGO_TARGET_DIR:-}" == '/private/custom target' ]] || return 92
  [[ "${CARGO_BUILD_TARGET:-}" == 'aarch64-apple-darwin' ]] || return 93
  if [[ "$slate_test_case" == offline ]]; then
    [[ "$*" == 'build --locked --offline --profile local-check --bin slate' ]] || return 94
  else
    [[ "$*" == 'build --locked --profile local-check --bin slate' ]] || return 95
  fi
  return "${slate_test_exit:-0}"
}
export -f cargo
export CARGO_TARGET_DIR='/private/custom target'
export CARGO_BUILD_TARGET=aarch64-apple-darwin
cd /tmp
export slate_test_case=offline
bash "$slate_test_root/scripts/build-dev.sh"
export slate_test_case=online
bash "$slate_test_root/scripts/build-dev.sh" --online
export slate_test_exit=17
slate_test_status=0
bash "$slate_test_root/scripts/build-dev.sh" --online || slate_test_status=$?
[[ "$slate_test_status" -eq 17 ]]
for slate_test_arg in --release --install --clean ''; do
  slate_test_status=0
  bash "$slate_test_root/scripts/build-dev.sh" "$slate_test_arg" >/dev/null 2>&1 || slate_test_status=$?
  [[ "$slate_test_status" -eq 2 ]]
done
slate_test_status=0
bash "$slate_test_root/scripts/build-dev.sh" --online --release || slate_test_status=$?
[[ "$slate_test_status" -eq 2 ]]
echo 'build-dev wrapper checks passed (no compilation or installation)'
