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

### Commands

```bash
slate                         # interactive hub — theme, font, auto-theme, tool toggles
slate setup                   # guided setup wizard
slate setup --quick           # non-interactive, all defaults
slate setup --only starship   # retry a single tool
slate theme                   # live preview picker
slate theme tokyo-night-storm # apply by name
slate theme --auto            # follow system dark/light
slate font                    # Nerd Font picker
slate config set opacity 85   # window opacity (Ghostty)
slate status                  # show current config at a glance
slate list                    # list all available themes
slate restore                 # pick a snapshot to roll back to
slate restore --list          # list restore points
slate clean                   # remove all slate-managed config
```

## Auto-Theme

Slate can follow your macOS system appearance. When dark mode toggles, your terminal theme switches automatically.

```bash
slate config set auto-theme on     # enable
slate theme --auto                 # apply once based on current appearance
```

Configure which themes map to dark and light through the hub (`slate` → Auto-Theme → Configure Pairing), or let Slate use built-in pairs like Catppuccin Mocha ↔ Latte.

## How It Works

Slate composes managed config files into your existing setup — it never replaces your dotfiles.

```text
~/.config/slate/config.toml        # preferences (theme, font, toggles)
~/.config/slate/auto.toml          # dark/light theme pairing
~/.config/slate/managed/<tool>/*   # generated assets Slate can fully rewrite
~/.config/<tool>/...               # your files, untouched — import managed files
```

For Ghostty that means `config-file = ...`; for Alacritty that means managed `import` entries; for shell startup that means a removable marker block in `.zshrc`. Slate files stay Slate-owned, your files stay yours.

## Install

```bash
brew install MoonMao42/tap/slate    # Homebrew (recommended)
brew upgrade slate                  # update
```

Binaries are also available from [GitHub Releases](https://github.com/MoonMao42/slate-dev/releases).

### Uninstall

```bash
slate clean          # remove managed config, shell hooks, watcher artifacts
brew uninstall slate
```

To also purge restore snapshots, remove `~/.cache/slate` after cleanup.

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
