#!/usr/bin/env bash
# Two-pass loudnorm to -18 LUFS for UI SFX samples.
# Usage: ./scripts/curate-sfx.sh <input-dir> <output-dir>
# Takes raw samples (wav/ogg/flac/aiff), writes 44.1kHz/mono/16-bit WAV
# normalized to -18 LUFS integrated, -1.5 dBTP, 11 LU LRA.
# Dependencies: ffmpeg, jq.
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <input-dir> <output-dir>" >&2
  exit 2
fi
IN="$1"; OUT="$2"
mkdir -p "$OUT"

shopt -s nullglob
for f in "$IN"/*.wav "$IN"/*.ogg "$IN"/*.flac "$IN"/*.aiff "$IN"/*.mp3; do
  [ -e "$f" ] || continue
  name=$(basename "${f%.*}")
  echo "[curate-sfx] measuring $name..."
  JSON=$(ffmpeg -hide_banner -nostats -i "$f" \
    -af "loudnorm=I=-18:TP=-1.5:LRA=11:print_format=json" \
    -f null - 2>&1 | awk '/^\{/,/^\}/')
  MI=$(echo "$JSON" | jq -r '.input_i')
  MTP=$(echo "$JSON" | jq -r '.input_tp')
  MLRA=$(echo "$JSON" | jq -r '.input_lra')
  MTHRESH=$(echo "$JSON" | jq -r '.input_thresh')
  OFFSET=$(echo "$JSON" | jq -r '.target_offset')
  echo "[curate-sfx] applying pass 2 for $name..."
  ffmpeg -hide_banner -nostats -y -i "$f" \
    -af "loudnorm=I=-18:TP=-1.5:LRA=11:measured_I=$MI:measured_TP=$MTP:measured_LRA=$MLRA:measured_thresh=$MTHRESH:offset=$OFFSET:linear=true:print_format=summary" \
    -ar 44100 -ac 1 -sample_fmt s16 "$OUT/$name.wav"
done
