#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

SCENE="${1:-${SCENE:-all}}"

SLATE_BIN="${SLATE_BIN:-$PROJECT_DIR/target/release/slate}"
GHOSTTY_APP="${GHOSTTY_APP:-/Applications/Ghostty.app}"
RECORDER_BUILD_DIR="${RECORDER_BUILD_DIR:-/tmp/slate-demo-recorder}"
RECORDER_BIN="$RECORDER_BUILD_DIR/window_recorder"
OUTPUT_DIR="${OUTPUT_DIR:-$SCRIPT_DIR}"
WIDTH="${WIDTH:-960}"
GIF_FPS="${GIF_FPS:-18}"
WINDOW_WIDTH="${WINDOW_WIDTH:-1180}"
WINDOW_HEIGHT="${WINDOW_HEIGHT:-760}"
WINDOW_X="${WINDOW_X:-140}"
WINDOW_Y="${WINDOW_Y:-110}"

case "$SCENE" in
  setup)
    OUTPUT_BASENAME="${OUTPUT_BASENAME:-setup-demo}"
    MAX_SECONDS="${MAX_SECONDS:-18}"
    TRIM_START="${TRIM_START:-0.60}"
    TRIM_END="${TRIM_END:-0.50}"
    STARTUP_WAIT="${STARTUP_WAIT:-1.8}"
    ;;
  theme)
    OUTPUT_BASENAME="${OUTPUT_BASENAME:-theme-demo}"
    MAX_SECONDS="${MAX_SECONDS:-20}"
    TRIM_START="${TRIM_START:-0.35}"
    TRIM_END="${TRIM_END:-0.45}"
    STARTUP_WAIT="${STARTUP_WAIT:-1.9}"
    ;;
  full)
    OUTPUT_BASENAME="${OUTPUT_BASENAME:-demo}"
    MAX_SECONDS="${MAX_SECONDS:-30}"
    TRIM_START="${TRIM_START:-0.35}"
    TRIM_END="${TRIM_END:-0.60}"
    STARTUP_WAIT="${STARTUP_WAIT:-1.8}"
    ;;
  all)
    "$0" setup
    "$0" theme
    exit 0
    ;;
  *)
    echo "Unknown scene: $SCENE" >&2
    echo "Usage: $0 [setup|theme|full|all]" >&2
    exit 1
    ;;
esac

RAW_MOV="$OUTPUT_DIR/${OUTPUT_BASENAME}-raw.mov"
MP4_OUT="$OUTPUT_DIR/${OUTPUT_BASENAME}.mp4"
GIF_OUT="$OUTPUT_DIR/${OUTPUT_BASENAME}.gif"
POSTER_OUT="$OUTPUT_DIR/${OUTPUT_BASENAME}-poster.png"

DEMO_ROOT="$(mktemp -d /tmp/slate-demo.XXXXXX)"
DEMO_HOME="$DEMO_ROOT/home"
DEMO_CONFIG="$DEMO_HOME/.config"
DEMO_BIN_DIR="$DEMO_ROOT/bin"
DEMO_GHOSTTY_DIR="$DEMO_CONFIG/ghostty"
DEMO_GHOSTTY_CONFIG="$DEMO_GHOSTTY_DIR/config"
DEMO_STARSHIP_CONFIG="$DEMO_CONFIG/starship.toml"
DEMO_PROJECT_LINK="$DEMO_HOME/slate"
PATCH_STARSHIP_SCRIPT="$DEMO_ROOT/patch-starship.sh"
PREWARM_SCRIPT="$DEMO_ROOT/prewarm.sh"
SCENE_SCRIPT="$DEMO_ROOT/scene.applescript"
WINDOW_TITLE="slate-${SCENE}-demo-$$-$RANDOM"

close_demo_window() {
  local window_key="${1:-}"
  if [[ -z "$window_key" ]]; then
    return 0
  fi

  osascript <<EOF >/dev/null 2>&1 || true
tell application "Ghostty"
  if (count of (every window whose id is "$window_key")) > 0 then
    set targetWindow to first window whose id is "$window_key"
    close window targetWindow
  end if
end tell
EOF
}

