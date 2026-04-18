# slate

## What This Is

A terminal beautification kit written in Rust. One command transforms a plain terminal into a polished, cohesive setup across macOS and Linux — installing and configuring Ghostty, Starship, bat, delta, eza, lazygit, Nerd Fonts, and more with a unified color theme. Like NvChad for your entire terminal stack, but with a shared core and platform-specific backends instead of one-off OS forks.

## Core Value

30-second Time-to-Dopamine: from `brew install` to a stunning terminal. We sell taste, not code.

## Product Principles

- **Time-to-Dopamine** — From install to "wow" in ≤ 30 seconds. Every design decision serves this metric.
- **Sell taste, not code** — Users don't know color theory or font pairing. We pre-package designer-verified setups. Out-of-the-box premium feel is the core value.
- **Transparent, never sneaky** — Show the full action list before executing. Never modify the system without the user seeing what's about to happen. Trust = adoption.
- **Idempotent always** — Run `slate setup` 10 times, get the same result as running it once. Always safe.
- **Composition over override** — Write base configs to managed directories; respect user customizations in separate files. Never overwrite what the user created. Three-tier: managed → integration → user.
- **Premium in every detail** — The name is "slate." Even error messages must be beautifully formatted with colors and clean typography. No cheap-feeling output anywhere. Brand language throughout (playful, never generic).
- **Lower the aesthetic barrier** — Like wallpaper apps for phones. Don't require users to understand design — give them curated, beautiful options.
- **Never install without consent** — Detect missing tools, explain what they do and why they're beautiful, ask permission. Friendly recommendation, not forced installation.

## Current State

**Shipped:** v2.1 Cross-Platform Core (2026-04-17) — 5 phases, 15 plans, 18 tasks.
**In progress:** v2.2 Editor Ecosystem + Polish — Phase 15 (`slate demo`) complete (2026-04-18). 14 new `SemanticColor` variants, exhaustive `Palette::resolve`, shared `file_type_colors` module for Phase 16, 4-block single-screen showcase hitting all 16 ANSI slots, once-only hint after `setup` and `theme <id>`.

Slate is now a macOS + Linux shared-core CLI. Platform-specific behavior flows through explicit capability interfaces (`DesktopAppearanceBackend`, `ShareCaptureBackend`, `FontPlatformBackend`, `PackageManagerBackend`, `ShellBackend`). Shell integration ships for `zsh`, `bash`, and `fish` from one managed environment model. Linux baseline is Debian/Ubuntu + GNOME with `apt` as the formal package-manager backend; portal-aware appearance/share fall back gracefully on other desktop stacks. Releases publish four targets (`x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`) with a shared `install.sh`, and CI runs fmt/clippy/build/tests on macOS + Ubuntu.

## Current Milestone: v2.2 Editor Ecosystem + Polish

**Goal:** Extend slate's "one palette across the stack" story into CLI file tools, the editor ecosystem, and interaction polish — while keeping editor-adapter scope honest by researching before building.

**Target features:**
- `slate demo` — curated palette showcase (code block, dir tree, git log, progress) rendered on demand so users see the payoff without hunting for it
- `LS_COLORS` and `EZA_COLORS` managed outputs driven by the active palette; macOS BSD-`ls` users get a one-time `coreutils` suggestion instead of a lossy 8-color fallback
- Cross-platform "new-terminal" reminder system — adapters declare `RequiresNewShell`, reminders surface per-run with platform-appropriate phrasing (macOS `[⌘N]`, Linux "open a new terminal")
- Vim/Neovim plugin-absorption research spike — evaluate upstream colorscheme plugins (catppuccin-nvim, tokyonight.nvim, gruvbox.nvim, etc.) for license + portability; produce a go/no-go recommendation for the v2.3 editor adapter
- Solarized dark + light palettes added; existing `family` grouping surfaced in `slate theme --list` / picker so users can navigate by family

**Deferred to later milestones:** VSCode adapter (JSON merge fragility); full vim/nvim editor adapter (depends on research outcome, targeted at v2.3); `slate export` (palette JSON/env var export for long-tail tools).

## Requirements

### Validated

- [x] Single command switches theme across Ghostty, Starship, bat, delta, lazygit — Validated in Milestone 1
- [x] Auto-detect installed tools and config file locations (macOS) — Validated in Milestone 1
- [x] Non-destructive config modification with comment preservation — Validated in Milestone 1
- [x] First-class Catppuccin (4 variants), Tokyo Night (2), Dracula, Nord themes — Validated in Milestone 1
- [x] Adapter/plugin architecture with trait-based interface — Validated in Milestone 1
- [x] First-run baseline backup and restore-point metadata substrate — Trust slice landed in Phase 7; full restore UX delivered in Phase 8

