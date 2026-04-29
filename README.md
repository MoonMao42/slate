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
  <img src="https://img.shields.io/badge/built_with-Rust-585b70?style=flat-square&logo=rust&logoColor=white" alt="Built with Rust" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="MIT license" />
</p>

<p align="center">
  <img src="./assets/theme-demo.gif" alt="slate theme picker swapping Solarized Dark and Light" width="700" />
  <br />
  <sub>Pick a theme — slate previews the whole stack live, no reload.</sub>
</p>

## Why I built this

I could never find a terminal-beautification tool that actually fit the way I use my machine. Every time I wanted a nice setup, I ended up chasing dotfile repos, copy-pasting snippets, and stacking plugins on top of plugins. After all that effort the environment would usually end up a mess, and when I needed to recover I had to dig through everything to figure out what had actually been changed.

So I wrote slate. One command sets up a coordinated look across your terminal, prompt, fonts, and CLI tools. Everything slate writes lives in files it owns, so when you want it gone, `slate clean` actually cleans.

## Install

```bash
# macOS — Homebrew
brew install MoonMao42/tap/slate-cli

# macOS or Linux — install script
curl -fsSL https://raw.githubusercontent.com/MoonMao42/slate/main/install.sh | sh

# Rust users
cargo install slate-cli
```

Then run `slate setup`.

<p align="center">
  <img src="./assets/setup-demo.gif" alt="slate setup demo" width="600" />
  <br />
  <sub>One command: <code>slate setup</code>.</sub>
</p>

## What it does

- One palette across Ghostty, Kitty, Alacritty, Neovim, Starship, bat, delta, ls, eza, lazygit, fastfetch, tmux, and zsh-syntax-highlighting.
- 🌓 Auto dark/light pairing — native watcher on macOS, XDG Desktop Portal (with GNOME fallback) on Linux.
- Non-destructive: slate writes into managed include files and never edits your dotfiles in place. Snapshots before every change; one command to roll back.
- One visual language across every command. Headings, severity markers, and tree receipts all flow through the same render contract, so `slate setup`, `slate status`, and an error message all look like they came from the same tool.
- Small sounds on theme apply, picker navigation, setup completion, and errors. Quiet by design; turn it off with `slate config set sound off`. (v0.3.0 recordings below are silent — see footnote at the bottom of this section.)

<p align="center">
  <img src="./assets/fastfetch-preview.png" alt="fastfetch themed output" width="600" />
  <br />
  <sub>Terminal, prompt, system info, CLI utilities — same palette everywhere.</sub>
</p>

<p align="center">
  <img src="./assets/promo/list-9-families.png" alt="slate list output showing 9 theme families" width="600" />
  <br />
  <sub><code>slate list</code> — 9 family bands, Solarized landing right after Catppuccin.</sub>
</p>

<sub>* Recordings render silently in v0.3.0; a future release will refresh with audio once the curated SFX library lands.</sub>

## Neovim follows along

Slate ships 20 Neovim colorschemes mirroring every terminal family and reloads open buffers the moment you switch.

<p align="center">
  <img src="./assets/nvim-before.png" alt="Neovim with Catppuccin Frappé" width="700" />
</p>

<p align="center">
  <img src="./assets/nvim-after.png" alt="Neovim with Kanagawa Lotus" width="700" />
</p>

Works with LazyVim, kickstart.nvim, or a bare init.lua.

## Auto-theme

```
Light mode  →  your light theme + matching prompt, syntax, tools
Dark mode   →  your dark theme + matching prompt, syntax, tools
```

Enable from the hub (`slate` → Auto-Theme). Every theme family ships a built-in dark/light pair, and you can override the pairing there too.

## Support

Official targets: `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`. Linux is validated on Debian/Ubuntu + GNOME.

| Tier | Platform | Status & Notes |
|------|----------|----------------|
| Tier 1 — first-class (CI smoke-tested every release) | macOS (Apple Silicon + Intel) | Ghostty, Kitty, Alacritty, Terminal.app (partial — no live preview, no opacity, font cannot auto-apply). |
| Tier 1 — first-class (CI smoke-tested every release) | Debian / Ubuntu + GNOME (x86_64 + aarch64) | Ghostty, Kitty, Alacritty all wired up; live reload works on each. |
| Tier 2 — best effort (wired up, not in CI) | Other Linux distros (Fedora, Arch) and other desktops (KDE, Sway) | Themes still apply; live reload depends on the terminal you run. |
| Tier 3 — out of scope | Windows | No plans to support. |

