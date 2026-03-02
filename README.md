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
  One command. 18 themes. Every tool in sync.
</p>

<p align="center">
  <a href="https://github.com/MoonMao42/slate-dev/releases"><img src="https://img.shields.io/github/v/release/MoonMao42/slate-dev?style=flat-square&color=585b70" alt="Latest release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS-585b70?style=flat-square" alt="macOS only" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="MIT license" />
</p>

<p align="center">
  <img src="./assets/theme-demo.gif" alt="slate theme live preview" width="700" />
  <br />
  <sub>Live preview across 18 curated themes — dark, light, frosted.</sub>
</p>

## Quick Start

> Requires **macOS** and **[Homebrew](https://brew.sh)**. Best experience with **[Ghostty](https://ghostty.org)** (live reload, frosted glass, auto-theme relaunch). Also supports Kitty, Alacritty, and Terminal.app.

```bash
brew install MoonMao42/homebrew-tap/slate
slate setup
```

If Homebrew is not ready on that Mac yet, download the matching binary from [GitHub Releases](https://github.com/MoonMao42/slate-dev/releases) as the fallback path.

<p align="center">
  <img src="./assets/setup-demo.gif" alt="slate setup demo" width="600" />
  <br />
  <sub>Detects your stack, installs what's missing, applies a coordinated theme — all in one command.</sub>
</p>

## Features

- **One palette, everywhere** — Ghostty, Kitty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting all share the same color scheme.
- **Auto dark/light** — Ghostty can relaunch the watcher automatically. Other terminals can still follow macOS while the watcher is running, but restart recovery is more manual.
- **Live preview** — Browse 18 themes with instant terminal preview. Arrow keys to navigate, Enter to apply.
- **Nerd Font management** — Detect, install, and switch fonts without leaving the terminal.
- **Non-destructive** — Uses managed includes, never overwrites your dotfiles. Snapshots before every change, one-command rollback.
- **Shareable** — Export your setup as a URI, import on another machine, or screenshot with a branded watermark.

<p align="center">
  <img src="./assets/fastfetch-preview.png" alt="fastfetch themed output" width="600" />
  <br />
  <sub>Every tool picks up the same palette — terminal, prompt, system info, CLI utilities.</sub>
</p>

## 🌗 Auto-Theme

```
☀️  Light Mode  →  your light theme + matching prompt, syntax, tools
🌙  Dark Mode   →  your dark theme + matching prompt, syntax, tools
```

Enable from the hub (`slate` → Auto-Theme) or:

```bash
slate config set auto-theme enable
```

Every theme family ships a built-in dark/light pair. Configure your own pairing through the hub.

Ghostty is the polished path here. In Terminal.app and other non-Ghostty terminals, Slate will keep shell/tool colors in sync but will not promise blur, automatic font switching, or watcher relaunch after every restart.

<details>
<summary><strong>All Commands</strong></summary>

```bash
slate                         # interactive hub
slate setup                   # guided setup wizard
slate setup --quick           # non-interactive, all defaults
slate setup --only starship   # retry a single tool
slate theme                   # live preview picker
slate theme <name>            # apply by name
slate theme --auto            # follow system dark/light
slate font                    # Nerd Font picker
slate config set opacity frosted  # window opacity (solid/frosted/clear)
slate config set sound off    # toggle feedback sound
slate export                  # export config as shareable URI
slate import <uri>            # apply config from URI
slate share                   # screenshot terminal with watermark
slate status                  # show current config at a glance
slate list                    # list all available themes
slate restore                 # pick a snapshot to roll back to
slate restore --list          # list restore points
slate clean                   # remove all slate-managed config
```

</details>

<details>
<summary><strong>How It Works</strong></summary>

Slate composes managed config files into your existing setup — it never replaces your dotfiles.

```text
~/.config/slate/config.toml        # preferences (theme, font, toggles)
~/.config/slate/auto.toml          # dark/light theme pairing
~/.config/slate/managed/<tool>/*   # generated assets Slate can fully rewrite
~/.config/<tool>/...               # your files, untouched
```

For Ghostty: `config-file = ...`. For Kitty/Alacritty: managed `include`/`import` entries. For zsh: a removable marker block in `.zshrc`. Slate files stay Slate-owned, your files stay yours.

</details>

## Install

```bash
brew install MoonMao42/homebrew-tap/slate
brew upgrade slate
```

If `brew` is not available yet, use the matching binary from [GitHub Releases](https://github.com/MoonMao42/slate-dev/releases) and then run `slate setup`.

```bash
# Uninstall
slate clean && brew uninstall slate
```

## Themes

18 curated variants across 8 families: **Catppuccin** · **Tokyo Night** · **Rosé Pine** · **Kanagawa** · **Everforest** · **Dracula** · **Nord** · **Gruvbox**

## License

MIT

## Credits

- [Ghostty](https://ghostty.org/) · [Kitty](https://sw.kovidgoyal.net/kitty/) · [Alacritty](https://github.com/alacritty/alacritty) · [Starship](https://github.com/starship/starship) · [bat](https://github.com/sharkdp/bat) · [delta](https://github.com/dandavison/delta) · [eza](https://github.com/eza-community/eza) · [lazygit](https://github.com/jesseduffield/lazygit) · [fastfetch](https://github.com/fastfetch-cli/fastfetch) · [tmux](https://github.com/tmux/tmux) · [zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting) · [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts)