### Active

See `.planning/REQUIREMENTS.md` for the v2.2 requirement list. Summary by category:
- Demo & showcase (`slate demo` rendering)
- CLI tool colors (LS_COLORS, EZA_COLORS, BSD-`ls` handling)
- UX: new-terminal reminders with platform-appropriate phrasing
- Editor research (vim/nvim plugin-absorption spike)
- Theme family (Solarized + family-aware listing)

### Validated in v2.2 (Editor Ecosystem + Polish)

- [x] `slate demo` single-screen showcase — 4 blocks (code, dir tree, git-log, progress) covering all 16 ANSI slots, live-palette sourced with zero hex literals, well under 1s render — Validated in Phase 15 (DEMO-01)
- [x] Post-action demo hint after `slate setup` and explicit `slate theme <id>` — once-only AtomicBool, suppressed on `--auto`/`--quiet`/picker/`slate set`, non-stacking with deprecation tips — Validated in Phase 15 (DEMO-02)
- [x] Shared `src/design/file_type_colors` module — `classify()` + flat `extension_map()` feeding Phase 16's `LS_COLORS`/`EZA_COLORS` generation — Validated in Phase 15

### Validated in v2.1 (Cross-Platform Core)

- [x] v2.1 architecture reset: platform-specific behavior moved behind shared capability interfaces — Validated in v2.1 (Phase 10)
- [x] Linux formal baseline: Debian/Ubuntu + GNOME, `apt` package installs, Linux font paths, `fc-cache`, portal-aware GNOME appearance/share backends — Validated in v2.1 (Phase 12)
- [x] Shell expansion: shared `env.zsh`, `env.bash`, and `env.fish` rendered from one managed model — Validated in v2.1 (Phase 11)
- [x] Terminal capability matrix: reload / live-preview / localized font-apply gated by validated terminal backend with truthful receipts — Validated in v2.1 (Phase 13)
- [x] Release and CI expansion: four target artifacts, macOS + Ubuntu PR coverage, shared installer with smoke tests — Validated in v2.1 (Phase 14)
- [x] v2.0 Phase 9 launch/distribution carry-forward reconciled under the v2.1 support matrix — Validated in v2.1 (Phase 14)

### Post-Phase-8 Codex Refactor (2026-04-14)

