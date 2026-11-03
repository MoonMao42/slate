#!/usr/bin/env bash
# Read-only rendering probe. Run directly and inside tmux/SSH after theme changes.
set -eu
printf 'ANSI foreground/background combinations (columns = foreground)\n'
printf 'bg   '
for ((fg=0; fg<16; fg++)); do printf ' %02d ' "$fg"; done
printf '\n'
for ((bg=0; bg<16; bg++)); do
  printf '%02d   ' "$bg"
  for ((fg=0; fg<16; fg++)); do
    printf '\033[38;5;%d;48;5;%dm Aa \033[0m' "$fg" "$bg"
  done
  printf '\n'
done
printf '\nDefault background: normal / bold / faint / reverse\n'
for attr in 0 1 2 7; do
  printf '%d    ' "$attr"
  for ((fg=0; fg<16; fg++)); do
    printf '\033[%d;38;5;%dm Aa \033[0m' "$attr" "$fg"
  done
  printf '\n'
done
printf '\nCommon application combinations\n'
printf '\033[1;37;42m NAT: Open \033[0m  \033[1;37;43m NAT: Restricted \033[0m  \033[1;37;41m NAT: Error \033[0m\n'
printf '\033[30;46m htop selected \033[0m  \033[30;42m sticky dir \033[0m  \033[37;44m status label \033[0m\n'
printf '\033[38;5;240m 256-color gray \033[0m  \033[38;5;244;48;5;245m indexed on indexed \033[0m\n'
printf '\033[38;2;40;40;40;48;2;45;45;45m truecolor dark pair \033[0m  \033[38;2;230;230;230;48;2;235;235;235m truecolor light pair \033[0m\n'
printf '\033[2;38;2;128;128;128m faint truecolor \033[0m  \033[7;38;5;240;48;5;241m reverse indexed \033[0m\n'
