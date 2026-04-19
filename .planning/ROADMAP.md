---
roadmap_version: 3.1
milestone: v2.2
milestone_name: editor-ecosystem-polish
phase_count: 8
granularity: balanced
created: "2026-04-16"
last_updated: "2026-04-19"
---

# Roadmap: slate

## Milestones

- ✅ **v1.0 themectl** — archived (superseded by v2.0 pivot, see `.planning/archive/v1.0-themectl/`)
- ✅ **v2.0 pre-v2.1 snapshot** — pre-pivot v2.0 milestone docs snapshotted for reference (see `.planning/milestones/v2.0-pre-v2.1/`)
- ✅ **v2.1 Cross-Platform Core** — Phases 10–14 (shipped 2026-04-17)
- 📋 **v2.2 Editor Ecosystem + Polish** — Phases 15–22 (3/8 complete; expanded 2026-04-19 to pull pre-release polish into v2.2 so the milestone ships as one cohesive release)

## Phases

<details>
<summary>✅ v2.1 Cross-Platform Core (Phases 10–14) — SHIPPED 2026-04-17</summary>

- [x] Phase 10: Capability Foundation (3/3 plans) — shared capability skeleton + truthful status model
- [x] Phase 11: Shared Shell Core (3/3 plans) — one managed shell env model rendered to `zsh` / `bash` / `fish`
- [x] Phase 12: Linux Platform Backends (3/3 plans) — portal-aware appearance/share, fonts, `apt` behind capability gates
- [x] Phase 13: Terminal Integration + Cross-Platform UX (3/3 plans) — terminal-specific reload/preview + restore/clean
- [x] Phase 14: Release Matrix + Installer + Docs (3/3 plans) — four-target assets, CI, installer smoke, truthful support matrix

Full archive: [`milestones/v2.1-ROADMAP.md`](./milestones/v2.1-ROADMAP.md) · [`milestones/v2.1-REQUIREMENTS.md`](./milestones/v2.1-REQUIREMENTS.md) · phase directories in [`milestones/v2.1-phases/`](./milestones/v2.1-phases/)

</details>

### 📋 v2.2 Editor Ecosystem + Polish (Phases 15–22)

- [x] **Phase 15: Palette Showcase — `slate demo`** — curated single-screen payoff render + contextual hint surfacing (completed 2026-04-18)
- [x] **Phase 16: CLI Tool Colors + New-Terminal UX** — `LS_COLORS` / `EZA_COLORS` from the active palette + cross-platform `RequiresNewShell` reminder plumbing (completed 2026-04-18, shipped as v0.1.2)
- [x] **Phase 17: Editor Adapter — Neovim Colorschemes** — 18 slate-generated Lua colorschemes + slate-managed loader + file-watcher hot-reload + 3-way consent prompt for the single `pcall(require, 'slate')` activation line; classic vim explicitly out of scope (completed 2026-04-19)
- [ ] **Phase 18: Brand Sketch + CLI Text-Role System** — sketch-driven design pass that introduces a unified text-role system for slate's CLI output (command keys, file paths, shortcuts, status, severity) and rolls the winning style across `setup` / `theme` / `status` / `clean` / `demo` receipt surfaces
- [ ] **Phase 19: `slate demo` Redesign — Picker + Live Preview** — evolve `slate demo` from one-shot showcase into an interactive theme picker where navigating variants live-previews the whole stack (ghostty + starship + bat + delta + eza + lazygit + nvim) in-place
- [ ] **Phase 20: Sound Design + Promo Assets** — curated SFX library, cross-platform audio playback, triggers at key moments (set success, picker move, error); produce VHS-scripted promo recordings for README and launch
- [ ] **Phase 21: Theme Family Expansion — Solarized** — originally Phase 18; scheduled last so Solarized's reveal is amplified by the new brand + demo + sound work. Solarized Dark + Light full-backend coverage, `family` grouping surfaced in listing + picker
- [ ] **Phase 22: README Rewrite + Release Polish** — rewrite README to reflect the matured v2.2 product (new demo, nvim adapter, sound cues, family grouping); embed recordings / screenshots; polish CHANGELOG and release-note scaffolding for the v2.2 tag

---

## Phase Details