cleanup() {
  if [[ -n "${RECORD_PID:-}" ]]; then
    kill -INT "$RECORD_PID" 2>/dev/null || true
    wait "$RECORD_PID" 2>/dev/null || true
  fi

  if [[ -n "${GHOSTTY_WINDOW_KEY:-}" ]]; then
    close_demo_window "$GHOSTTY_WINDOW_KEY"
  fi

  if [[ -n "${DEMO_GHOSTTY_PID:-}" ]]; then
    kill "$DEMO_GHOSTTY_PID" 2>/dev/null || true
    wait "$DEMO_GHOSTTY_PID" 2>/dev/null || true
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
  if [[ ! -x "$SLATE_BIN" ]] || find "$PROJECT_DIR/src" "$PROJECT_DIR/Cargo.toml" "$PROJECT_DIR/Cargo.lock" -type f -newer "$SLATE_BIN" -print -quit | grep -q .; then
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
  mkdir -p "$DEMO_HOME" "$DEMO_CONFIG" "$DEMO_BIN_DIR" "$DEMO_GHOSTTY_DIR"
  : > "$DEMO_HOME/.zshrc"
  ln -s "$PROJECT_DIR" "$DEMO_PROJECT_LINK"

  cat > "$DEMO_GHOSTTY_CONFIG" <<EOF
title = "$WINDOW_TITLE"
font-size = 14
window-show-tab-bar = never
macos-titlebar-style = hidden
macos-titlebar-proxy-icon = hidden
command = /bin/zsh -f -i
EOF

  cat > "$PATCH_STARSHIP_SCRIPT" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

patch_file() {
  local file="$1"
  [[ -f "$file" ]] || return 0
  perl -0pi -e 's/format = '\''\[\$user \]\(\$style\)'\''/format = '\''[ moonmao ](\$style)'\''/g' "$file"
  perl -0pi -e 's/format = "\[\$user\]\(\$style\) "/format = "[moonmao](\$style) "/g' "$file"
}

patch_file "${DEMO_STARSHIP_CONFIG:-}"

if [[ -d "${DEMO_STARSHIP_DIR:-}" ]]; then
  while IFS= read -r -d '' file; do
    patch_file "$file"
  done < <(find "${DEMO_STARSHIP_DIR:-}" -type f -name '*.toml' -print0)
fi
EOF
  chmod +x "$PATCH_STARSHIP_SCRIPT"

  cat > "$DEMO_BIN_DIR/slate" <<EOF
#!/usr/bin/env bash
set -euo pipefail

set +e
"$SLATE_BIN" "\$@"
status=\$?
set -e

case "\${1:-}" in
  setup|theme|set)
    if [[ \$status -eq 0 ]]; then
      "$PATCH_STARSHIP_SCRIPT" >/dev/null 2>&1 || true
    fi
    ;;
esac

exit \$status
EOF
  chmod +x "$DEMO_BIN_DIR/slate"
}

write_prewarm_script() {
  cat > "$PREWARM_SCRIPT" <<EOF
export HOME="$DEMO_HOME"
export ZDOTDIR="$DEMO_HOME"
export XDG_CONFIG_HOME="$DEMO_CONFIG"
export SLATE_HOME="$DEMO_HOME"
export DEMO_STARSHIP_CONFIG="$DEMO_STARSHIP_CONFIG"
export DEMO_STARSHIP_DIR="$DEMO_CONFIG/slate/managed/starship"
export PATH="$DEMO_BIN_DIR:$(dirname "$SLATE_BIN"):\$PATH"
export USER="moonmao"
export LOGNAME="moonmao"
export LANG="en_US.UTF-8"
export LC_ALL="en_US.UTF-8"
export TERM_PROGRAM="ghostty"
cd "$DEMO_PROJECT_LINK"
PS1='$ '
PROMPT='$ '
RPROMPT=''
RPS1=''
PROMPT_EOL_MARK=''
precmd_functions=()
preexec_functions=()
EOF

  case "$SCENE" in
    setup|full)
      cat >> "$PREWARM_SCRIPT" <<'EOF'
clear
EOF
      ;;
    theme)
      cat >> "$PREWARM_SCRIPT" <<'EOF'
slate setup --quick </dev/null >/dev/null 2>&1
EOF
      ;;
  esac
}

ghostty_window_id() {
  "$RECORDER_BIN" list 2>/dev/null | awk -F '\t' -v title="$WINDOW_TITLE" '
    $2 == "Ghostty" && $3 == title { print $1; exit }
  '
}

