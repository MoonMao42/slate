# Requirements: slate v2.2

**Defined:** 2026-04-18 · **Expanded:** 2026-04-19 (4 → 8 phases)
**Core Value:** 30-second Time-to-Dopamine: from `brew install` to a stunning terminal
**Milestone Goal:** Extend "one palette across the stack" to CLI file tools + editor ecosystem, then polish the product surfaces (brand text roles, interactive demo, sound cues) and ship v2.2 as one cohesive release — Solarized as the crowning reveal.

## v2.2 Requirements

Each requirement maps to exactly one roadmap phase. Phases continue numbering from v2.1 (Phases 15–22).

### Demo & Showcase

- [x] **DEMO-01** *(Superseded by Phase 19 / DEMO-03 on 2026-04-20)*: originally — "User can run `slate demo` and see a curated, single-screen showcase of the active palette — covering at minimum a code snippet with syntax highlighting, a directory tree (file-type colors), git-log excerpt, and a progress bar — so the 'wow' moment is discoverable without hunting for it". Shipped as Phase 15 (2026-04-18) and retired as a standalone CLI surface on 2026-04-20 — the 4-block showcase lives on as an internal picker preview component per Phase 19 D-07.
- [x] **DEMO-02** *(Superseded by Phase 19 / DEMO-03 on 2026-04-20)*: originally — "User sees a demo-style hint (not the full demo, but a pointer like 'run `slate demo` to see it in action') after `slate theme set <id>` and `slate setup` so the showcase is surfaced at the right moments without being intrusive". Shipped as Phase 15 (2026-04-18) and retired on 2026-04-20 — Gemini review (Phase 19 CONTEXT §specifics) flagged "previewing is a purchasing behavior, not a possession behavior"; `slate status` now covers the "what am I using" slot, and the new picker from Phase 19 carries the live-preview burden.

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

- [ ] **BRAND-01**: slate's CLI output emits via a unified text-role system codified under `src/brand/`: `Roles<'a>` exposes 13 role methods (`command / path / shortcut / code / theme_name / brand / logo / status_success / status_warn / status_error / heading / tree_branch / tree_end`), consuming a `RenderContext` carrying the active theme + `RenderMode` (`Truecolor / Basic / None`). Brand anchors (slate logo, ◆ headings, ★ completion, error frame icon) render fixed `#7287fd`; everyday role chrome (command / theme_name / inline brand verb) pulls from the per-theme `brand_accent` palette slot; severity never uses lavender. `command` and `code` pills use a per-appearance alpha blend (14% dark / 24% light) against the active theme's background, with `› text ‹` + Dim+Bold fallback under `RenderMode::Basic` and plain text under `RenderMode::None`. `SlateTheme : impl cliclack::Theme` overrides 9 methods (`bar_color`, `state_symbol_color`, `state_symbol`, `info_symbol`, `warning_symbol`, `error_symbol`, `format_intro`, `format_outro`, `format_outro_cancel`) and is injected once at startup via `cliclack::set_theme`. Every user-facing surface (setup wizard, `slate theme`, `slate font`, `slate status`, `slate clean`, `slate restore`, `slate demo`, picker chrome, error + reminder paths, `slate config`, `slate share`) routes through the role API. Per-role ANSI byte sequences are snapshot-locked via `insta` under `src/brand/snapshots/` (6 roles × 3 render modes = ~18 fixtures); per-wave grep-invariant tests + a phase-level aggregate assert zero raw styling ANSI outside allowlisted swatch + terminal-control sites. A `BrandEvent` enum + `EventSink` trait + `NoopSink` default ships as the Phase 20 sound seam with dispatch sites planted at theme-apply success, picker navigation + selection, setup completion, config mutation, and error surfaces.
- [ ] **BRAND-02**: The role system was chosen via a `/gsd-sketch` artifact shipped at commit `e412143` — three sketches (role-differentiation, accent-placement, header-receipt) captured ~3 candidates each as side-by-side HTML mocks; user picked `pill-led` role differentiation + `medium` lavender density + `tree` header/receipt narrative. The picked variants are codified in `.planning/sketches/MANIFEST.md` (the canonical visual contract) and the Phase 18 Role API renders them faithfully. Future brand changes go through the same sketch-first loop before any implementation.

### Interactive Demo

