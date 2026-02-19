#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

SLATE_BIN="${SLATE_BIN:-$PROJECT_DIR/target/release/slate}"
GHOSTTY_BIN="${GHOSTTY_BIN:-/Applications/Ghostty.app/Contents/MacOS/ghostty}"
RECORDER_BUILD_DIR="${RECORDER_BUILD_DIR:-/tmp/slate-demo-recorder}"
RECORDER_BIN="$RECORDER_BUILD_DIR/window_recorder"

OUTPUT_BASENAME="${OUTPUT_BASENAME:-demo}"
OUTPUT_DIR="${OUTPUT_DIR:-$SCRIPT_DIR}"
RAW_MOV="$OUTPUT_DIR/${OUTPUT_BASENAME}-raw.mov"
MP4_OUT="$OUTPUT_DIR/${OUTPUT_BASENAME}.mp4"
GIF_OUT="$OUTPUT_DIR/${OUTPUT_BASENAME}.gif"
POSTER_OUT="$OUTPUT_DIR/${OUTPUT_BASENAME}-poster.png"

MAX_SECONDS="${MAX_SECONDS:-30}"
TRIM_START="${TRIM_START:-0.60}"
TRIM_END="${TRIM_END:-0.80}"
WIDTH="${WIDTH:-960}"
GIF_FPS="${GIF_FPS:-18}"

DEMO_ROOT="$(mktemp -d /tmp/slate-demo.XXXXXX)"
DEMO_HOME="$DEMO_ROOT/home"
DEMO_CONFIG="$DEMO_HOME/.config"
GHOSTTY_CONFIG="$DEMO_CONFIG/ghostty/config"
STARSHIP_CONFIG="$DEMO_CONFIG/starship.toml"
EXPECT_SCRIPT="$DEMO_ROOT/demo.expect"
READY_FILE="$DEMO_ROOT/ready"
DONE_FILE="$DEMO_ROOT/done"

cleanup() {
  if [[ -n "${RECORD_PID:-}" ]]; then
    kill -INT "$RECORD_PID" 2>/dev/null || true
    wait "$RECORD_PID" 2>/dev/null || true
  fi
  if [[ -n "${GHOSTTY_PID:-}" ]]; then
    kill "$GHOSTTY_PID" 2>/dev/null || true
    wait "$GHOSTTY_PID" 2>/dev/null || true
  fi
  if [[ "${KEEP_DEMO_SANDBOX:-0}" != "1" ]]; then
    rm -rf "$DEMO_ROOT"
  else
    echo "Keeping sandbox at $DEMO_ROOT"
  fi
}
trap cleanup EXIT

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

build_slate() {
  if [[ ! -x "$SLATE_BIN" ]]; then
    echo "==> building release binary"
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
  fi
}

build_recorder() {
  mkdir -p "$RECORDER_BUILD_DIR"
  if [[ ! -x "$RECORDER_BIN" || "$SCRIPT_DIR/window_recorder.swift" -nt "$RECORDER_BIN" ]]; then
    echo "==> compiling window recorder"
    swiftc "$SCRIPT_DIR/window_recorder.swift" \
      -o "$RECORDER_BIN" \
      -framework ScreenCaptureKit \
      -framework AVFoundation \
      -framework AppKit
  fi
}

prepare_demo_fs() {
  mkdir -p "$DEMO_CONFIG/ghostty" "$DEMO_HOME"

  cat > "$GHOSTTY_CONFIG" <<'EOF'
font-family = Menlo
font-size = 14
background = #f6f6f3
foreground = #2b2b2b
cursor-color = #2b2b2b
selection-background = #d8e8ff
selection-foreground = #1f1f1f
window-padding-x = 6
window-padding-y = 6
window-width = 102
window-height = 30
title = slate demo
EOF

  : > "$STARSHIP_CONFIG"
  : > "$DEMO_HOME/.zshrc"
}

write_expect_script() {
  cat > "$EXPECT_SCRIPT" <<EOF
#!/usr/bin/expect -f
set timeout 180

proc wait_for_prompt {} {
    expect -re {[$] $}
}

proc type_slow {text {delay_ms 55}} {
    foreach char [split \$text ""] {
        send -- "\$char"
        after \$delay_ms
    }
}

spawn env \\
  HOME="$DEMO_HOME" \\
  SLATE_HOME="$DEMO_HOME" \\
  XDG_CONFIG_HOME="$DEMO_CONFIG" \\
  STARSHIP_CONFIG="$STARSHIP_CONFIG" \\
  PATH="$(dirname "$SLATE_BIN"):\$env(PATH)" \\
  TERM_PROGRAM=ghostty \\
  zsh -f

wait_for_prompt
send -- "PROMPT='\\$ '\\r"
wait_for_prompt
send -- "clear\\r"
wait_for_prompt

type_slow "echo 'default prompt. default colors. default everything.'" 40
send -- "\\r"
after 700
wait_for_prompt

type_slow "ls" 40
send -- "\\r"
after 900
wait_for_prompt

exec touch "$READY_FILE"
after 1200

type_slow "slate setup --quick" 55
send -- "\\r"
expect -re {[$] $}
after 900

type_slow "slate theme" 55
send -- "\\r"
after 1500
send -- "\\033\\[B"
after 500
send -- "\\033\\[B"
after 500
send -- "\\033\\[A"
after 500
send -- "\\r"
after 1500
wait_for_prompt

type_slow "exec zsh" 55
send -- "\\r"
after 1800

type_slow "slate status" 50
send -- "\\r"
after 3200

exec touch "$DONE_FILE"
after 500
EOF

  chmod +x "$EXPECT_SCRIPT"
}

