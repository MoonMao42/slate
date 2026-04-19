# Requirements: slate v2.2

**Defined:** 2026-04-18 · **Expanded:** 2026-04-19 (4 → 8 phases)
**Core Value:** 30-second Time-to-Dopamine: from `brew install` to a stunning terminal
**Milestone Goal:** Extend "one palette across the stack" to CLI file tools + editor ecosystem, then polish the product surfaces (brand text roles, interactive demo, sound cues) and ship v2.2 as one cohesive release — Solarized as the crowning reveal.

## v2.2 Requirements

Each requirement maps to exactly one roadmap phase. Phases continue numbering from v2.1 (Phases 15–22).

### Demo & Showcase

- [ ] **DEMO-01**: User can run `slate demo` and see a curated, single-screen showcase of the active palette — covering at minimum a code snippet with syntax highlighting, a directory tree (file-type colors), git-log excerpt, and a progress bar — so the "wow" moment is discoverable without hunting for it
- [ ] **DEMO-02**: User sees a demo-style hint (not the full demo, but a pointer like "run `slate demo` to see it in action") after `slate theme set <id>` and `slate setup` so the showcase is surfaced at the right moments without being intrusive

### CLI Tool Colors

- [ ] **LS-01**: User can have a slate-managed `LS_COLORS` environment variable rendered from the active palette, covering the common file-type and extension groups (directory, exec, symlink, archive, media, code, docs, etc.), written to the managed shell integration so every new shell picks it up
- [ ] **LS-02**: User can have a slate-managed `EZA_COLORS` environment variable rendered from the active palette, using eza's own palette keys where they diverge from GNU ls, written alongside `LS_COLORS`
- [ ] **LS-03**: On macOS where BSD `ls` is the default, user sees a one-time capability message recommending `brew install coreutils` (for GNU `gls`) instead of slate silently writing an 8-color `LSCOLORS` fallback

### UX: New-Terminal Reminders

- [ ] **UX-01**: Adapters can declare a `RequiresNewShell` signal on their return value, indicating whether the change they just applied needs a fresh shell session to take effect (env/PATH/integration changes do; file-only writes to already-reloadable tools do not)
- [ ] **UX-02**: At the end of `slate setup`, `slate theme …`, `slate font …`, and `slate config …` runs, slate emits at most one "new-terminal" reminder per run, deduplicated across adapters, only when at least one `RequiresNewShell` signal fired
- [ ] **UX-03**: The reminder text is platform-aware and uses active-experience language (no "please", no "you need to"): on macOS it points to `⌘N`; on Linux it says "open a new terminal"; the phrasing frames the new shell as the reveal of the change, not as a limitation

### Editor Adapter

- [x] **EDITOR-01**: `slate theme set <id>` propagates to Neovim — each built-in theme renders as a slate-generated Lua colorscheme at `~/.config/nvim/colors/slate-<variant>.lua` (one per built-in theme) backed by a slate-managed loader at `~/.config/nvim/lua/slate/init.lua`; the loader watches `~/.cache/slate/current_theme.lua` and hot-reloads every running nvim instance when the state file changes. The user's `init.lua` is touched only via a single-line `pcall(require, 'slate')` activation, written ONLY on explicit consent through a 3-way A/B/C prompt (add-for-me / show-me-the-line / skip). Every generated colorscheme passes `nvim --headless -c 'luafile %' -c 'q'` syntax validation, and the adapter reports `ApplyOutcome::Applied { requires_new_shell: false }`. Classic vim (not nvim) is out of scope; missing or too-old nvim emits a one-line capability hint and skips silently.

### Brand & CLI Text Roles

Placeholder — refined during `/gsd-discuss-phase 18` after the sketch phase picks a direction.

- [ ] **BRAND-01**: slate's CLI output emits via a unified text-role system — command keys, file paths, keyboard shortcuts, status severity, quoted code, and brand accents each have a single canonical ANSI treatment codified in `src/brand/` rather than ad-hoc inline escapes. Every user-facing surface (setup wizard, `slate theme`, `slate status`, `slate clean`, completion receipts, errors, new-shell reminders) routes through the role API, and ANSI byte sequences per role are regression-locked so brand drift is caught at CI.
- [ ] **BRAND-02**: The role system was chosen via a `/gsd-sketch` artifact — 3–4 candidate treatments captured side-by-side, user picked one, and the picked variant is the shipped style. Future brand changes go through the same sketch-first loop.

### Interactive Demo

- [ ] **DEMO-03**: `slate demo` evolves from one-shot showcase into an interactive theme picker — navigating variants (grouped by family) live-previews the full stack in-place (ghostty bg, starship prompt, bat snippet, delta diff, eza listing, lazygit mini-UI, nvim syntax) without mutating persistent config. `Enter` applies the highlighted variant; `Esc` / `q` exits without applying. Preview state is ephemeral.

### Sound + Promo

Placeholder — refined during `/gsd-discuss-phase 20`.