ghostty_process_id() {
  ps -axo pid=,command= | awk -v config="$DEMO_GHOSTTY_CONFIG" '
    index($0, "/Applications/Ghostty.app/Contents/MacOS/ghostty") && index($0, config) {
      print $1
    }
  ' | tail -n 1
}

wait_for_window_key() {
  local elapsed=0
  while [[ -z "${GHOSTTY_WINDOW_KEY:-}" ]]; do
    GHOSTTY_WINDOW_KEY="$(osascript <<EOF 2>/dev/null || true
tell application "Ghostty"
  if (count of (every window whose name is "$WINDOW_TITLE")) > 0 then
    return id of (first window whose name is "$WINDOW_TITLE")
  end if
end tell
EOF
)"
    if [[ -n "${GHOSTTY_WINDOW_KEY:-}" ]]; then
      break
    fi
    sleep 0.5
    elapsed=$((elapsed + 1))
    if (( elapsed >= 20 )); then
      echo "Could not locate Ghostty AppleScript window for $WINDOW_TITLE" >&2
      exit 1
    fi
  done
}

wait_for_window_recorder_id() {
  local elapsed=0
  while [[ -z "${WINDOW_ID:-}" ]]; do
    WINDOW_ID="$(ghostty_window_id)"
    if [[ -n "${WINDOW_ID:-}" ]]; then
      break
    fi
    sleep 0.5
    elapsed=$((elapsed + 1))
    if (( elapsed >= 20 )); then
      echo "Could not determine ScreenCaptureKit window id for $WINDOW_TITLE" >&2
      exit 1
    fi
  done
}

wait_for_ghostty_pid() {
  local elapsed=0
  while [[ -z "${DEMO_GHOSTTY_PID:-}" ]]; do
    DEMO_GHOSTTY_PID="$(ghostty_process_id)"
    if [[ -n "${DEMO_GHOSTTY_PID:-}" ]]; then
      break
    fi
    sleep 0.5
    elapsed=$((elapsed + 1))
    if (( elapsed >= 20 )); then
      echo "Could not determine Ghostty process id for demo instance" >&2
      exit 1
    fi
  done
}

activate_demo_window() {
  osascript <<EOF >/dev/null
tell application "Ghostty"
  set targetWindow to first window whose id is "$GHOSTTY_WINDOW_KEY"
  activate
  activate window targetWindow
end tell
EOF
}

position_demo_window() {
  activate_demo_window
  osascript <<EOF >/dev/null 2>&1 || true
tell application "System Events"
  tell process "ghostty"
    set position of front window to {$WINDOW_X, $WINDOW_Y}
    set size of front window to {$WINDOW_WIDTH, $WINDOW_HEIGHT}
  end tell
end tell
EOF
}

launch_demo_window() {
  env \
    HOME="$DEMO_HOME" \
    XDG_CONFIG_HOME="$DEMO_CONFIG" \
    SLATE_HOME="$DEMO_HOME" \
    USER="moonmao" \
    LOGNAME="moonmao" \
    PATH="$DEMO_BIN_DIR:$(dirname "$SLATE_BIN"):$PATH" \
    open -na "$GHOSTTY_APP" --args --config-file="$DEMO_GHOSTTY_CONFIG"

  sleep "$STARTUP_WAIT"

  wait_for_window_key
  wait_for_window_recorder_id
  wait_for_ghostty_pid
  position_demo_window
}

run_prewarm() {
  local post_exec_delay="${1:-2.2}"

  case "$SCENE" in
    setup|full)
      osascript <<EOF >/dev/null
tell application "Ghostty"
  set targetWindow to first window whose id is "$GHOSTTY_WINDOW_KEY"
  activate window targetWindow
  set t to focused terminal of selected tab of targetWindow
  input text ". \"$PREWARM_SCRIPT\"" to t
  send key "enter" to t
  delay 0.8
end tell
EOF
      ;;
    theme)
      osascript <<EOF >/dev/null
tell application "Ghostty"
  set targetWindow to first window whose id is "$GHOSTTY_WINDOW_KEY"
  activate window targetWindow
  set t to focused terminal of selected tab of targetWindow
  input text ". \"$PREWARM_SCRIPT\"" to t
  send key "enter" to t
  delay 3.2
  input text "exec zsh -i" to t
  send key "enter" to t
  delay $post_exec_delay
  input text "clear" to t
  send key "enter" to t
  delay 0.8
