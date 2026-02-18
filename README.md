<p align="center">
  <img
    width="320"
    src="assets/logo.svg"
    alt="slate — terminal aesthetics suite"
  />
</p>

<p align="center">
  <strong>30 seconds to a beautiful macOS terminal.</strong>
</p>

<p align="center">
  <a href="https://github.com/maokaiyue/slate/releases"><img src="https://img.shields.io/github/v/release/maokaiyue/slate?style=flat-square&color=585b70" alt="Release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS-585b70?style=flat-square" alt="macOS" />
  <img src="https://img.shields.io/badge/license-MIT-585b70?style=flat-square" alt="License" />
</p>

---

<!-- Hero demo goes here -->
<!-- <p align="center"><img src="assets/demo.gif" width="720" /></p> -->

## Quick Start

```bash
brew install maokaiyue/tap/slate
slate setup
```

That's it. Slate detects your terminal and tools, installs what's missing, and applies a curated theme — fonts, colors, prompt, and all.

## See It in Action

<!-- Live preview picker demo goes here -->
<!-- <p align="center"><img src="assets/picker.gif" width="720" /></p> -->

Run `slate theme` to open the interactive picker. Arrow keys to browse, live preview as you go, Enter to commit.

## Day & Night

<!-- Dark/light comparison image goes here -->
<!-- <p align="center"><img src="assets/day-night.png" width="720" /></p> -->

`slate theme --auto` follows macOS appearance. Switch to dark mode and your entire terminal stack updates instantly — no manual intervention.

## Ecosystem

Slate unifies the look of your entire terminal environment in one command:

| Terminals | Shell & Prompt | CLI Tools |
|-----------|---------------|-----------|
| Ghostty | Starship | bat |
| Alacritty | zsh-syntax-highlighting | eza |
| | | delta |
| | | lazygit |
| | | fastfetch |
| | | tmux |

Plus Nerd Font detection and installation.

## Everyday Usage

```bash
slate theme              # interactive picker with live preview
slate theme tokyo-night  # switch directly
slate theme --auto       # follow macOS dark/light mode
slate font               # change your Nerd Font
slate status             # see what's configured
slate list               # browse all 18 themes
```

## The Safety Net

Slate backs up your configs before touching anything. If something goes wrong:

```bash
slate restore            # pick a snapshot to roll back to
slate clean              # remove everything slate added
```

## 18 Curated Themes

<!-- Gallery: 4 signature themes as large screenshots -->
<!-- Collapsible section with full 18-theme matrix -->

<details>
<summary><strong>Explore all 18 themes</strong></summary>

**Catppuccin** — Mocha, Macchiato, Frappe, Latte
**Tokyo Night** — Night, Storm
**Rose Pine** — Main, Moon, Dawn
**Kanagawa** — Wave, Dragon, Lotus
**Everforest** — Dark, Light
**Dracula** — Classic
**Nord** — Classic
**Gruvbox** — Dark, Light

</details>

## Command Reference

<details>
<summary><strong>Full command list</strong></summary>

| Command | Description |
|---------|------------|
| `slate` | Status dashboard + guided action |
| `slate setup` | First-time setup wizard |
| `slate setup --quick` | One-click defaults |
| `slate theme [NAME]` | Switch theme or open picker |
| `slate theme --auto` | Auto-follow macOS appearance |
| `slate font [NAME]` | Switch Nerd Font |
| `slate status` | Current configuration |
| `slate list` | All available themes |
| `slate config set KEY VALUE` | Configure settings |
| `slate clean` | Remove slate configurations |
| `slate restore [ID]` | Restore from snapshot |

</details>

## Philosophy

> We sell taste, not code.

Slate exists because configuring 11 tools to look good together is painful. We package designer-verified setups so you don't think about color theory — just pick a vibe and go.

---

<p align="center">
  Built with Rust. Themed with care.
</p>