- [x] Theme system consolidated: 18 per-theme .rs struct literals + xtask code generator + themes/*.json → single `themes/themes.toml` loaded via `include_str!` + `OnceLock`; strict validation (hex color format, required tool_refs, auto_pair cross-reference)
- [x] Config module split: `src/config/mod.rs` (24KB monolith) → 5 submodules (`backup/`, `auto_theme.rs`, `shell_integration.rs`, `state_files.rs`, `flags.rs`); public API unchanged
- [x] Picker decomposed: `event_loop.rs` rendering + actions extracted to `render.rs` + `actions.rs`
- [x] Wizard helpers extracted to `wizard_support.rs`; tool catalog converted to compile-time const array
- [x] CLI handler signatures simplified: `set::handle` and `restore::handle` take typed params instead of `&[&str]`
- [x] `marker_block` gains file-level convenience API (`upsert_managed_block_file`, `remove_managed_blocks_from_file`); delta/eza/tmux/clean deduplicated
- [x] Hub "More options" gains fastfetch toggle with shell integration refresh + rollback on failure
- [x] Dead code removed: `launchd.rs`, `wcag.rs.bak`, `plist` dependency

### Validated in Phase 8 (Safety Net)

- [x] Real `slate restore` UX with interactive picker, direct ID mode, pre-restore snapshot, continue-on-error aggregation; `reset` hidden as compatibility alias
- [x] Restore point management: `--list` and `--delete` fully implemented
- [x] Strict WCAG 4.5:1 contrast gate — 51 failures repaired across 18 themes, build-blocking audit test
- [x] Font refresh decoupling — font changes no longer cascade theme reapply; font-only adapter paths for Ghostty + Alacritty
- [x] Graceful version detection for Ghostty 1.1.0+ and Alacritty 0.12.0+
- [x] Swift watcher precompiled at build time (no runtime swiftc dependency)
- [x] Picker live-preview permission state persisted in config
- [x] Hub redesigned as single-entry guided flow (no looping menu)
- [x] Command/help surface reviewed for release readiness

### Validated in Phase 7 (Polish + Gap Fixes)

- [x] Compatibility-first noun-driven CLI: `slate theme`, `slate font`, `slate config`; `slate set` preserved as alias
- [x] Hub flattening, fastfetch toggle, and picker readability upgrades
- [x] First-run trust slice: baseline snapshot before setup plus `slate clean`
- [x] Auto-theme configuration flow wired to the Ghostty-scoped runtime watcher path; enable/disable, status, and clean now all converge on the same watcher lifecycle (real-environment sign-off remains a Phase 8 gate)

### Validated in Phase 5 (Quality Fixes + Baseline Usability)

- [x] ZSH highlight hex color format (#RRGGBB), complete palette fields across all 10 themes, registry loop pattern with partial failure handling — Validated in Phase 5
- [x] Font detection (NerdFont/Nerd Font variants), font-family written to Ghostty and Alacritty configs — Validated in Phase 5
- [x] Graceful Ctrl+C cancellation ("✦ Setup cancelled.", exit 130) — Validated in Phase 5

### Validated in Phase 4 (Shell Integration + Extras)

- [x] Shell integration via source static file: .zshrc marker block sources env.zsh with all exports, fastfetch wrapper, and zsh-highlight source — Validated in Phase 4
- [x] Fastfetch premium JSONC generation: 8 modules, Apple logo preserved, ANSI 24-bit RGB colors across all 10 themes — Validated in Phase 4
- [x] Tmux 7-element theming: status, window-current, pane-border, pane-active, message, mode, message-command — Validated in Phase 4
- [x] `slate list` TrueColor palette preview blocks (4 colors per theme) — Validated in Phase 4
- [x] CLI routing foundation established in Phase 4 (setup/set/status/list/reset at the time); later phases intentionally evolved the public surface toward `theme/font/config` while deferring real reset UX to Phase 8

### Validated in Phase 3 (Tool Adapters)

- [x] 11 tool adapters with three-tier config — bat, Ghostty, eza, Alacritty, delta, tmux, Starship, lazygit, fastfetch, zsh-syntax-highlighting, Nerd Font
- [x] MarkerBlock utility for safe config editing with validation — strip, upsert, validate (0/0 or 1/1 pairs)
- [x] PaletteRenderer with 5 output formats — TOML, YAML, shell vars, tmux, JSONC
- [x] Shell integration scaffolding: env exports (BAT_THEME, EZA_CONFIG_DIR, LG_CONFIG_FILE) and fastfetch wrapper (Phase 4 supersedes with source static file pattern, removes init command)

### Validated in Phase 2 (Setup Wizard)

- [x] `slate setup` wizard installs + configures a complete beautiful terminal from scratch — Validated in Phase 2: cliclack wizard with quick/manual modes, 4 presets, 10 themes, 4 fonts
- [x] Beautiful CLI output with cliclack, brand language, ASCII logo — Validated in Phase 2: typography system, branded intro/outro, step counters, review receipts
- [x] Idempotent setup (safe to re-run, can add/remove components) — Validated in Phase 2: state-aware rerun, force flag, retry with --only

### Validated in Phase 1 (Foundation)

- [x] Three-tier config architecture (managed → integration → user override) — ConfigManager with write_managed_file(), edit_config_field(), atomic writes
- [x] Brand language centralized — 80+ constants in Language struct
- [x] Design system (symbols + colors) — ✦✓✗○ + GRAY/ACCENT/RESET
- [x] Adapter trait v2 with ApplyStrategy — 8 methods, 4 strategies
- [x] 8 theme variants embedded — Catppuccin×4, Tokyo Night×2, Dracula, Nord
- [x] Error framework (thiserror + color-eyre) — 18 error variants

### Out of Scope

- Windows support — not part of v2.1
- IDE/editor theme switching — separate domain
- Real-time config watching — tools auto-reload or need restart
- Full cross-distro package-manager parity — v2.1 formally supports Homebrew on macOS and `apt` on Linux; `pacman` / `dnf` are extension points only
- Full persistent TUI product — interactive flows can exist (wizard/hub/picker), but slate is not becoming an always-open dashboard app
- Network-dependent features — all data embedded in binary
- KDE and other Linux desktops as first-class support targets in v2.1 — Debian/Ubuntu + GNOME ships first; others stay best-effort
- Shell plugin parity across `zsh`, `bash`, and `fish` — common core is required, shell-specific extras are not
- iTerm2 adapter in v2.0 — complex plist manipulation, deferred
- Warp adapter — proprietary, cloud-dependent, unreliable to automate
- Custom palette builder — undermines the curated value proposition
- Long-tail niche CLI tool support — low ROI; focus on the high-impact stack

## Context

- **Milestone shift (2026-04-16):** v2.1 reframes slate from a macOS-only product into a cross-platform terminal beautification kit with a shared core plus platform backends. Existing v2.0 launch work is now a carry-forward concern that must be re-scoped under the new support matrix.
- **Product pivot (2026-04-09):** Pivoted from "theme switcher" to "terminal beautification kit." Milestone 1 (themectl) built the foundation; Milestone 2 (slate) builds the product.
- **Market gap confirmed:** No existing tool does one-click terminal beautification (fonts + colors + prompt + tools). tinty does colors only, gogh does terminal colors only, NvChad only does Neovim.
- **2026 consensus stack:** Ghostty + Starship + Catppuccin + bat/delta/eza/lazygit + Nerd Font. We're building the installer for this exact stack.
- **Existing code:** themectl Rust codebase has adapter pattern, backup system, and theme data — useful as reference. New crate `slate-cli` rewrites the product surface with a higher quality bar; old backup/restore work is reference input, not proof that Phase 8 Safety Net is already shipped.
- **Reality-check audit (2026-04-13):** Public surface is now real and usable, but Phase 8 must close setup/watcher edge cases and keep docs/support claims aligned to verified reality before anything is called fully shippable. Kitty remains unsupported today; `restore` remains a Phase 8 target, not a shipped surface.
- **Phase 8 hardening direction (2026-04-13):** Phase 8 is now the \"make the product truly hold together\" phase, not cosmetic polish. It must land strict WCAG repair, localized font refresh, real restore UX, watcher/live-preview hardening, command-fit review, real-machine demo readiness, color fidelity across Starship/fastfetch/core tools, and a clear mainstream dotfile boundary. Phase 9 should assume those product questions are already resolved.
- **Tech stack decided:** Rust + cliclack (wizard) + indicatif (progress) + toml_edit/serde_yaml (config). Single binary, brew tap via cargo-dist.
- **Config philosophy:** Most tools can be composed through include or env-based managed configs (Ghostty, lazygit, eza, delta, Alacritty, tmux, bat, fastfetch via shell wrapper). Starship is the notable monolithic exception and requires a narrowly scoped TOML edit instead of broad overwrites.

## Constraints

- **Tech stack**: Rust — single binary, fast theme apply path, platform backends isolated behind shared traits
- **Platform**: First-class support for macOS (`x86_64` + `aarch64`) and Linux (`x86_64` + `aarch64`) with Debian/Ubuntu + GNOME as the formal Linux baseline
- **Distribution**: GitHub Releases + shared `install.sh`; Homebrew/tap stays first-class on macOS, cargo install remains secondary. No npm, no runtime dependencies
- **Package management**: Homebrew first-class on macOS, `apt` first-class on Linux, Starship may keep upstream user-local installer path
- **Shell support**: `zsh`, `bash`, and `fish` share a common managed environment core, with shell-specific integration files where required
- **UI framework**: cliclack for setup wizard, not full TUI (ratatui). Premium feel without complexity
- **Config safety**: Atomic writes, backup before every mutation, marker comments for .zshrc
- **Brand quality**: Every user-facing string must feel premium. No generic "Installing..." messages

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rename to slate | "themectl" too narrow for beautification kit; "slate" = clean surface, 5 chars, available | -- Pending |
| Rust over Go/Node | Single binary, fast `set`, same ecosystem as target tools (bat, delta, starship) | -- Pending |
| cliclack over ratatui | Setup wizard, not full TUI app. cliclack = charm.sh level beauty, lower complexity | -- Pending |
| New crate, not refactor | Quality bar fundamentally different; three-tier config, brand language, beautiful errors | -- Pending |
| Three-tier config | Managed + integration + user override. Never overwrite user customizations | -- Pending |
| brew tap distribution | No star requirement; cargo-dist auto-generates formula; user just `brew install` | -- Pending |
| macOS only | Eliminates cross-platform complexity; brew/zsh/Nerd Font paths all known | Superseded by v2.1 |
| zsh primary, no fish v2.0 | macOS default is zsh (95%+ of target users); fish deferred | Superseded by v2.1 |
| Shared core + platform backends | Keep one Slate product across macOS and Linux instead of maintaining a Linux fork | ✓ Good (v2.1) |
| Debian/Ubuntu + GNOME as Linux baseline | Constrains scope to one desktop stack while still delivering real Linux support | ✓ Good (v2.1) |
| `apt` first-class, `pacman` / `dnf` deferred | Avoid pretending package management is uniform across distros while preserving future extension points | ✓ Good (v2.1) |
| `zsh` / `bash` / `fish` shared shell core | Expand shell reach without forcing plugin-level parity everywhere | ✓ Good (v2.1) |
| Portal-aware Linux appearance/share backends | Keep GNOME as validated baseline while degrading gracefully on other desktops via XDG Desktop Portal | ✓ Good (v2.1) |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-18 after Phase 15 (`slate demo`) shipped — curated single-screen showcase + post-action hint live; file_type_colors module ready for Phase 16 consumption*