### Phase 15: Palette Showcase — `slate demo`
**Goal**: Users can discover the "wow" moment of the active palette on demand, without hunting — one command renders a curated, single-screen showcase, and the showcase is surfaced at the right moments (after setup, after a theme switch).
**Depends on**: Existing palette infrastructure (`themes/themes.toml`, `PaletteRenderer`) — no v2.2 dependency.
**Requirements**: DEMO-01, DEMO-02
**Success Criteria** (what must be TRUE):
  1. User can run `slate demo` and see a single-screen showcase rendering at minimum a syntax-highlighted code snippet, a directory tree with file-type colors, a git-log excerpt, and a progress bar — all in the active palette.
  2. The demo completes in well under a second on a normal terminal (no network, no external tool invocation) and fits standard 80×24 without clipping.
  3. After `slate theme set <id>` and after `slate setup`, the user sees a single-line hint pointing to `slate demo` — not the full showcase, just a surfaced pointer.
  4. The hint is skippable / non-intrusive and only appears once per successful run (not repeated on `--quiet`, not stacked with other hints).
**Plans:** 6/6 plans complete
Plans:
- [x] 15-00-PLAN.md — Wave 0 scaffolding: extend `SemanticColor` with 14 stubs, create `file_type_colors` + `demo` module skeletons, lay down 10 `#[ignore]`d integration-test stubs + bench scaffold
- [x] 15-01-PLAN.md — Wave 1: replace placeholder arms in `Palette::resolve` with real palette-slot assignments + rstest coverage across 3 themes
- [x] 15-02-PLAN.md — Wave 1: implement `file_type_colors::classify` + `extension_map` (Phase 16 shared module) with 7 precedence rules + rstest coverage
- [x] 15-03-PLAN.md — Wave 2: build `demo.rs` renderer (4 blocks, 80-col fit, palette-resolved colors), size gate, hint emitter, `Language::DEMO_HINT` + `demo_size_error`
- [x] 15-04-PLAN.md — Wave 3: wire `Commands::Demo` dispatch, `emit_demo_hint_once` call sites in `setup.rs` + `theme.rs` (explicit branch only), suppression in `set.rs` (D-C3)
- [x] 15-05-PLAN.md — Wave 4: fill 10 `demo_*` integration tests, run bench, confirm full gate (fmt + clippy + test + bench) green

### Phase 16: CLI Tool Colors + New-Terminal UX
**Goal**: The active palette extends into file-listing tools (`ls`, `eza`) via managed env vars, BSD-`ls` users get an honest upgrade path instead of a lossy fallback, and adapters can truthfully tell the user when a change requires a fresh shell — with platform-appropriate phrasing that frames the new shell as the reveal, not a limitation.
**Depends on**: Phase 11's shared shell integration model (v2.1, shipped) — no v2.2 dependency. Two concerns merged intentionally: both `LS_COLORS`/`EZA_COLORS` rendering and the `RequiresNewShell` reminder plumbing touch the shell integration surface, so shared-edit territory makes them one phase.
**Requirements**: LS-01, LS-02, LS-03, UX-01, UX-02, UX-03
**Success Criteria** (what must be TRUE):
  1. User opens a new shell after applying a theme and observes `ls` output colored from the active palette (directory, exec, symlink, archive, media, code, docs groups all distinct and palette-consistent) via a slate-managed `LS_COLORS`.
  2. User opens a new shell and observes `eza` output using `EZA_COLORS` drawn from the same palette, including eza-specific keys where they diverge from GNU ls.
  3. On macOS with only BSD `ls` present, user sees a one-time capability message recommending `brew install coreutils` (for GNU `gls`) — slate does not silently emit an 8-color `LSCOLORS` fallback.
  4. At the end of `slate setup`, `slate theme …`, `slate font …`, and `slate config …` runs, user sees at most one "new-terminal" reminder per run — emitted only when at least one adapter returned `RequiresNewShell`, deduplicated across adapters.
  5. Reminder phrasing is platform-aware and active-voice: macOS points at `⌘N`; Linux says "open a new terminal"; the copy frames the new shell as the reveal of the change, with no "please" / "you need to".
