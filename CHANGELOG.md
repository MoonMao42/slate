# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2026-04-29

### Fixed
- `install.sh` now downloads the correct `slate-cli-<target>.tar.xz` asset
  cargo-dist actually publishes; v0.3.0 was looking for `slate-<target>.tar.gz`
  and would fail outright on a fresh `curl ... | sh` install.
- `bat` cache rebuild now falls back to the `batcat` binary, so theme apply
  no longer no-ops on Debian/Ubuntu where bat ships under that name.
- Short-lived `slate` invocations now flush the SFX queue before exit, so the
  apply / success / failure sounds actually play instead of being dropped when
  the process tears down inside the 60ms coalesce window.
- `ensure_cache` rewrites WAV samples whose contents differ from the embedded
  bytes, so an upgrade that ships fresher samples actually replaces stale
  files in `~/.cache/slate/sounds`.

### Changed
- Homebrew install line tightened to `brew install MoonMao42/tap/slate-cli`
  (Homebrew expands `tap` → `homebrew-tap` automatically). The README,
  CHANGELOG, distribution doc, and release-time tap check all match.
- `scripts/render-theme-gallery.sh` now ships in the published crate so
  downstream packagers can regenerate the README's swatch table from
  `themes/themes.toml` without cloning the repo.

## [0.3.0] - 2026-04-28

### Added
- Solarized Dark and Solarized Light — Ethan Schoonover's precision-engineered
  palette joins as slate's 19th and 20th built-in themes, with full coverage
  across all 10 tool backends. `slate theme set solarized-dark` /
  `solarized-light`; auto-pair flips between them when system appearance
  changes.
- Theme listing now groups variants by family across both `slate theme --list`
  and the live browser — Solarized lands at index 1 right after Catppuccin so
  the new band is impossible to miss. Run `slate theme --list` to see the
  9-band layout in the terminal.
- Subtle interaction sound — short, brand-coherent SFX on theme apply, menu
  navigation, setup completion, and errors. Default on, opt-out via
  `slate config set sound off`. Respects `--quiet` and `--auto`. (v0.3.0
  ships with placeholder samples; the curated SFX library lands in a future release.)

### Changed
- Unified visual language across every command via the new `Roles<'a>`
  text-role system — headings, severity markers, paths, shortcuts, and tree
  receipts share one render contract. Brand glyphs use the signature
  lavender; errors and success route through the active theme's red and
  green. Every user-facing surface (setup wizard, `slate theme`, status,
  clean, restore, config, share, demo, browse chrome, error and reminder
  paths) emits through the same API.
- 12 WCAG contrast repairs across the Solarized variants (4 dark + 8 light).
  Every theme-token pair now clears the 4.5:1 readability bar; the canonical
  Schoonover hex stays preserved alongside the repaired ANSI slots.
- README rewritten for v0.3.0 — new hero recording, 9-family theme gallery,
  Tier 1 / 2 / 3 platform matrix, and an honest accounting of what is and
  isn't supported.

### Fixed
- **Solarized powerline contrast**: `bg_darkest` now anchors at Solarized's
  `base03` instead of inheriting from a brighter slot, so starship powerline
  pills render crisp text on both Dark and Light variants.
- **delta light/dark drift**: the delta adapter now writes the
  appearance-correct flag per theme, instead of hard-coding `dark = true`.
  Solarized Light syntax now renders correctly out of the box.
- **`slate theme --list` ergonomics**: the listing surface picked up an
  explicit `--list` flag and a display-name lookup, plus a few small
  adapter cleanups discovered during Solarized UAT.

## [0.2.0] - 2026-04-20

