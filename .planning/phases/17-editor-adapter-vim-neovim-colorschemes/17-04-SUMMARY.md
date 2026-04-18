---
phase: 17
plan: 04
subsystem: design+adapter-nvim
tags: [editor-adapter, nvim, tdd, plugin-coverage, lualine, plan-04]
dependency_graph:
  requires:
    - "src/design/nvim_highlights.rs::HIGHLIGHT_GROUPS (Plan 01 — 270 base entries)"
    - "src/theme::Palette::resolve (Plan 01 cascade)"
    - "src/adapter/nvim.rs::render_loader (Plan 03 — LOADER_TEMPLATE_{HEAD,MID,TAIL})"
    - "src/theme::ThemeRegistry::all (stable TOML order — deterministic splice)"
  provides:
    - "src/design/nvim_highlights.rs::lualine_theme(&Palette) -> String"
    - "+136 plugin HIGHLIGHT_GROUPS entries (telescope 17, neo-tree 30, GitSigns 10, which-key 6, blink.cmp 39, nvim-cmp 34)"
    - "LUALINE_THEMES splice loop in render_loader — one ['<id>'] per variant"
  affects:
    - "Plan 05 (NvimAdapter::apply_setup — writes the now-complete loader)"
    - "Plan 07 (integration tests — loader size envelope, lualine refresh gate)"
tech-stack:
  added: []
  patterns:
    - "pure-render-function (lualine_theme mirrors render_colorscheme shape)"
    - "deterministic splice loop: ThemeRegistry::all() iteration between LOADER_TEMPLATE_MID and _TAIL"
    - "blink.cmp + nvim-cmp kind-parity (D-08 both-emit policy)"
key-files:
  created:
    - ".planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-04-SUMMARY.md"
  modified:
    - "src/design/nvim_highlights.rs (1009 LOC → 1834 LOC; +136 plugin entries + lualine_theme + 14 new tests)"
    - "src/adapter/nvim.rs (888 LOC → 963 LOC; +lualine splice loop + 3 new tests)"
    - "src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap (270-entry → 406-entry output)"
key-decisions:
  - "SemanticColor variant-name reconciliation: plan referenced `FileVideo`/`FileSource`/`FileDoc` but the actual enum has `FileMedia`/`FileCode`/`FileDocs`. Used the actual variants — same semantics, spelling mismatch in the plan draft. Plan 01 established `Status` / `Comment` as the substitutes for the non-existent `Info` / `Hint` variants; this plan continues that convention."
  - "Total entry count: 406 (270 base + 136 plugin) — exceeds the 392 floor by 14. 136 plugin entries land above the plan's 130 minimum because telescope included 4 extra sub-borders (prompt/preview/results) and nvim-cmp added TabNine + Codeium Copilot alternates explicitly requested by §Pattern 5."
  - "Snapshot drift: Plan 02's locked `…catppuccin_mocha.snap` captured the 270-entry output; after Task 1 the rendered colorscheme naturally grows to 406 entries (+19 KB). Mechanical `.snap.new` → `.snap` rename (same path Plan 02 used since `cargo-insta` is unavailable on this runner). New snapshot reviewed before promotion — leading comment + 270 existing groups unchanged, 136 plugin entries appended in declaration order."
  - "`lualine_theme` is a pure function taking `&Palette` — not an `impl` method. Easier to test in isolation and parallels `render_colorscheme`'s shape. Returns a `String` carrying a Lua table literal wrapped in `{ … }` with 4-space indent so it nests cleanly inside `render_loader`'s 2-space-indented `LUALINE_THEMES` block."
  - "Loader size upper bound kept at 512 KB (matching Plan 03's existing assertion) rather than the plan's 256 KB value. Plan 04's draft assumed Plan 03's loader was ~15 KB; Plan 03's own summary records 230 KB, and post-Task-1 it grows to 343 KB before lualine splicing. Rule 3 deviation documented inline in `render_loader_size_adjusted_for_lualine`."
  - "Per-mode accent mapping (lualine_theme): normal→Accent, insert→String, visual→Warning, replace→Error, command→Keyword, inactive→Muted on Surface. Matches tokyonight/catppuccin lualine theme precedent. All `a` sections bolded (active OR inactive) so the mode pill shape reads as a single visual element regardless of focus."
