---
roadmap_version: 3.0
milestone: v2.2
milestone_name: editor-ecosystem-polish
phase_count: 4
granularity: balanced
created: "2026-04-16"
last_updated: "2026-04-18"
---

# Roadmap: slate

## Milestones

- ✅ **v1.0 themectl** — archived (superseded by v2.0 pivot, see `.planning/archive/v1.0-themectl/`)
- ✅ **v2.0 pre-v2.1 snapshot** — pre-pivot v2.0 milestone docs snapshotted for reference (see `.planning/milestones/v2.0-pre-v2.1/`)
- ✅ **v2.1 Cross-Platform Core** — Phases 10–14 (shipped 2026-04-17)
- 📋 **v2.2 Editor Ecosystem + Polish** — Phases 15–18 (planning)

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

### 📋 v2.2 Editor Ecosystem + Polish (Phases 15–18)

- [x] **Phase 15: Palette Showcase — `slate demo`** — curated single-screen payoff render + contextual hint surfacing (completed 2026-04-18)
- [ ] **Phase 16: CLI Tool Colors + New-Terminal UX** — `LS_COLORS` / `EZA_COLORS` from the active palette + cross-platform `RequiresNewShell` reminder plumbing
- [ ] **Phase 17: Editor Adapter Research Spike** — license + portability evaluation of vim/nvim colorscheme plugins, go/no-go recommendation for v2.3
- [ ] **Phase 18: Theme Family Expansion — Solarized** — Solarized Dark + Light full-backend coverage, `family` grouping surfaced in listing + picker

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
**Plans**: TBD

### Phase 17: Editor Adapter Research Spike
**Goal**: Produce a research artifact (not production code) that answers, with evidence, whether existing vim/nvim colorscheme plugins can be absorbed into slate under a compatible license — culminating in a concrete go/no-go recommendation and an architecture sketch for the v2.3 editor adapter, so v2.3 planning starts from a decided direction instead of open-ended debate.
**Depends on**: None (pure research; no code dependency).
**Requirements**: EDITOR-01
**Success Criteria** (what must be TRUE):
  1. A research artifact exists at a discoverable location (e.g. `.planning/spikes/editor-adapter/SPIKE.md`) and is linked from `.planning/PROJECT.md` or the v2.2 phase index so future milestones can find it.
  2. The artifact includes a license analysis covering at minimum the five listed candidate plugins — catppuccin-nvim, tokyonight.nvim, gruvbox.nvim, rose-pine/neovim, and solarized — with the license verbatim (or linked) and a compatibility verdict for absorption into slate's codebase.
  3. The artifact includes a per-candidate portability assessment (how tightly each plugin is coupled to Lua-only APIs, to specific plugin managers, or to non-portable runtime state) and rates each on a shared scale.
  4. The artifact ends with a single explicit go/no-go recommendation for v2.3 — naming the preferred integration approach (absorb upstream code vs. slate-generated drop-in vs. plugin-manager spec) — and sketches the minimum adapter shape v2.3 would implement.
  5. No new production `src/` code ships with this phase; any experimental code exists only as a scoped proof-of-concept referenced by the spike, not wired into the CLI surface.
**Plans**: TBD

### Phase 18: Theme Family Expansion — Solarized
**Goal**: Solarized Dark + Light land as first-class palettes with full tool-backend coverage matching the existing quality bar, and the `family` grouping already present in `themes/themes.toml` becomes navigable surface — so the 20+ variants across 10 families stop being a flat list.
**Depends on**: No hard v2.2 dependency. Ideally lands last in the milestone because Phase 16's new-terminal reminder and Phase 15's `slate demo` both amplify Solarized's reveal moment — applying Solarized and seeing the full showcase + env-driven `ls` colors together is the intended payoff.
**Requirements**: FAM-01, FAM-02
**Success Criteria** (what must be TRUE):
  1. User runs `slate theme set solarized-dark` or `slate theme set solarized-light` and sees palette-correct output across every existing tool backend (Ghostty, Alacritty, Starship, bat, delta, eza, lazygit, fastfetch, tmux, zsh-syntax-highlighting) — matching the visual fidelity of existing themes.
  2. Both Solarized variants pass the strict WCAG 4.5:1 contrast gate and the theme-registry validation (hex format, required tool_refs, auto_pair cross-reference) introduced during the Codex refactor.
  3. Running `slate theme --list` (or equivalent listing surface) shows themes grouped by `family`, with Solarized appearing as a family alongside Catppuccin, Tokyo Night, Dracula, Nord, Gruvbox, Everforest, Kanagawa, and Rose Pine.
  4. Browsing the interactive picker, the user sees themes organized by family rather than a flat 20-item scroll, and family headings are visually distinct from individual variants.
  5. The Solarized auto-pair (dark ↔ light) is wired so `slate theme --auto` flips correctly between the two variants when system appearance changes.
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
| 16. CLI Tool Colors + New-Terminal UX | v2.2 | 0/? | Not started | — |
| 17. Editor Adapter Research Spike | v2.2 | 0/? | Not started | — |
| 18. Theme Family Expansion — Solarized | v2.2 | 0/? | Not started | — |

---

*Roadmap reorganized: 2026-04-18 at v2.1 milestone close. v2.2 phases 15–18 planned 2026-04-18.*
