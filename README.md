<p align="center">
  <img
    width="180"
    src="./assets/logo-icon.svg"
    alt="slate logo"
  />
</p>

<h1 align="center">slate</h1>

<p align="center">
  <strong>30 seconds to a beautiful macOS terminal.</strong><br />
  Curated terminal theming for people who want taste, not dotfile archaeology.
</p>

<p align="center">
  <a href="https://github.com/MoonMao42/slate-dev/releases"><img src="https://img.shields.io/github/v/release/MoonMao42/slate-dev?style=flat-square&color=585b70" alt="Latest release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS-585b70?style=flat-square" alt="macOS only" />
  <img src="https://img.shields.io/badge/rust-stable-585b70?style=flat-square" alt="Rust stable" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="MIT license" />
</p>

<p align="center">
  <img src="./assets/theme-demo.gif" alt="slate theme live preview" width="700" />
  <br />
  <sub>Live preview across 18 curated themes — dark, light, frosted.</sub>
</p>

## Quick Start

```bash
brew install MoonMao42/tap/slate
slate setup
```

<p align="center">
  <img src="./assets/setup-demo.gif" alt="slate setup demo" width="600" />
  <br />
  <sub>Detects your stack, installs what's missing, applies a coordinated theme — all in one command.</sub>
</p>

## Why Slate

Most terminal setup guides hand you a bag of unrelated configs and tell you to sort it out yourself.

Slate takes the opposite approach:

- One command configures a coherent stack instead of 11 disconnected tools.
- Managed files are composed into your setup instead of replacing your dotfiles.
- Backups are created before mutation, and cleanup removes Slate-owned shell hooks cleanly.
- The visual language is intentionally curated: minimal, premium, and macOS-native rather than endlessly tweakable for its own sake.

## What You Get

| Layer | Tools |
| --- | --- |
| Terminal chrome | Ghostty, Alacritty |
| Prompt and shell | Starship, zsh-syntax-highlighting |
| Daily CLI tools | bat, delta, eza, lazygit, fastfetch, tmux |
| Typography | Nerd Font detection, install, and switching |
| Theme logic | 18 curated variants across 8 families |

### Daily Commands

```bash
slate setup --quick
slate theme
slate theme tokyo-night-storm
slate theme --auto
slate font
slate status
slate clean
slate restore
```

## Install Channels

| Channel | Status | Why it exists |
| --- | --- | --- |
| Homebrew tap | Primary | Best default for macOS users. Minimal friction, native updates, no Rust toolchain required. |
| GitHub Releases | Required | Direct binary download, checksums, and release notes. Also powers Homebrew formula distribution. |
| crates.io | Prepared | Important Rust-native path for discoverability and ecosystem compatibility. Publish once release assets and package metadata are finalized. |

Homebrew is the opinionated user-facing path. GitHub Releases are the binary source of truth. `crates.io` should exist too, but as a secondary install path instead of the main onboarding story.

## Compatibility Strategy

Slate aims for **semantic consistency** and **graceful visual degradation**.

- Colors, theme identity, and managed-file composition should stay consistent everywhere Slate supports.
- Terminal-specific chrome such as blur, opacity, and live reload can degrade when the host terminal does not expose identical capabilities.
- If a user selects a non-Nerd Font or the machine has no Nerd Font installed, Slate now switches Starship to a basic prompt profile for new shells instead of leaving powerline glyphs to render as tofu.
- Shell activation is scoped to a managed marker block so PATH changes and environment exports disappear cleanly on `slate clean`.

That means we do **not** chase pixel-perfect parity across every terminal at any cost. We keep the important parts consistent, and we degrade intentionally when an emulator cannot deliver the same surface area.

## Configuration Architecture

Slate already follows a composition model that is closer to Ghostty and Alacritty than to copy-paste dotfile repos:

```text
~/.config/slate/config.toml        # Slate-owned feature flags
~/.config/slate/auto.toml          # light/dark theme pairing
~/.config/slate/managed/<tool>/*   # generated assets Slate can fully rewrite
~/.config/<tool>/...               # user entry files that import/include managed files
```

The guiding rule is:

- user files stay user-owned
- Slate files stay Slate-owned
- integration happens through import/include layers, not destructive replacement

For Ghostty that means `config-file = ...`; for Alacritty that means managed `import` entries; for shell startup that means a removable marker block in `.zshrc`.

## Lifecycle

### Install

```bash
brew install MoonMao42/tap/slate
slate setup
```

### Update

```bash
brew upgrade slate
```

### Remove Slate Cleanly

```bash
slate clean
brew uninstall slate
```

`slate clean` removes Slate-owned shell integration, watcher artifacts, and managed config state. If you also want to purge restore snapshots, remove `~/.cache/slate` manually after cleanup.

For repeatable verification, run [`scripts/test-install-lifecycle.sh`](scripts/test-install-lifecycle.sh). It runs isolated lifecycle smoke tests for setup, cleanup, fallback mode, and terminal hook removal without touching your real home directory.

## Theming

Slate ships 18 curated variants across these families:

- Catppuccin
- Tokyo Night
- Rosé Pine
- Kanagawa
- Everforest
- Dracula
- Nord
- Gruvbox

The project philosophy is to curate a small, confident palette library and make every supported tool agree with it.

## Demo Pipeline

The README demos are recorded from a real Ghostty window using [Recordly](https://recordly.app/), then optimized with ffmpeg palette-based GIF encoding for GitHub autoplay.

## License

MIT.

## Credits

Slate stands on the shoulders of great tools and great visual systems.

- Terminal inspiration: [Ghostty](https://ghostty.org/), [Alacritty](https://github.com/alacritty/alacritty)
- Prompt layer: [Starship](https://github.com/starship/starship)
- Tooling ecosystem: [bat](https://github.com/sharkdp/bat), [delta](https://github.com/dandavison/delta), [eza](https://github.com/eza-community/eza), [lazygit](https://github.com/jesseduffield/lazygit), [fastfetch](https://github.com/fastfetch-cli/fastfetch), [tmux](https://github.com/tmux/tmux), [zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting)
- Font ecosystem: [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts)
- Theme families: Catppuccin, Tokyo Night, Rosé Pine, Kanagawa, Everforest, Dracula, Nord, Gruvbox

Open source deserves visible credit. If Slate borrows an idea, palette, or integration pattern, it should say so.