metrics:
  duration_seconds: 611
  duration_human: "10m 11s"
  completed_at: "2026-04-18T17:48:10Z"
  tasks_completed: 3
  tdd_phases: ["RED", "GREEN", "RED", "GREEN", "RED", "GREEN"]
  commits: 6
  files_created: 1
  files_modified: 3
  lib_tests_before: 618
  lib_tests_after: 635
  new_tests_added: 17
  highlight_groups_before: 270
  highlight_groups_after: 406
  plugin_entries_added: 136
  nvim_highlights_loc_before: 1009
  nvim_highlights_loc_after: 1834
  nvim_rs_loc_before: 888
  nvim_rs_loc_after: 963
---

# Phase 17 Plan 04: Plugin coverage + lualine runtime refresh Summary

Landed the plugin-integration surface (6 plugins, 136 highlight groups — 6
above the ≥ 130 floor) and the lualine runtime refresh path that closes D-08:
`HIGHLIGHT_GROUPS` grows from 270 → 406 entries with telescope / neo-tree /
GitSigns / which-key / blink.cmp / nvim-cmp coverage; `lualine_theme(palette)`
emits a deterministic 6-mode × 3-section Lua table literal; and
`render_loader` now splices one `['<id>'] = <lualine table>` entry per
built-in variant into the previously-empty `LUALINE_THEMES` block so the
Pitfall-5 lualine guard in `M.load` actually fires when lualine is loaded.

## D-06 reconciliation

**Total highlight groups = 262 base/treesitter/LSP (Plan 01) + 130 plugin
(Plan 04) = 392, which exceeds D-06's ~300 target because plugin groups
are counted in the same table. This is intentional — plugin coverage is
additive.**

Actual final count is **406** entries (270 base including Plan 01's
back-compat aliases + 136 plugin including the 4 telescope sub-borders
and the TabNine/Codeium Copilot alternates explicitly called out in
§Pattern 5). Fourteen entries above the 392 floor; no scope creep —
every entry is in the authoritative list.

## Performance

- **Duration:** 10m 11s (611 seconds)
- **Completed:** 2026-04-18T17:48:10Z
- **Tasks:** 3 / 3 (all `auto tdd="true"`)
- **Commits:** 6 (3 RED + 3 GREEN, no REFACTORs needed)
- **Files modified:** 3 (`src/design/nvim_highlights.rs`, `src/adapter/nvim.rs`, snapshot)
- **LOC delta:** nvim_highlights.rs 1009 → 1834 (+825); nvim.rs 888 → 963 (+75)

## Accomplishments

- **`HIGHLIGHT_GROUPS` reaches 406 entries** — 270 base + 136 plugin across
  6 authoritative plugin families. Every plugin block uses only existing
  `SemanticColor` variants (no enum churn) and routes through
  `Palette::resolve` so palette switches propagate automatically.
- **`lualine_theme(&Palette) -> String`** — pure, deterministic, 6 modes ×
  3 sections, 7-char hex literals throughout, wrapped in `{ … }` ready
  for splicing into the loader's `LUALINE_THEMES` block.
- **`render_loader` now splices 18 lualine tables** — one per built-in
  variant — between `LOADER_TEMPLATE_MID` (which opens `LUALINE_THEMES`)
  and `LOADER_TEMPLATE_TAIL` (which closes it and begins `M.load`). Plan
  03's lualine-guard path (`if package.loaded['lualine'] and LUALINE_THEMES[variant]`)
  no longer silently short-circuits on `nil`.

## Lualine per-mode palette mapping (one-line summary)

**`a` (mode pill):** normal→Accent, insert→String, visual→Warning,
replace→Error, command→Keyword, inactive→Muted on Surface — all bolded.
**`b` (mid-bar):** Text on Surface (active) / Muted on Surface (inactive).
**`c` (fill):** Text on Background (active) / Muted on Background (inactive).

## Output shape (loader)

With plugin + lualine additions the loader grows from Plan 03's
230,477 bytes / 4,984 lines to approximately 366 KB / 7,400 lines
(18 variants × +6 KB PALETTES entries for plugin groups +
18 variants × ~1.3 KB LUALINE_THEMES entries). Still well inside the
512 KB envelope.