Shells: `zsh`, `bash`, `fish`. `zsh` is locally verified; `bash` and `fish` are wired up but pending broader testing.

<details>
<summary><strong>Per-terminal status</strong></summary>

| Terminal | Status | Notes |
|----------|--------|-------|
| Ghostty | Recommended | Full support — live reload, opacity, watcher relaunch |
| Kitty | Full | Live palette push via `kitten @ set-colors`; opacity + Nerd Font sync |
| Alacritty | Full | Inline preview and reload |
| Terminal.app | Partial | macOS only — no live preview, no opacity, font cannot be auto-applied |
| Other | Best effort | Shell and CLI tool theming works; terminal visuals depend on the app |

</details>

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

For Ghostty: `config-file = ...`. For Kitty/Alacritty: managed `include`/`import` entries. For zsh/bash/fish: a removable marker block in the shell rc. For Neovim: a `pcall(require, 'slate')` marker block in `init.lua` (`init.vim` works too) that falls back silently if slate is uninstalled. Slate-owned files stay slate-owned; yours stay yours.

</details>

## Themes

20 variants across 9 families: Catppuccin · Solarized · Tokyo Night · Rosé Pine · Kanagawa · Everforest · Dracula · Nord · Gruvbox.

<details>
<summary><strong>All 20 variants — palette gallery</strong></summary>

Regenerate with `scripts/render-theme-gallery.sh`; future drift is caught by `tests/docs_invariants.rs`. Swatch order, left to right: background · foreground · brand accent · red.

<!-- THEME-GALLERY-START -->
<!-- generated by scripts/render-theme-gallery.sh — do NOT hand-edit; regenerate from themes/themes.toml -->