- [ ] **AUDIO-01**: slate emits subtle SFX at key interaction moments (theme set success, picker move, error, setup completion) — opt-in via `slate config set sound enable`, default off, respects `--quiet`, at most one SFX per user action. SFX library is licensed / original, cross-platform playback works on macOS + Linux with no extra daemons. VHS-scripted promo recordings (`demo` picker, `theme set`, setup wizard) ship as `.tape` + rendered outputs for README / launch embedding.

### Theme Family (Phase 21 — scheduled last)

- [ ] **FAM-01**: User can apply Solarized Dark and Solarized Light through `slate theme set solarized-dark` / `solarized-light`, with full coverage across existing tool backends (Ghostty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting) matching the quality bar of existing themes
- [ ] **FAM-02**: User can see themes grouped by `family` when listing (`slate theme --list` or equivalent) and when browsing the picker from Phase 19, so the 20 variants across 10 families are navigable rather than a flat list

### Docs + Release

Placeholder — refined during `/gsd-discuss-phase 22`.

- [ ] **DOCS-01**: README is rewritten to reflect the matured v2.2 product (interactive demo, nvim adapter, sound cues, Solarized, family grouping) with a hero recording, 30-second onboarding, command reference, theme gallery covering all 20 variants, and an honest platform matrix.
- [ ] **DOCS-02**: CHANGELOG has a complete v2.2 entry; release-notes draft captures demo redesign + nvim adapter + sound + Solarized as narrative beats; brew tap formula + cargo-dist automation validated for the v2.2 release cut.

## Future Requirements (Deferred)

### v2.3 candidates (depend on v2.2 research)

- **EDITOR-02**: VSCode adapter — deferred due to settings.json merge + profile/workspace layering fragility
- **EDITOR-03**: Helix / Zed / Emacs adapters — candidates for future per-editor phases once the vim/nvim adapter ships and the role-to-highlight mapping stabilizes

### Later

- **EXPORT-01**: `slate export` — palette exported as JSON / env vars / CSS variables for long-tail tools (fzf, yazi, zellij) — deliberately deferred from v2.2 per user decision

## Out of Scope

| Feature | Reason |
|---------|--------|
| VSCode adapter | JSON merge + profile/workspace layering is too fragile for a drop-in approach; revisit after editor research |
| `slate export` (palette export to JSON/env/CSS) | User chose to defer; keep v2.2 scope tight on editor-adjacent + UX polish |
| BSD `ls` 8-color fallback | Cannot render Catppuccin/Solarized-grade palettes faithfully; recommending `coreutils` is more honest |
| Plugin-manager detection (lazy.nvim / packer / vim-plug) | Rejected by the same principle that rejects VSCode: fragile state detection; conflicts with slate's three-tier config philosophy |
| Shipping additional theme families beyond Solarized in v2.2 | 10 families (Catppuccin, Dracula, Everforest, Gruvbox, Kanagawa, Nord, Rose Pine, Solarized, Tokyo Night) is enough surface area; new families land per-milestone |

## Traceability

| Requirement | Phase | Status | Verified by |
|-------------|-------|--------|-------------|
| DEMO-01 | Phase 15 | Complete | — |
| DEMO-02 | Phase 15 | Complete | — |
| LS-01 | Phase 16 | Complete | — |
| LS-02 | Phase 16 | Complete | — |
| LS-03 | Phase 16 | Complete | — |
| UX-01 | Phase 16 | Complete | — |
| UX-02 | Phase 16 | Complete | — |
| UX-03 | Phase 16 | Complete | — |
| EDITOR-01 | Phase 17 | Complete | `src/adapter/nvim.rs` (render_colorscheme, render_loader, render_shim, NvimAdapter, version_check); `src/cli/setup.rs` 3-way consent prompt + `src/cli/clean.rs::remove_nvim_managed_references` + `src/cli/config.rs::handle_config_set_with_env` (`editor disable`); `tests/nvim_integration.rs` 7 nvim-headless gates (state-file atomicity, fs_event hot-reload, 18-variant `luafile` syntax, Pitfall 4 marker-comment regression, capability hint); `src/cli/clean.rs::tests` + `src/cli/config.rs::tests` source-side coverage of clean/disable contracts |
| BRAND-01 | Phase 18 | Pending | — |
| BRAND-02 | Phase 18 | Pending | — |
| DEMO-03 | Phase 19 | Pending | — |
| AUDIO-01 | Phase 20 | Pending | — |
| FAM-01 | Phase 21 | Pending | — |
| FAM-02 | Phase 21 | Pending | — |
| DOCS-01 | Phase 22 | Pending | — |
| DOCS-02 | Phase 22 | Pending | — |

**Coverage:**
- v2.2 requirements: 17 total
- Mapped to phases: 17
- Unmapped: 0 ✓
- Note: Phase 15's DEMO-01/02 flipped to Complete retroactively (Phase 15 shipped 2026-04-18 but the table was never back-filled).

---

*Requirements defined: 2026-04-18*
*Source: post-v2.1 Gemini strategic review + user decisions during /gsd-new-milestone flow*