```
~366 KB / 18 variant entries each in both PALETTES and LUALINE_THEMES
 ├── LOADER_TEMPLATE_HEAD          (   ~280 bytes)
 ├── 18 × "  ['<id>'] = { 406 groups },"   (~18-20 KB per variant)
 ├── LOADER_TEMPLATE_MID           (   ~140 bytes)
 ├── 18 × "  ['<id>'] = <lualine table>,"  (~1.3 KB per variant)
 └── LOADER_TEMPLATE_TAIL          (  ~1,850 bytes)
```

## Per-task Commits

| Task | Phase | Commit    | Message                                                                                    |
| ---- | ----- | --------- | ------------------------------------------------------------------------------------------ |
| 1    | RED   | `1ed54d5` | `test(17-04): add failing plugin-coverage tests (telescope, neo-tree, GitSigns, …)`        |
| 1    | GREEN | `7891917` | `feat(17-04): add 136 plugin highlight entries (telescope, neo-tree, GitSigns, …)`         |
| 2    | RED   | `18e9c18` | `test(17-04): add failing tests for lualine_theme function`                                |
| 2    | GREEN | `25b3339` | `feat(17-04): implement lualine_theme pure function`                                       |
| 3    | RED   | `fbd51f6` | `test(17-04): add failing tests for LUALINE_THEMES splice in render_loader`                |
| 3    | GREEN | `687db59` | `feat(17-04): splice per-variant lualine tables into LUALINE_THEMES block`                 |

REFACTOR steps intentionally skipped: each GREEN implementation was
already idiomatic (static data table append, small closure for
mode emission, simple splice loop). No behaviour-preserving cleanup
warranted a third commit per task.

## Tests Added

### Task 1 — plugin coverage (7 new tests + 1 tightened)

| Test                                                 | Guards                                                            |
| ---------------------------------------------------- | ----------------------------------------------------------------- |
| `group_count_meets_coverage_floor` (tightened)       | `HIGHLIGHT_GROUPS.len() >= 392`                                   |
| `plugin_telescope_groups_present`                    | 13 named telescope groups                                         |
| `plugin_neotree_groups_present`                      | 13 named neo-tree groups                                          |
| `plugin_gitsigns_groups_present`                     | ≥ 10 `GitSigns*` entries                                          |
| `plugin_which_key_groups_present`                    | ≥ 6 `WhichKey*` entries                                           |
| `plugin_blink_and_cmp_both_emit_kind_parity`         | blink + cmp base sets present; ≥ 18 `*Kind*` sub-variants each    |
| `deprecated_groups_use_undercurl`                    | `BlinkCmpLabelDeprecated` + `CmpItemAbbrDeprecated` → Undercurl   |
| `match_groups_use_bold`                              | 4 match groups (Telescope + blink + 2× cmp) → Bold                |
| `link_targets_resolve_or_reference_builtin` (extended) | Plugin link targets covered (FloatTitle, Pmenu*, VertSplit, …) |

### Task 2 — lualine_theme (7 new tests)

| Test                                         | Guards                                                           |
| -------------------------------------------- | ---------------------------------------------------------------- |
| `lualine_theme_contains_all_six_modes`       | normal, insert, visual, replace, command, inactive               |
| `lualine_theme_each_mode_has_abc_sections`   | exactly 6 `a = {`, 6 `b = {`, 6 `c = {`                          |
| `lualine_theme_a_sections_are_bold`          | exactly 6 `gui = 'bold'` per palette                             |
| `lualine_theme_is_deterministic`             | Two calls return byte-identical strings                          |
| `lualine_theme_hex_values_are_7_chars`       | Every `'#…'` literal is 7 chars                                  |
| `lualine_theme_wraps_with_braces`            | Starts `{`, ends `}`                                             |
| `lualine_theme_inactive_uses_muted_fg`       | Muted hex appears inside `inactive = { … }` block                |

### Task 3 — render_loader lualine splice (3 new tests)

| Test                                                           | Guards                                                               |
| -------------------------------------------------------------- | -------------------------------------------------------------------- |
| `render_loader_populates_lualine_themes_for_all_variants`      | Every variant id present as `['<id>']` inside LUALINE_THEMES block   |
| `render_loader_lualine_entries_are_bold_capable`               | ≥ 60 `gui = 'bold'` markers (6 per variant × 18 = 108 expected)      |
| `render_loader_size_adjusted_for_lualine`                      | 8 KB ≤ len ≤ 512 KB                                                  |

**Total:** 17 new tests across the plan. All green, sub-second run time.

## Files Created / Modified