**Plans:** 7/7 plans complete
Plans:
- [x] 16-01-PLAN.md — Wave 0: extend ApplyOutcome to struct variant `{ requires_new_shell: bool }` across 13 adapter constructor sites + registry match arm (UX-01 foundation)
- [x] 16-02-PLAN.md — Wave 1: build `LsColorsAdapter` with `render_ls_colors` / `render_eza_colors` / `render_strings` (LS-01, LS-02 — truecolor projection from Phase-15 classifier)
- [x] 16-03-PLAN.md — Wave 1: brand Language constants + `emit_new_shell_reminder_once` emitter + state-file helpers + `is_gnu_ls_present` detection (LS-03, UX-02, UX-03 pure-function layer)
- [x] 16-04-PLAN.md — Wave 2: extend `SharedShellModel` with `ls_colors` / `eza_colors` fields, emit from `render_shared_exports` / `render_fish_shell`, add `registry::requires_new_shell(&[ToolApplyResult])` aggregator (LS-01, LS-02, UX-02 wiring)
- [x] 16-05-PLAN.md — Wave 2: macOS-gated BSD-`ls` preflight capability check with flat state-file acknowledgement (LS-03)
- [x] 16-06-PLAN.md — Wave 3: wire reminder emit into `slate setup` / `theme <name>` / `font …` / `config` sub-commands per D-D3 order; exclude --auto / picker / opacity (UX-02, UX-03)
- [x] 16-07-PLAN.md — Wave 4: end-to-end integration tests + eza truecolor empirical smoke test + human UAT checkpoint (LS-01, LS-02 verification)

### Phase 17: Editor Adapter — Vim/Neovim Colorschemes
**Goal**: `slate theme set <id>` propagates to Neovim — each built-in theme renders as a slate-generated Lua colorscheme file under nvim's runtime path, with a slate-managed loader + file-watcher hot-reload and a 3-way consent prompt for activating on the user's init.lua. Classic vim is out of scope (D-01 in Phase 17 CONTEXT.md).
**Depends on**: Phase 15's `PaletteRenderer` + role system (shipped); Phase 11's managed-include contract (shipped). No other v2.2 dependency.
**Requirements**: EDITOR-01
**Success Criteria** (what must be TRUE):
  1. After `slate theme set <id>` and opening a running nvim instance, the editor's syntax highlight reflects the active palette — at minimum Normal, Comment, String, Keyword, Function, Constant, Error, DiffAdd/Change/Delete, StatusLine, LineNr, and every treesitter + LSP semantic-token group in the ~400-group table render in palette-consistent colors.
  2. The adapter follows the existing slate adapter contract: slate owns files at `~/.config/nvim/colors/slate-<variant>.lua` (18 files) + `~/.config/nvim/lua/slate/init.lua` (loader) + `~/.cache/slate/current_theme.lua` (state), reports `ApplyOutcome::Applied { requires_new_shell: false }`, and hot-reloads running nvim instances via `vim.uv.new_fs_event()` on the state file.
  3. Slate's activation line in `init.lua` is a single `pcall(require, 'slate')` wrapped in a slate-managed marker block — written ONLY on explicit consent via a 3-way A/B/C prompt during `slate setup`. `slate clean` best-effort removes the line; `slate config editor disable` removes ONLY the line while keeping colors/ intact.
  4. All 18 built-in theme variants produce a colorscheme that passes `nvim --headless -c 'luafile %' -c 'q'` syntax validation in CI (via the `has-nvim` feature flag + `rhysd/action-setup-vim@v1` action).
  5. Works on macOS + Linux. When nvim is missing or < 0.8, slate emits a one-line capability hint (`NVIM_MISSING_HINT` / `NVIM_TOO_OLD_HINT`) and skips silently — no error.
**Plans:** 9/9 plans complete
Plans:
- [x] 17-00-PLAN.md — Wave 0 scaffolding: has-nvim feature, CI nvim install, empty adapter module, 7 ignored integration-test stubs (+ 1 always-passing sanity test)
- [x] 17-01-PLAN.md — Wave 1 design: 6 SemanticColor variants + Palette::resolve + nvim_highlights.rs base/treesitter/LSP table (≥ 262 entries)
- [x] 17-02-PLAN.md — Wave 2 renderer core: render_colorscheme + render_shim (pure fns, snapshot-locked)
- [x] 17-03-PLAN.md — Wave 3 loader + state: render_loader (debounce/uv-compat/VimLeavePre/package-load-guard) + write_state_file + SlateEnv::slate_cache_dir
- [x] 17-04-PLAN.md — Wave 4 plugins + lualine: telescope/neo-tree/GitSigns/which-key/blink.cmp/nvim-cmp (≥ 130 entries) + lualine_theme + loader LUALINE_THEMES splice
- [x] 17-05-PLAN.md — Wave 5 adapter trait: NvimAdapter (apply_theme fast path, setup slow path) + registry + version_check + slate theme set hook
- [x] 17-06-PLAN.md — Wave 6 CLI flows: brand/language consent copy + setup 3-way prompt + clean + config editor disable + capability-hint surfacing
- [x] 17-07-PLAN.md — Wave 7 integration: fill 7 ignored test stubs with nvim-headless gates + atomicity/debounce/Pitfall-4 regression tests
- [x] 17-08-PLAN.md — Wave 8 housekeeping: rewrite REQUIREMENTS / ROADMAP / STATE wording per CONTEXT.md §domain

