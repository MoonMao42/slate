#!/usr/bin/env bash
# bake SFX into a VHS-rendered MP4 using .tape-derived
# offsets.
# Usage:
# ./scripts/bake-audio.sh <tape-file> <input-mp4> <output-mp4>
# Dependencies: ffmpeg, python3. Both are standard on macOS (via Homebrew)
# and Ubuntu 24.04 CI runners. The same resources/sfx/*.wav files the
# runtime SoundSink embeds are reused here so the README MP4 you hear
# matches the sounds the real CLI plays (single source of truth).
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <tape-file> <input-mp4> <output-mp4>" >&2
  exit 2
fi

TAPE="$1"
IN="$2"
OUT="$3"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [ ! -f "${TAPE}" ]; then
  echo "[bake-audio] tape file not found: ${TAPE}" >&2
  exit 1
fi
if [ ! -f "${IN}" ]; then
  echo "[bake-audio] input MP4 not found: ${IN}" >&2
  exit 1
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "[bake-audio] ffmpeg not on PATH — install via 'brew install ffmpeg' or 'apt install ffmpeg'" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "[bake-audio] python3 not on PATH — required to parse .tape timing" >&2
  exit 1
fi

# Event name -> WAV filename. The mapping aligns with the 
# marker vocabulary (ApplyComplete / SetupComplete / Success / Failure /
# Selection / Navigation) plus a few short aliases so either spelling
# works in a tape. Implemented as a case statement so this script runs on
# the stock macOS /bin/bash 3.2 — associative arrays need bash 4+.
sfx_wav_for() {
  case "$1" in
    Hero|SetupComplete) echo "hero.wav" ;;
    Apply|ApplyComplete) echo "apply.wav" ;;
    Success) echo "success.wav" ;;
    Failure) echo "failure.wav" ;;
    Select|Selection) echo "select.wav" ;;
    Click|Navigation) echo "click.wav" ;;
    *) echo "" ;;
  esac
}

MARKERS="$(python3 "${SCRIPT_DIR}/parse_tape_sfx.py" "${TAPE}")"

if [ -z "${MARKERS}" ]; then
  echo "[bake-audio] no SFX markers found — copying MP4 without audio overlay"
  cp "${IN}" "${OUT}"
  exit 0
fi

# Build ffmpeg input list + filter graph.
INPUTS=(-y -i "${IN}")
FILTER=""
IDX=0

while IFS=' ' read -r EVENT OFFSET; do
  [ -z "${EVENT:-}" ] && continue
  WAV="$(sfx_wav_for "${EVENT}")"
  if [ -z "${WAV}" ]; then
    echo "[bake-audio] warning: no WAV mapped for event '${EVENT}' — skipping" >&2
    continue
  fi
  WAV_PATH="${REPO_ROOT}/resources/sfx/${WAV}"
  if [ ! -f "${WAV_PATH}" ]; then
    echo "[bake-audio] error: ${WAV_PATH} missing" >&2
    exit 1
  fi
  INPUTS+=(-itsoffset "${OFFSET}" -i "${WAV_PATH}")
  IDX=$((IDX + 1))
  FILTER+="[${IDX}:a]"
done <<< "${MARKERS}"

if [ "${IDX}" -eq 0 ]; then
  echo "[bake-audio] no mappable markers — copying MP4 without audio overlay"
  cp "${IN}" "${OUT}"
  exit 0
fi

FILTER+="amix=inputs=${IDX}:duration=longest[a]"

ffmpeg -hide_banner \
  "${INPUTS[@]}" \
  -filter_complex "${FILTER}" \
  -map 0:v -map "[a]" \
  -c:v copy -c:a aac \
  "${OUT}"

echo "[bake-audio] wrote ${OUT} with ${IDX} SFX overlay(s)"
