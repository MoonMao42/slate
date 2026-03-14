<p align="center">
  <img
    width="180"
    src="./assets/logo-icon.svg"
    alt="slate logo"
  />
</p>

<h1 align="center">slate</h1>

<p align="center">
  A one-command terminal setup for macOS and Linux — themes, prompts, fonts, and tools all in sync.
</p>

<p align="center">
  English · <a href="./README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="https://github.com/MoonMao42/slate/releases"><img src="https://img.shields.io/github/v/release/MoonMao42/slate?style=flat-square&color=585b70" alt="Latest release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-585b70?style=flat-square" alt="macOS and Linux" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="MIT license" />
</p>

<p align="center">
  <img src="./assets/theme-demo.gif" alt="slate theme live preview" width="700" />
  <br />
  <sub>Browse 18 curated themes inline — with live push on terminals that support it.</sub>
</p>

## Why I built this

I could never find a terminal-beautification tool that actually fit the way I use my machine. Every time I wanted a nice setup, I ended up chasing dotfile repos, copy-pasting snippets from other people's configs, and stacking plugins on top of plugins. The results looked okay, but the state left behind was a mess — scattered files under `~/.config`, orphaned plugin managers, shell startup blocks I couldn't remember installing. When I tried to undo it, I usually couldn't.

So I wrote slate. One command sets up a coordinated look across your terminal, prompt, fonts, and CLI tools. Everything slate writes lives in files it owns, so when you want it gone, `slate clean` actually cleans.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/MoonMao42/slate/main/install.sh | sh
slate setup
```

Or on macOS with Homebrew:

```bash
brew install MoonMao42/homebrew-tap/slate
```

Uninstall:

```bash
slate clean && rm "$(which slate)"
```

## Support matrix

Official targets: `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`. Linux is validated on Debian/Ubuntu + GNOME; slate prefers XDG Desktop Portal where available and falls back honestly when it can't.

| Layer | macOS | Linux |
|-------|-------|-------|
| Desktop appearance | `defaults` + embedded Swift watcher | XDG Desktop Portal first, GNOME `gsettings` fallback |
| Share capture | `screencapture` | XDG Desktop Portal first, `gnome-screenshot` fallback |
| Package installs | Homebrew | `apt` |
| Fonts | `~/Library/Fonts` | `~/.local/share/fonts` + `fc-cache` |
| Shell loaders | `.zshrc`, `.bashrc`, `~/.config/fish/conf.d/slate.fish` | same |

| Terminal | Status | Notes |
|----------|--------|-------|
| Ghostty | Best experience | Live reload, opacity, watcher relaunch where the backend supports it |
| Kitty | Supported | Live push via remote control, macOS and Linux |
| Alacritty | Partial | Inline preview works; live reload is best-effort |
| Terminal.app | Partial | macOS only; manual font selection, no blur or live push |
| Everything else | Best effort | Shared shell/tool theming works; terminal-specific visuals depend on the app |

<p align="center">
  <img src="./assets/setup-demo.gif" alt="slate setup demo" width="600" />
  <br />
  <sub>Detects your stack, installs what's missing, applies a coordinated theme.</sub>
</p>

## What it does

- One palette across Ghostty, Kitty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, and zsh-syntax-highlighting.
- Auto dark/light pairing backed by a native watcher on macOS and XDG Desktop Portal (with GNOME fallback) on Linux.
- Inline theme picker on every supported platform. Ghostty and Kitty also get live push when available.
- Nerd Font detection and install, using the platform's real user-font directory.
- Non-destructive: slate writes into managed include files and never edits your dotfiles in place. It snapshots before every change and can roll back with one command.
- Shareable: export your setup as a URI, re-apply on another machine, or capture a branded screenshot when the platform backend allows.

<p align="center">
  <img src="./assets/fastfetch-preview.png" alt="fastfetch themed output" width="600" />
  <br />
  <sub>Terminal, prompt, system info, CLI utilities — same palette everywhere.</sub>
</p>

## Auto-theme

```
Light mode  →  your light theme + matching prompt, syntax, tools
Dark mode   →  your dark theme + matching prompt, syntax, tools
```

Enable from the hub (`slate` → Auto-Theme), or:

```bash
slate config set auto-theme enable
```

Every theme family ships a built-in dark/light pair. You can override the pairing from the hub.

Platform notes:

- macOS uses an embedded Swift watcher.
- Linux prefers XDG Desktop Portal, with GNOME `gsettings` as fallback.
- Ghostty can relaunch the watcher from new shell sessions. Other terminals keep the shared theme logic but don't promise the same restart behavior.

<details>
<summary><strong>All commands</strong></summary>

```bash
slate                         # interactive hub
slate setup                   # guided setup
slate setup --quick           # non-interactive, defaults
slate setup --only starship   # retry a single tool
slate theme                   # live preview picker
slate theme <name>            # apply by name
slate theme --auto            # follow system dark/light
slate font                    # Nerd Font picker
slate config set opacity frosted  # solid / frosted / clear
slate config set sound off    # toggle feedback sound
slate export                  # export current config as URI
slate import <uri>            # re-apply from URI
slate share                   # screenshot terminal with watermark
slate status                  # show current config
slate list                    # list available themes
slate restore                 # pick a snapshot to roll back
slate restore --list          # list restore points
slate clean                   # remove everything slate wrote
```

</details>

<details>
<summary><strong>How it works</strong></summary>

Slate composes managed config files alongside your existing setup rather than replacing your dotfiles.

```text
~/.config/slate/config.toml        # preferences (theme, font, toggles)
~/.config/slate/auto.toml          # dark/light theme pairing
~/.config/slate/managed/<tool>/*   # generated assets slate owns
~/.config/<tool>/...               # your files, untouched
```

For Ghostty: `config-file = ...`. For Kitty/Alacritty: managed `include`/`import` entries. For zsh: a removable marker block in `.zshrc`. Slate-owned files stay slate-owned; yours stay yours.

</details>

## Themes

18 variants across 8 families: Catppuccin · Tokyo Night · Rosé Pine · Kanagawa · Everforest · Dracula · Nord · Gruvbox.

## License

MIT.

## Credits

Built on top of great work from others:
[Ghostty](https://ghostty.org/) · [Kitty](https://sw.kovidgoyal.net/kitty/) · [Alacritty](https://github.com/alacritty/alacritty) · [Starship](https://github.com/starship/starship) · [bat](https://github.com/sharkdp/bat) · [delta](https://github.com/dandavison/delta) · [eza](https://github.com/eza-community/eza) · [lazygit](https://github.com/jesseduffield/lazygit) · [fastfetch](https://github.com/fastfetch-cli/fastfetch) · [tmux](https://github.com/tmux/tmux) · [zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting) · [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts).