### Phase 18: Brand Sketch + CLI Text-Role System
**Goal**: slate's CLI output gets a unified visual voice — command keys, paths, keyboard shortcuts, status severity, and quoted code each have a consistent role-based style. Sketch first (throw-away HTML / ANSI mocks), pick the winning treatment, then roll it out across every user-facing surface.
**Depends on**: No hard dependency. Runs before Phase 19 so the demo rebuild inherits the new text-role system; runs before Phase 21 so Solarized's reveal surfaces use the new brand.
**Requirements**: BRAND-01, BRAND-02
**Success Criteria** (what must be TRUE — refined during `/gsd-discuss-phase 18`):
  1. A `/gsd-sketch` artifact captures 3–4 candidate text-role treatments side-by-side (command keys, paths, shortcuts, status severities, quoted code); user picks one and the picked variant gets codified in `src/brand/`.
  2. A text-role system (module or helper functions) replaces ad-hoc ANSI escapes across setup wizard, `slate theme` output, `slate status`, completion receipts, error surfaces, and new-shell reminders — every user-facing surface emits via the role API.
  3. Regression tests lock the ANSI byte sequences per role so brand drift is caught at CI.
**Plans**: TBD — start with `/gsd-sketch` then `/gsd-discuss-phase 18`.

### Phase 19: `slate demo` Redesign — Picker + Live Preview
**Goal**: `slate demo` stops being a one-shot showcase and becomes an interactive surface where navigating theme variants live-previews the full stack (terminal bg, prompt, syntax, file listing, diff, git UI, nvim) in-place — user sees the "wow" moment per variant without applying + rolling back.
**Depends on**: Phase 18 (text-role system drives the picker chrome); existing Phase 6 picker UX debt (memory: `project_phase6_picker_ux_debt`) gets addressed as part of this redesign.
**Requirements**: DEMO-03
**Success Criteria** (what must be TRUE — refined during `/gsd-discuss-phase 19`):
  1. User runs `slate demo` (or `slate demo <theme>`) and sees a picker listing all 20 variants grouped by family.
  2. As the user moves through the picker, the preview panel live-updates the full stack without mutating persistent config — at minimum ghostty bg, starship prompt, bat snippet, delta diff, eza listing, lazygit mini-UI, nvim syntax.
  3. `Enter` applies the highlighted variant (same semantics as `slate theme <id>`); `Esc` / `q` exits without applying. Preview state is ephemeral.
**Plans**: TBD

### Phase 20: Sound Design + Promo Assets
**Goal**: slate gains a subtle, premium sound layer — small SFX at key moments (theme set success, picker move, error) — plus a set of VHS-scripted promo recordings for README and launch. Sound is opt-in, off by default, never intrusive.
**Depends on**: Phase 18 (text-role system — brand consistency applies to sound triggers too); Phase 19 (picker is a primary trigger surface); Phase 15 demo (shipped) for recording subject.
**Requirements**: AUDIO-01
**Success Criteria** (what must be TRUE — refined during `/gsd-discuss-phase 20`):
  1. SFX library selected / recorded + licensing resolved; cross-platform audio playback path works on macOS + Linux without extra daemons.
  2. Opt-in via `slate config set sound enable`; default off; respects `--quiet`. At most one SFX per user action — no stacking.
  3. VHS-scripted promo recordings (demo picker, `slate theme set`, full setup wizard) produced as `.tape` + rendered outputs for embedding in README / launch posts.
**Plans**: TBD