- **`src/design/nvim_highlights.rs`** (1009 → 1834 LOC) — adds 136 plugin
  HIGHLIGHT_GROUPS entries (6 families), `pub fn lualine_theme`, 14 new
  unit tests; extends `link_targets_resolve_or_reference_builtin` built-in
  allowlist for plugin link targets.
- **`src/adapter/nvim.rs`** (888 → 963 LOC) — imports `lualine_theme` from
  `crate::design::nvim_highlights`; replaces the empty-comment `LUALINE_THEMES`
  body with an 18-iteration splice loop; 3 new unit tests.
- **`src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap`**
  — regenerated (270 entries → 406 entries, 12,680 → 19,012 bytes).
- **`.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-04-SUMMARY.md`** (this file).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] Plan referenced non-existent `SemanticColor` variants**

- **Found during:** Task 1 GREEN compilation scaffolding (same issue Plan
  01 noted in its deviations).
- **Issue:** The plan's §Pattern 5 mappings use `FileVideo`, `FileSource`,
  and `FileDoc`; the actual enum defines `FileMedia`, `FileCode`, and
  `FileDocs`. (Plan 01 already established that `Info` / `Hint` don't
  exist and mapped them to `Status` / `Comment` — this plan's mappings
  don't reach into those variants directly.)
- **Fix:** Used the actual enum names — `FileDocs` for BlinkCmpKindFile /
  CmpItemKindFile. `FileVideo` and `FileSource` weren't reached by Plan
  04's mappings, so no substitution was needed there. Preserves Plan 04's
  contract of "no enum churn" exactly.
- **Files modified:** `src/design/nvim_highlights.rs` only.
- **Commit:** `7891917`.

**2. [Rule 3 — Blocking] Plan's 256 KB upper bound on `render_loader` size incompatible with Task-1's table growth**

- **Found during:** Task 3 RED gate.
- **Issue:** Plan 04's `render_loader_size_adjusted_for_lualine` asserts
  `out.len() <= 256 * 1024`. Plan 03's own summary records a baseline of
  230 KB, and Task 1's addition of 136 plugin entries grows the rendered
  colorscheme output by ~6-7 KB per variant × 18 variants ≈ 113 KB,
  pushing the pre-lualine loader to 343 KB. The 256 KB cap was drafted
  on an out-of-date "Plan 03 loader was 15 KB" assumption.
- **Fix:** Kept the upper bound at 512 KB (matching Plan 03's existing
  `render_loader_size_is_bounded` assertion). Lower bound moved to 8 KB
  per the plan's stated intent. Rule-3 deviation documented inline as a
  doc comment inside the new test.
- **Files modified:** `src/adapter/nvim.rs` test only.
- **Commit:** `fbd51f6`.

### Plan-shape Clarifications (not deviations)

- **Telescope count is 17, not 13.** The plan's <behavior> minimum is
  ≥ 13; the plan's <action> list actually specifies 17 entries
  (3 extra border sub-groups: prompt/preview/results border; plus
  TelescopeSelection's `bg`-only HighlightSpec is counted).
  The 13 in <behavior> is a floor, not an exact count. Implemented the
  full 17-entry list.
- **nvim-cmp count is 34, not 32.** The plan's <action> text says "32
  entries" but then lists all 26 kind variants + 6 base + TabNine + Codeium = 34.
  Implemented the full 34-entry list (both §Pattern 5 and the plan's own
  explicit inventory call out TabNine + Codeium).
- **Snapshot drift:** Plan 02's locked catppuccin-mocha snapshot grows from
  270 entries (12 KB) to 406 entries (19 KB) because Plan 04's whole job
  is to grow `HIGHLIGHT_GROUPS`. Mechanical `.snap.new` → `.snap` rename,
  exactly the procedure Plan 02 documented.

### Auth Gates

None — pure-Rust changes with no external auth.

### Out of Scope (deferred to Plan 05+)

- `NvimAdapter` struct + `ToolAdapter` impl → Plan 05.
- `slate setup` integration (writing the loader to disk) → Plan 05.
- Integration tests (`nvim --headless -c 'luafile %'`) → Plan 07.

## Verification

| Gate                                                              | Result                      |
| ----------------------------------------------------------------- | --------------------------- |
| `cargo test --lib nvim_highlights::tests`                         | 23 / 23 pass (+14 new)      |
| `cargo test --lib adapter::nvim::tests`                           | 33 / 33 pass (+3 new)       |
| `cargo test --lib`                                                | 635 / 635 pass (+17 new)    |
| `cargo clippy --all-targets --all-features -- -D warnings`        | 0 warnings                  |
| `cargo fmt --all -- --check`                                      | no diff                     |
| `grep -c "BlinkCmpKind" src/design/nvim_highlights.rs`            | 30 (≥ 25)                   |
| `grep -c "CmpItemKind" src/design/nvim_highlights.rs`             | 32 (≥ 26)                   |
| `grep -n "pub fn lualine_theme" src/design/nvim_highlights.rs`    | 1 match                     |
| `grep -n "lualine_theme(&variant" src/adapter/nvim.rs`            | 1 match                     |
| `HIGHLIGHT_GROUPS.len()` (final)                                  | 406 (≥ 392)                 |
| `render_loader()` size                                            | ~366 KB (in 8 KB–512 KB)    |
| Plan 02 insta snapshot drift                                      | Promoted (406-entry output) |

## Architecture Notes for Plan 05

- **`lualine_theme` is pure and cheap** (~1.3 KB output, single
  `String::with_capacity` + writeln!). `NvimAdapter::apply_theme`'s fast
  path does NOT need to re-render lualine themes — the loader already
  contains all 18 pre-rendered, and `M.load` dispatches them via
  `LUALINE_THEMES[variant]`.
- **`render_loader()` now emits ~366 KB**; `NvimAdapter::apply_setup`
  should write it via `AtomicWriteFile` (same pattern as `write_state_file`)
  to `<home>/.config/nvim/lua/slate/init.lua`. The file is large but
  still well below reasonable filesystem limits.
- **`LUALINE_THEMES` splice uses the same `ThemeRegistry::all()` order**
  as the `PALETTES` splice, so the two blocks stay in lockstep. If a
  future plan adds a variant, both blocks pick it up automatically.

## Known Stubs

None. Every highlight group, every lualine theme entry is wired to a real
`SemanticColor` role → palette hex. The loader's `M.setup()` is already
called at file load (Plan 03), so the spliced lualine tables become live
the moment the loader is `require`d.

## Self-Check: PASSED

**Created files exist:**

- `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-04-SUMMARY.md` — being written now

**Modified files reflect changes:**

- `src/design/nvim_highlights.rs` — 1834 LOC, `pub fn lualine_theme` present, HIGHLIGHT_GROUPS has 406 entries
- `src/adapter/nvim.rs` — 963 LOC, `lualine_theme(&variant.palette)` splice present, import line updated
- `src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap` — 19,012 bytes (was 12,680)

**Commits on branch:**

- `1ed54d5` — `test(17-04): add failing plugin-coverage tests …` — FOUND
- `7891917` — `feat(17-04): add 136 plugin highlight entries …` — FOUND
- `18e9c18` — `test(17-04): add failing tests for lualine_theme function` — FOUND
- `25b3339` — `feat(17-04): implement lualine_theme pure function` — FOUND
- `fbd51f6` — `test(17-04): add failing tests for LUALINE_THEMES splice …` — FOUND
- `687db59` — `feat(17-04): splice per-variant lualine tables into LUALINE_THEMES block` — FOUND

All six hashes present in `git log 3f3a312..HEAD`. clippy / fmt / test
gates all green. Plan 17-04 is complete.

## TDD Gate Compliance

Plan-level `type: execute` (not `tdd`), but each individual task used
`tdd="true"`. RED → GREEN discipline observed across all 3 tasks:

| Task | RED commit | GREEN commit | REFACTOR |
| ---- | ---------- | ------------ | -------- |
| 1    | `1ed54d5`  | `7891917`    | skipped  |
| 2    | `18e9c18`  | `25b3339`    | skipped  |
| 3    | `fbd51f6`  | `687db59`    | skipped  |

Each RED commit introduced at least one assertion that failed at RED-
time (plugin-coverage assertions for Task 1, undefined-symbol compile
error for Task 2 pre-implementation, LUALINE_THEMES-key absence +
bold-count assertions for Task 3). Each GREEN commit closed exactly those
failing assertions without introducing new TODOs.

---

*Phase: 17-editor-adapter-vim-neovim-colorschemes*
*Plan: 04 (Wave 4 — plugin coverage + lualine runtime refresh)*
*Completed: 2026-04-18*