### Added
- Neovim adapter — slate now themes Neovim alongside the terminal. 18 curated colorschemes covering every built-in family, plus font sync and a watcher-style live reload so open buffers update when you switch theme. Opt out with `slate config set editor disable`.
- Live-preview theme picker (`slate theme`) — browse variants with the whole stack (terminal, prompt, nvim, utilities) rendering the previewed theme in real time. Commit with Enter, discard with Esc; the previous state is snapshotted first so Esc is free.
- Kitty support reaches feature parity with Ghostty — live color push via `kitten @ set-colors`, opacity presets, Nerd Font sync.
- Full support for Fish and Bash shells (in addition to Zsh).

### Changed
- Unified visual language across every command — headers, tree receipts, and severity markers now share one system. Errors always render in the theme's red, success in theme's green, brand glyphs in the signature lavender.
- `slate demo` retired in favour of the always-on picker preview. The old one-shot renderer is gone.
- Every adapter now accepts an injected `SlateEnv` so the preview path and the commit path go through the same code — no more "preview looked fine but set broke".

### Fixed
- **Baseline restore**: restoring to the pre-slate baseline no longer immediately re-applies the current theme on top. Rolling back to `baseline-pre-slate` now actually returns your configs to how they were before slate was installed.
- **Kitty blind spot**: `~/.config/kitty/kitty.conf` is now captured in the baseline snapshot and swept by `slate clean`. Previously slate's `include` directives and live-preview `listen_on` line survived both `clean` and `restore baseline`, leaving the terminal still themed.
- **Nvim blind spot**: `~/.config/nvim/init.lua` and `init.vim` are now captured in the baseline snapshot. The slate marker block (`pcall(require, 'slate')`) can be cleanly rolled back.
- **Starship cleanup**: `slate clean` now reverts `palette = "slate"` and `[palettes.slate]` edits in your `starship.toml`. Previously the prompt stayed themed even after uninstalling everything else.
- **Non-UTF-8 dotfiles**: stray non-UTF-8 bytes in `starship.toml` or `alacritty.toml` now produce an actionable error naming the file, the byte offset, and an `xxd` hint — no more opaque "stream did not contain valid UTF-8".
- Picker polish: palette swatch labels now align, Tab affordance is visible, full-preview mode is locked against accidental mutation during browsing.

## [0.1.2] - 2026-04-18

### Added
- `LS_COLORS` and `EZA_COLORS` generated from the active palette, so `ls`, `gls`, `eza`, lazygit, and anything reading those env vars picks up the theme
- `slate demo` sub-command renders the active palette for a quick visual check
- `✦ ⌘N for a fresh shell` reminder after operations that change env vars; suppressed under `--auto` / `--quiet`
- macOS-only: first `slate setup` without GNU coreutils suggests `brew install coreutils` (one-shot, gated by a marker file)

### Changed
- `slate theme <name> --quiet` is now fully silent
- Each adapter's `ApplyOutcome` now carries `requires_new_shell` so the shell reminder only fires when something actually changed

## [0.1.1] - 2026-04-17

### Fixed
- `slate clean` now reloads Ghostty so the active terminal actually drops the theme background instead of holding the palette until the next launch
- `slate clean` writes a pre-clean restore point first, so `slate restore` can bring the previous state back (previously the only target was the pre-install baseline)
- Manual `/Applications/Ghostty.app` installs are now auto-configured during setup (were silently skipped because the app bundle tiered as "fallback")
- `slate setup` auto-queues fastfetch install when autorun is enabled but the binary isn't on PATH — and the hub toggle warns instead of enabling a silent no-op

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
- Homebrew tap distribution (`brew install MoonMao42/tap/slate-cli`)

[0.3.1]: https://github.com/MoonMao42/slate/releases/tag/v0.3.1
[0.3.0]: https://github.com/MoonMao42/slate/releases/tag/v0.3.0
[0.2.0]: https://github.com/MoonMao42/slate/releases/tag/v0.2.0
[0.1.2]: https://github.com/MoonMao42/slate/releases/tag/v0.1.2
[0.1.1]: https://github.com/MoonMao42/slate/releases/tag/v0.1.1
[0.1.0]: https://github.com/MoonMao42/slate/releases/tag/v0.1.0