### Phase 21: Theme Family Expansion — Solarized
**Goal**: Solarized Dark + Light land as first-class palettes with full tool-backend coverage matching the existing quality bar, and the `family` grouping already present in `themes/themes.toml` becomes navigable surface — so the 20+ variants across 10 families stop being a flat list.
**Depends on**: No hard v2.2 dependency. Deliberately scheduled LAST so its reveal benefits from Phase 18's new brand + Phase 19's live-preview picker + Phase 20's SFX — applying Solarized with the full polished experience is the intended payoff moment.
**Requirements**: FAM-01, FAM-02
**Success Criteria** (what must be TRUE):
  1. User runs `slate theme set solarized-dark` or `slate theme set solarized-light` and sees palette-correct output across every existing tool backend (Ghostty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting) — matching the visual fidelity of existing themes.
  2. Both Solarized variants pass the strict WCAG 4.5:1 contrast gate and the theme-registry validation (hex format, required tool_refs, auto_pair cross-reference) introduced during the Codex refactor.
  3. Running `slate theme --list` (or equivalent listing surface) shows themes grouped by `family`, with Solarized appearing as a family alongside Catppuccin, Tokyo Night, Dracula, Nord, Gruvbox, Everforest, Kanagawa, and Rose Pine.
  4. Browsing the interactive picker (from Phase 19 redesign), the user sees themes organized by family rather than a flat 20-item scroll, and family headings are visually distinct from individual variants.
  5. The Solarized auto-pair (dark ↔ light) is wired so `slate theme --auto` flips correctly between the two variants when system appearance changes.
**Plans**: TBD

### Phase 22: README Rewrite + Release Polish
**Goal**: README reflects the matured v2.2 product (interactive demo, nvim adapter, sound cues, Solarized, family grouping), embeds polished recordings + screenshots, and the release scaffolding (CHANGELOG, brew tap, release notes template) is ready for the v2.2 tag.
**Depends on**: Phase 20 (VHS recordings) + Phase 21 (Solarized ships before README mentions it). Last phase of v2.2.
**Requirements**: DOCS-01, DOCS-02
**Success Criteria** (what must be TRUE — refined during `/gsd-discuss-phase 22`):
  1. README front page reads like a launch landing — hero recording, 30-second onboarding, command reference, theme gallery with all 20 variants, honest platform matrix.
  2. CHANGELOG has a complete v2.2 entry; release notes draft captures demo redesign + nvim adapter + sound + Solarized as the narrative beats.
  3. brew tap formula / cargo-dist automation validated for the v2.2 release cut.
**Plans**: TBD

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 10. Capability Foundation | v2.1 | 3/3 | Complete | 2026-04-17 |
| 11. Shared Shell Core | v2.1 | 3/3 | Complete | 2026-04-17 |
| 12. Linux Platform Backends | v2.1 | 3/3 | Complete | 2026-04-17 |
| 13. Terminal Integration + Cross-Platform UX | v2.1 | 3/3 | Complete | 2026-04-17 |
| 14. Release Matrix + Installer + Docs | v2.1 | 3/3 | Complete | 2026-04-17 |
| 15. Palette Showcase — `slate demo` | v2.2 | 6/6 | Complete    | 2026-04-18 |
| 16. CLI Tool Colors + New-Terminal UX | v2.2 | 7/7 | Complete    | 2026-04-18 |
| 17. Editor Adapter — Neovim Colorschemes | v2.2 | 9/9 | Complete   | 2026-04-19 |
| 18. Brand Sketch + CLI Text-Role System | v2.2 | 0/? | Not started | — |
| 19. `slate demo` Redesign — Picker + Live Preview | v2.2 | 0/? | Not started | — |
| 20. Sound Design + Promo Assets | v2.2 | 0/? | Not started | — |
| 21. Theme Family Expansion — Solarized | v2.2 | 0/? | Not started | — |
| 22. README Rewrite + Release Polish | v2.2 | 0/? | Not started | — |

---

*Roadmap reorganized: 2026-04-18 at v2.1 milestone close. v2.2 phases 15–18 planned 2026-04-18. Phase 16 plans defined 2026-04-18 (7 plans across 5 waves). Phase 17 redefined from research spike to shipping editor adapter 2026-04-18. Phase 16 completed + shipped as v0.1.2 on 2026-04-18. Phase 17 completed 2026-04-19 (9 plans, nvim-only adapter with 18 colorschemes + loader + file-watcher hot-reload + 3-way consent prompt). v2.2 restructured 2026-04-19: original 4-phase scope (15–18) expanded to 8 phases (15–22) so the milestone ships as one cohesive release — added Phase 18 brand-sketch, Phase 19 demo-redesign, Phase 20 sound+promo, Phase 22 README+release-polish; Solarized moved from Phase 18 to Phase 21 so its reveal is amplified by the new brand/demo/sound work.*