end tell
EOF
      ;;
  esac
}

scene_body() {
  case "$SCENE" in
    setup)
      cat <<'EOF'
  delay 0.95
  my type_slow(t, "echo 'default prompt. default colors. default everything.'", 0.04)
  send key "enter" to t
  delay 1.1

  my type_slow(t, "ls", 0.04)
  send key "enter" to t
  delay 1.0

  my type_slow(t, "slate setup --quick </dev/null", 0.05)
  send key "enter" to t
  delay 2.6

  my type_slow(t, "exec zsh -i", 0.05)
  send key "enter" to t
  delay 2.8

  input text "clear" to t
  send key "enter" to t
  delay 3.0
EOF
      ;;
    theme)
      cat <<'EOF'
  delay 0.4
  my type_slow(t, "slate theme", 0.05)
  send key "enter" to t
  delay 1.8

  send key "arrowRight" to t
  delay 0.65
  send key "arrowRight" to t
  delay 0.65
  send key "arrowLeft" to t
  delay 0.65
  send key "arrowDown" to t
  delay 0.65
  send key "arrowDown" to t
  delay 0.65
  send key "arrowUp" to t
  delay 0.65
  send key "enter" to t
  delay 2.6

  input text "clear" to t
  send key "enter" to t
  delay 2.4
EOF
      ;;
    full)
      cat <<'EOF'
  delay 0.95
  my type_slow(t, "echo 'default prompt. default colors. default everything.'", 0.04)
  send key "enter" to t
  delay 1.1

  my type_slow(t, "ls", 0.04)
  send key "enter" to t
  delay 1.0

  my type_slow(t, "slate setup --quick </dev/null", 0.05)
  send key "enter" to t
  delay 2.6

  my type_slow(t, "exec zsh -i", 0.05)
  send key "enter" to t
  delay 2.8

  input text "clear" to t
  send key "enter" to t
  delay 0.8

  my type_slow(t, "slate theme", 0.05)
  send key "enter" to t
  delay 1.8

  send key "arrowRight" to t
  delay 0.65
  send key "arrowLeft" to t
  delay 0.65
  send key "arrowDown" to t
  delay 0.65
  send key "arrowUp" to t
  delay 0.65
  send key "enter" to t
  delay 2.4

  input text "clear" to t
  send key "enter" to t
  delay 2.2
EOF
      ;;
  esac
}

write_scene_script() {
  cat > "$SCENE_SCRIPT" <<EOF
on type_slow(t, txt, delay_seconds)
  repeat with ch in characters of txt
    tell application "Ghostty" to input text (contents of ch) to t
    delay delay_seconds
  end repeat
end type_slow

tell application "Ghostty"
  set targetWindow to first window whose id is "$GHOSTTY_WINDOW_KEY"
  activate window targetWindow
  set t to focused terminal of selected tab of targetWindow
$(scene_body)
end tell
EOF
}

record_demo() {
  if [[ -z "${WINDOW_ID:-}" ]]; then
    echo "Could not determine Ghostty window id" >&2
    exit 1
  fi

  rm -f "$RAW_MOV" "$MP4_OUT" "$GIF_OUT" "$POSTER_OUT"

  echo "==> recording Ghostty window $WINDOW_ID"
  "$RECORDER_BIN" "$WINDOW_ID" "$RAW_MOV" &
  RECORD_PID=$!

  osascript "$SCENE_SCRIPT"
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
  require_command open
  require_command osascript
  require_command perl
  require_command ps
  require_command swiftc

  if [[ ! -d "$GHOSTTY_APP" ]]; then
    echo "Ghostty.app not found at $GHOSTTY_APP" >&2
    exit 1
  fi

  build_slate
  build_recorder
  prepare_demo_fs
  write_prewarm_script

  echo "==> scene: $SCENE"
  launch_demo_window
  run_prewarm
  write_scene_script
  record_demo
  export_assets

  echo ""
  echo "Demo assets generated:"
  ls -lh "$MP4_OUT" "$GIF_OUT" "$POSTER_OUT"
}

main "$@"