- [ ] **DEMO-03**: `slate theme` (no args) and `slate set` open a single interactive picker listing all 20 variants grouped by family (◆ FamilyName section headers inserted at render time — not data rows; ↑↓/jk skip them). Navigating variants live-previews the full stack in-place — at minimum Ghostty background (via SIGUSR2 hot-reload), Starship prompt, bat snippet, delta diff, eza listing, lazygit mini-UI, nvim syntax — without mutating `~/.config/slate/current` or `~/.config/slate/current-opacity` (both are ephemeral during navigation per D-10). `Enter` commits via `silent_commit_apply` (same semantics as `slate theme <id>`); `Esc` / `q` / Ctrl+C exit with full rollback of `managed/*` to the original theme + opacity. Rollback is triple-guarded: active `silent_preview_apply(original)` on Esc + `RollbackGuard: Drop` on normal stack unwind + `std::panic::set_hook` to cover `panic = "abort"` in release. Tab toggles between list-dominant mode (bottom 3-line mini-preview) and full-screen ◆ Heading mode (responsive fold: 4 blocks at 80×24, 6 at ≥32 rows, 8 at ≥40 rows). Tab is side-effect-free — no `BrandEvent` dispatch. Hybrid starship prompt in full-screen mode forks the user's `starship prompt` with a per-subprocess `STARSHIP_CONFIG` env override pointed at `managed/starship/active.toml`; missing starship falls back to self-drawn `SAMPLE_TOKENS` per D-04. The Phase 15 `slate demo` command and DEMO-02 hint are retired — the 4-block renderer lives on as an internal picker preview component under `src/cli/picker/preview/blocks.rs` per D-07.

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
| DEMO-01 | Phase 15 → Phase 19 | Superseded by DEMO-03 (2026-04-20) | Shipped Phase 15 2026-04-18; CLI surface retired Phase 19. Renderer lives on at `src/cli/picker/preview/blocks.rs`. |
| DEMO-02 | Phase 15 → Phase 19 | Superseded by DEMO-03 (2026-04-20) | Shipped Phase 15 2026-04-18; hint infrastructure retired Phase 19 (D-06 — `emit_demo_hint_once` + `DEMO_HINT` + `demo_size_error` all deleted). |
| LS-01 | Phase 16 | Complete | — |
| LS-02 | Phase 16 | Complete | — |
| LS-03 | Phase 16 | Complete | — |
| UX-01 | Phase 16 | Complete | — |
| UX-02 | Phase 16 | Complete | — |
| UX-03 | Phase 16 | Complete | — |
| EDITOR-01 | Phase 17 | Complete | `src/adapter/nvim.rs` (render_colorscheme, render_loader, render_shim, NvimAdapter, version_check); `src/cli/setup.rs` 3-way consent prompt + `src/cli/clean.rs::remove_nvim_managed_references` + `src/cli/config.rs::handle_config_set_with_env` (`editor disable`); `tests/nvim_integration.rs` 7 nvim-headless gates (state-file atomicity, fs_event hot-reload, 18-variant `luafile` syntax, Pitfall 4 marker-comment regression, capability hint); `src/cli/clean.rs::tests` + `src/cli/config.rs::tests` source-side coverage of clean/disable contracts |
| BRAND-01 | Phase 18 | In progress (Waves 0-6 shipped) | Wave 0 (2026-04-19, plan 18-01): `src/brand/{palette,render_context,roles,cliclack_theme,events,migration,symbols}.rs` + `brand_accent` schema on all 18 themes + MockTheme snapshot harness + main.rs init wiring + zero src/cli/* caller diff per D-12. Wave 1 (2026-04-19, plan 18-02): setup.rs / setup_executor / wizard_core / hub / wizard_support / tool_selection routed through Roles + 5 BrandEvent dispatch sites. Wave 2 (2026-04-19, plan 18-03): theme.rs / font.rs / set.rs routed through Roles + 16 dispatch sites (21 total). Wave 3 (2026-04-19, plan 18-04): status_panel.rs + SWATCH-RENDERER allowlist. Wave 4 (2026-04-20, plan 18-05): clean.rs + restore.rs + tree-narrative `println!` receipts + 3 dispatch sites (24 total). Wave 5 (2026-04-20, plan 18-06): demo.rs + preview_panel.rs + render.rs chrome through Roles + TERMINAL-CONTROL single-line allowlist (complement to SWATCH-RENDERER function scope) + 2 picker dispatch sites (PickerMove/PickerEnter; 26 total). Wave 6 (2026-04-20, plan 18-07): auto_theme.rs + aura.rs + list.rs + new_shell_reminder.rs + share.rs + config.rs + adapter/ls_colors.rs docstrings all through Roles + 12 dispatch sites (2 AutoThemeFailed outer-wrapper with UserCancelled exclusion + 10 ConfigSet across 8 config sub-commands + share import + auto-theme configure save; 38 total); Wave 6 grep gate active; final `no_deprecated_allow_in_user_surfaces_after_phase_18` sweep test green; EVERY `src/cli/*` surface now emits through Roles. Plan 18-08 adds the phase-level aggregate invariant test + BRAND-01/02 acceptance text refresh + `src/design/*` deletion. |
| BRAND-02 | Phase 18 | In progress (Wave 0 sketch canon anchored) | Wave 0 (2026-04-19, plan 18-01): `src/brand/roles.rs` module docstring + `SKETCH_CANON_DOCTEST_ANCHOR` runnable doctest assert the 3 sketch winners (pill-led role differentiation / medium lavender density / tree-style narrative) from `.planning/sketches/MANIFEST.md`. Finalized at Plan 18-08 when BRAND-01/02 acceptance text refresh lands. |
| DEMO-03 | Phase 19 | In progress | — |
| AUDIO-01 | Phase 20 | Pending | — |
| FAM-01 | Phase 21 | Pending | — |
| FAM-02 | Phase 21 | Pending | — |
| DOCS-01 | Phase 22 | Pending | — |
| DOCS-02 | Phase 22 | Pending | — |

**Coverage:**
- v2.2 requirements: 17 total
- Mapped to phases: 17
- Unmapped: 0 ✓
- Note: DEMO-01 / DEMO-02 superseded by DEMO-03 on 2026-04-20 (Phase 19 D-05 / D-06). Phase 15 shipped the original; Phase 19 retires the CLI surface while preserving the 4-block renderer as a picker preview component.

---

*Requirements defined: 2026-04-18*
*Source: post-v2.1 Gemini strategic review + user decisions during /gsd-new-milestone flow*
*DEMO-01 / DEMO-02 superseded 2026-04-20 (Phase 19 Wave 0, plan 19-01).*