wait_for_file() {
  local path="$1"
  local seconds="$2"
  local elapsed=0

  while [[ ! -f "$path" ]]; do
    sleep 1
    elapsed=$((elapsed + 1))
    if (( elapsed >= seconds )); then
      echo "Timed out waiting for $path" >&2
      exit 1
    fi
  done
}

find_new_window_id() {
  local before_ids after_ids
  before_ids="$("$RECORDER_BIN" list 2>/dev/null | awk '/Ghostty/ {print $1}' | sort)"

  XDG_CONFIG_HOME="$DEMO_CONFIG" \
    "$GHOSTTY_BIN" --config-file="$GHOSTTY_CONFIG" -e "$EXPECT_SCRIPT" &
  GHOSTTY_PID=$!

  wait_for_file "$READY_FILE" 45
  sleep 1

  after_ids="$("$RECORDER_BIN" list 2>/dev/null | awk '/Ghostty/ {print $1}' | sort)"
  comm -13 <(printf "%s\n" "$before_ids") <(printf "%s\n" "$after_ids") | head -n 1
}

record_demo() {
  local window_id="$1"

  if [[ -z "$window_id" ]]; then
    echo "Could not determine Ghostty window id" >&2
    exit 1
  fi

  rm -f "$RAW_MOV" "$MP4_OUT" "$GIF_OUT" "$POSTER_OUT"

  echo "==> recording Ghostty window $window_id"
  "$RECORDER_BIN" "$window_id" "$RAW_MOV" &
  RECORD_PID=$!

  wait_for_file "$DONE_FILE" 120
  sleep 1

  kill -INT "$RECORD_PID" 2>/dev/null || true
  wait "$RECORD_PID" 2>/dev/null || true
  unset RECORD_PID
}

trimmed_duration() {
  local raw_duration
  raw_duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$RAW_MOV")"
  awk -v d="$raw_duration" -v start="$TRIM_START" -v tail="$TRIM_END" -v max="$MAX_SECONDS" '
    BEGIN {
      result = d - start - tail;
      if (result <= 0) {
        result = d;
      }
      if (result > max) {
        result = max;
      }
      printf "%.2f", result;
    }
  '
}

export_assets() {
  local duration palette
  duration="$(trimmed_duration)"
  palette="$DEMO_ROOT/palette.png"

  echo "==> exporting mp4"
  ffmpeg -y \
    -ss "$TRIM_START" \
    -t "$duration" \
    -i "$RAW_MOV" \
    -vf "scale=${WIDTH}:-2:flags=lanczos,format=yuv420p" \
    -an \
    "$MP4_OUT" >/dev/null 2>&1

  echo "==> generating palette"
  ffmpeg -y \
    -ss "$TRIM_START" \
    -t "$duration" \
    -i "$RAW_MOV" \
    -vf "fps=${GIF_FPS},scale=${WIDTH}:-2:flags=lanczos,palettegen=stats_mode=diff" \
    "$palette" >/dev/null 2>&1

  echo "==> exporting gif"
  ffmpeg -y \
    -ss "$TRIM_START" \
    -t "$duration" \
    -i "$RAW_MOV" \
    -i "$palette" \
    -lavfi "fps=${GIF_FPS},scale=${WIDTH}:-2:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle" \
    -loop 0 \
    "$GIF_OUT" >/dev/null 2>&1

  echo "==> exporting poster frame"
  ffmpeg -y \
    -ss 1.5 \
    -i "$MP4_OUT" \
    -frames:v 1 \
    "$POSTER_OUT" >/dev/null 2>&1
}

main() {
  require_command cargo
  require_command ffmpeg
  require_command ffprobe
  require_command swiftc
  require_command expect

  if [[ ! -x "$GHOSTTY_BIN" ]]; then
    echo "Ghostty binary not found at $GHOSTTY_BIN" >&2
    exit 1
  fi

  build_slate
  build_recorder
  prepare_demo_fs
  write_expect_script

  echo "==> launching storyboard"
  local window_id
  window_id="$(find_new_window_id)"
  record_demo "$window_id"
  export_assets

  echo ""
  echo "Demo assets generated:"
  ls -lh "$MP4_OUT" "$GIF_OUT" "$POSTER_OUT"
}

main "$@"