| Family | Variant | ID | Appearance | Palette |
|--------|---------|----|-----------:|---------|
| Catppuccin | Catppuccin Frappé | `catppuccin-frappe` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#303446"/><rect width="20" height="14" x="20" fill="#c6d0f5"/><rect width="20" height="14" x="40" fill="#babbf1"/><rect width="20" height="14" x="60" fill="#e78284"/></svg> |
| Catppuccin | Catppuccin Latte | `catppuccin-latte` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#eff1f5"/><rect width="20" height="14" x="20" fill="#4c4f69"/><rect width="20" height="14" x="40" fill="#7287fd"/><rect width="20" height="14" x="60" fill="#d20f39"/></svg> |
| Catppuccin | Catppuccin Macchiato | `catppuccin-macchiato` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#24273a"/><rect width="20" height="14" x="20" fill="#cad3f5"/><rect width="20" height="14" x="40" fill="#b7bdf8"/><rect width="20" height="14" x="60" fill="#ed8796"/></svg> |
| Catppuccin | Catppuccin Mocha | `catppuccin-mocha` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1e1e2e"/><rect width="20" height="14" x="20" fill="#cdd6f4"/><rect width="20" height="14" x="40" fill="#b4befe"/><rect width="20" height="14" x="60" fill="#f38ba8"/></svg> |
| Solarized | Solarized Dark | `solarized-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#002b36"/><rect width="20" height="14" x="20" fill="#839496"/><rect width="20" height="14" x="40" fill="#6c71c4"/><rect width="20" height="14" x="60" fill="#ea6e60"/></svg> |
| Solarized | Solarized Light | `solarized-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#fdf6e3"/><rect width="20" height="14" x="20" fill="#3e4d52"/><rect width="20" height="14" x="40" fill="#6c71c4"/><rect width="20" height="14" x="60" fill="#a00d0d"/></svg> |
| Tokyo Night | Tokyo Night Dark | `tokyo-night-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1a1b26"/><rect width="20" height="14" x="20" fill="#c0caf5"/><rect width="20" height="14" x="40" fill="#bb9af7"/><rect width="20" height="14" x="60" fill="#f7768e"/></svg> |
| Tokyo Night | Tokyo Night Light | `tokyo-night-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#e1e2e7"/><rect width="20" height="14" x="20" fill="#3760bf"/><rect width="20" height="14" x="40" fill="#5a4a78"/><rect width="20" height="14" x="60" fill="#9f1f63"/></svg> |
| Rosé Pine | Rose Pine Dawn | `rose-pine-dawn` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#faf4ed"/><rect width="20" height="14" x="20" fill="#575279"/><rect width="20" height="14" x="40" fill="#907aa9"/><rect width="20" height="14" x="60" fill="#a72464"/></svg> |
| Rosé Pine | Rose Pine Main | `rose-pine-main` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#191724"/><rect width="20" height="14" x="20" fill="#e0def4"/><rect width="20" height="14" x="40" fill="#c4a7e7"/><rect width="20" height="14" x="60" fill="#eb6f92"/></svg> |
| Rosé Pine | Rose Pine Moon | `rose-pine-moon` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#232136"/><rect width="20" height="14" x="20" fill="#e0def4"/><rect width="20" height="14" x="40" fill="#c4a7e7"/><rect width="20" height="14" x="60" fill="#eb6f92"/></svg> |
| Kanagawa | Kanagawa Dragon | `kanagawa-dragon` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#181616"/><rect width="20" height="14" x="20" fill="#c5d0ff"/><rect width="20" height="14" x="40" fill="#8ba4b0"/><rect width="20" height="14" x="60" fill="#ff6666"/></svg> |
| Kanagawa | Kanagawa Lotus | `kanagawa-lotus` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#f2ecbc"/><rect width="20" height="14" x="20" fill="#545464"/><rect width="20" height="14" x="40" fill="#4d699b"/><rect width="20" height="14" x="60" fill="#8e1b32"/></svg> |
| Kanagawa | Kanagawa Wave | `kanagawa-wave` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1f1f28"/><rect width="20" height="14" x="20" fill="#c8d1d8"/><rect width="20" height="14" x="40" fill="#938aa9"/><rect width="20" height="14" x="60" fill="#ff6666"/></svg> |
| Everforest | Everforest Dark | `everforest-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#1e2326"/><rect width="20" height="14" x="20" fill="#d3c6aa"/><rect width="20" height="14" x="40" fill="#a7c080"/><rect width="20" height="14" x="60" fill="#e67e80"/></svg> |
| Everforest | Everforest Light | `everforest-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#efebd4"/><rect width="20" height="14" x="20" fill="#5c6a72"/><rect width="20" height="14" x="40" fill="#8da101"/><rect width="20" height="14" x="60" fill="#9d1f1a"/></svg> |
| Dracula | Dracula | `dracula` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#282a36"/><rect width="20" height="14" x="20" fill="#f8f8f2"/><rect width="20" height="14" x="40" fill="#bd93f9"/><rect width="20" height="14" x="60" fill="#ff5555"/></svg> |
| Nord | Nord | `nord` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#2e3440"/><rect width="20" height="14" x="20" fill="#d8dee9"/><rect width="20" height="14" x="40" fill="#88c0d0"/><rect width="20" height="14" x="60" fill="#ff7777"/></svg> |
| Gruvbox | Gruvbox Dark | `gruvbox-dark` | Dark | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#282828"/><rect width="20" height="14" x="20" fill="#ebdbb2"/><rect width="20" height="14" x="40" fill="#fe8019"/><rect width="20" height="14" x="60" fill="#ff5555"/></svg> |
| Gruvbox | Gruvbox Light | `gruvbox-light` | Light | <svg width="80" height="14" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="14" x="0" fill="#fbf1c7"/><rect width="20" height="14" x="20" fill="#3c3836"/><rect width="20" height="14" x="40" fill="#af3a03"/><rect width="20" height="14" x="60" fill="#9d0006"/></svg> |

<!-- THEME-GALLERY-END -->

</details>

## Development

Built with AI assistance, with every change reviewed and tested by a human before it lands.

## License

MIT.

## Credits

Built on top of great work from others:
[Ghostty](https://ghostty.org/) · [Kitty](https://sw.kovidgoyal.net/kitty/) · [Alacritty](https://github.com/alacritty/alacritty) · [Neovim](https://neovim.io/) · [Starship](https://github.com/starship/starship) · [bat](https://github.com/sharkdp/bat) · [delta](https://github.com/dandavison/delta) · [eza](https://github.com/eza-community/eza) · [lazygit](https://github.com/jesseduffield/lazygit) · [fastfetch](https://github.com/fastfetch-cli/fastfetch) · [tmux](https://github.com/tmux/tmux) · [zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting) · [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts).
