# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-17

### Added
- Interactive setup wizard (`slate setup` / `slate setup --quick`)
- Theme picker with live preview and hot-reload (`slate theme`)
- 18 built-in themes across 8 families (Catppuccin, Tokyo Night, Rose Pine, Kanagawa, Everforest, Dracula, Nord, Gruvbox)
- 11 tool adapters: Ghostty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting, Nerd Font
- Auto-theme: follow macOS dark/light mode with event-driven Swift watcher
- Opacity presets (Solid / Frosted / Clear) for Ghostty and Alacritty
- Interactive hub menu (`slate` with no args)
- Status dashboard (`slate status`)
- Theme listing with family grouping (`slate list`)
- Snapshot-based restore system (`slate restore`)
- Font management (`slate font`)
- Three-tier config architecture (managed / integration / user override)
- Homebrew tap distribution (`brew install MoonMao42/homebrew-tap/slate-cli`)

[0.1.0]: https://github.com/MoonMao42/slate/releases/tag/v0.1.0
