---
phase: 17
plan: 01
subsystem: design+theme
tags: [editor-adapter, nvim, semantic-color, highlight-groups, plan-01]
requires:
  - "src/cli/picker/preview_panel.rs::SemanticColor (Phase 15 enum)"
  - "src/theme/mod.rs::Palette::resolve (Phase 8 cascade pattern)"
  - "themes/themes.toml (18 embedded themes, OnceLock loader)"
provides:
  - "SemanticColor::Background, ::Surface, ::SurfaceAlt, ::Selection, ::Border, ::LspParameter"
  - "Palette::resolve cascading fallback arms for the 6 new variants"
  - "src/design/nvim_highlights.rs::HIGHLIGHT_GROUPS (270 entries)"
  - "src/design/nvim_highlights.rs::HighlightSpec, ::Style, ::HighlightSpec::{fg, fg_bg, bg_only, styled, styled_fg_bg, linked, style_only}"
affects:
  - "src/design/mod.rs (registers nvim_highlights module)"
  - "src/cli/picker/preview_panel.rs (extends SemanticColor enum by 6 variants)"
  - "src/theme/mod.rs (extends Palette::resolve match by 6 arms)"
tech-stack:
  added: []
  patterns:
    - "static-table data module (mirrors src/design/file_type_colors.rs shape)"
    - "cascading-fallback Option chains in Palette::resolve (Phase 8 idiom)"
    - "rstest parameterised tests across all 18 embedded themes"
key-files:
  created:
    - "src/design/nvim_highlights.rs"
    - ".planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-01-SUMMARY.md"
  modified:
    - "src/cli/picker/preview_panel.rs"
    - "src/theme/mod.rs"
    - "src/design/mod.rs"
decisions:
  - "Mapped DiagnosticInfo / DiagnosticHint family entries to existing SemanticColor::Status (cyan) and ::Comment (bright_black) instead of adding two new variants — preserves Plan-01's locked 6-variant scope."
  - "Added a small set of treesitter back-compat aliases (@namespace, @field, @parameter, @text.*) so the link-target test stays green and Plan-04 doesn't have to retro-fit them."
  - "Linked @diff.{plus,minus,delta} to DiffAdd/Delete/Change rather than re-emitting fg/bg, matching the tokyonight idiom."
  - "Cursor uses Style::Reverse (Background/Text inverted) to remain readable across both dark and light embedded themes without per-theme tuning."
metrics:
  duration_seconds: 601
  duration_human: "10m 1s"
  tasks_completed: 2
  files_created: 1
  files_modified: 3
  highlight_entries: 270
  semantic_color_variants_added: 6
  lib_tests_passing: 586
  completed_at: "2026-04-18T17:05:21Z"
---

# Phase 17 Plan 01: Design layer — SemanticColor extension + nvim highlight table Summary

Established the design data surface that the rest of the editor-adapter waves build on:
extended `SemanticColor` with six editor-theming variants (Background, Surface, SurfaceAlt,
Selection, Border, LspParameter), wired each through `Palette::resolve` with two-deep
cascading fallbacks so even minimalist palettes (Solarized, Nord) always return a printable
hex, and landed a new `src/design/nvim_highlights.rs` module exposing
`HIGHLIGHT_GROUPS: &[(&str, HighlightSpec)]` with 270 authoritative entries covering Base UI,
Diff/diagnostics, Treesitter, and LSP semantic tokens — eight clear of the ≥262 floor and
Plan-04-ready for plugin extensions on top.

## What Shipped

### Task 1 — SemanticColor + Palette::resolve

Added six variants under a new `// Editor theming (Phase 17 — consumed by src/adapter/nvim.rs)`
section in `SemanticColor`. Each variant gets a dedicated arm in `Palette::resolve` with at
least two fallback levels:

| Variant        | Cascade                                              |
| -------------- | ---------------------------------------------------- |
| `Background`   | `background` (always populated, no fallback needed)  |
| `Surface`      | `surface0` → `bg_dim` → `background`                 |
| `SurfaceAlt`   | `surface1` → `overlay0` → `bright_black`             |
| `Selection`    | `selection_bg` → `surface2` → `bright_black`         |
| `Border`       | `surface2` → `overlay0` → `bright_black`             |
| `LspParameter` | `flamingo` → `rosewater` → `yellow`                  |

All six field names referenced (`bg_dim`, `surface0`, `surface1`, `surface2`, `overlay0`,
`selection_bg`, `flamingo`, `rosewater`, `bright_black`, `yellow`) already exist on the
`Palette` struct verbatim — no field-name substitutions were necessary.

A new rstest parameterised test (`resolve_editor_variant_returns_valid_hex_for_all_themes`,
6 cases × 18 themes = 108 assertions) walks the entire `ThemeRegistry::new().all()` set
and asserts each new variant resolves to a 7-character `#RRGGBB` hex that starts with `#`
and contains only hex digits.

### Task 2 — `src/design/nvim_highlights.rs`

Added a new design data module that pure-data describes the nvim highlight table:

- `Style` enum (None, Bold, Italic, Underline, Undercurl, Reverse) — the six modifiers
  exposed by `nvim_set_hl`.
- `HighlightSpec` struct (`fg: Option<SemanticColor>`, `bg: Option<SemanticColor>`,
  `style: Style`, `link: Option<&'static str>`) with seven `const fn` constructors:
  `fg`, `fg_bg`, `bg_only`, `styled`, `styled_fg_bg`, `linked`, `style_only`.
- `pub static HIGHLIGHT_GROUPS: &[(&str, HighlightSpec)]` — the 270-entry authoritative
  table, organised into four section-commented blocks matching `17-RESEARCH.md`
  §Pattern 4.1.

Module registered alphabetically in `src/design/mod.rs`
(`pub mod nvim_highlights;` between `file_type_colors` and `presets`).

Nine tests guard the contract:

| Test                                              | Asserts                                            |
| ------------------------------------------------- | -------------------------------------------------- |
| `group_count_meets_coverage_floor`                | `HIGHLIGHT_GROUPS.len() >= 262`                    |
| `every_entry_resolves_for_every_theme`            | All fg/bg refs resolve to 7-char hex × 18 themes   |
| `core_base_ui_groups_present`                     | 20 must-have base-UI names in the table            |
| `core_treesitter_groups_present`                  | `@comment, @function, @keyword, @string, @type, @variable` |
| `core_diagnostic_groups_present`                  | `DiagnosticError/Warn/Info/Hint`                   |
| `lsp_parameter_group_is_present_and_uses_new_variant` | `@lsp.type.parameter` fg ≡ `LspParameter`      |
| `link_style_used_for_at_least_five_entries`       | At least 5 entries use `HighlightSpec::linked`     |
| `group_names_are_unique`                          | No duplicate entry names                           |
| `link_targets_resolve_or_reference_builtin`       | Every link target is in-table or known nvim built-in |

## Per-task Commits

| Task | Step | Commit    | Description                                                              |
| ---- | ---- | --------- | ------------------------------------------------------------------------ |
| 1    | RED  | `1bce769` | Add failing tests for editor-theming SemanticColor variants              |
| 1    | GREEN| `bc22ffa` | Implement Palette::resolve cascading fallbacks for editor variants       |
| 2    | RED  | `d8956e4` | Scaffold nvim_highlights module with failing coverage tests              |
| 2    | GREEN| `ee0b1d1` | Populate HIGHLIGHT_GROUPS with 270 nvim highlight entries                |

REFACTOR steps were skipped — the GREEN implementations were already idiomatic
(small const-fn constructors, flat data table, no behavior to refactor without changing semantics).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `SemanticColor::Info` / `::Hint` referenced but undefined**

- **Found during:** Task 2 GREEN compilation
- **Issue:** The plan's group mappings repeatedly used `SemanticColor::Info` (for
  `DiagnosticInfo`, `DiagnosticVirtualTextInfo`, `DiagnosticUnderlineInfo`,
  `SpellLocal`, `@comment.info`, `@text.note`) and `SemanticColor::Hint` (for the matching
  Hint family + `ComplHint` + `@comment.hint` + `SpellRare`). Neither variant exists on
  the existing enum — only `Error`, `Warning`, `Failed`, `Status` and the Phase-15 syntax
  variants are present.
- **Fix:** Mapped `Info` → existing `Status` (cyan, semantically informational) and
  `Hint` → existing `Comment` (bright_black, semantically subtle / muted). 12 sites
  updated via a single `replace_all` per variant. This preserves the Plan-01 contract
  of "exactly 6 new SemanticColor variants" — adding two more variants would have
  expanded scope and triggered the must_haves count from 6 to 8.
- **Files modified:** `src/design/nvim_highlights.rs` only (no enum/Palette changes)
- **Commit:** `ee0b1d1`

**2. [Rule 2 - Critical] Treesitter back-compat aliases missing**

- **Found during:** Task 2 GREEN authoring
- **Issue:** `@namespace`, `@field`, `@parameter`, `@text.*` are pre-0.10 treesitter
  group names that nvim still emits via legacy parsers (Helix-style configs, older
  plugins). Without them, `link_targets_resolve_or_reference_builtin` would fail for
  `@field` → `@variable.member` and `@parameter` → `@variable.parameter` linkage, and
  Plan 02's renderer would silently ship dead links to users on older grammars.
- **Fix:** Added 10 alias entries (`@namespace`, `@field` linked to `@variable.member`,
  `@parameter` linked to `@variable.parameter`, plus `@text`, `@text.literal`,
  `@text.reference`, `@text.title`, `@text.uri`, `@text.todo`, `@text.note`,
  `@text.warning`, `@text.danger`).
- **Commit:** `ee0b1d1`

### Auth Gates

None — pure-Rust design changes with no external auth.

### Plan-Verification Step Skipped

The plan's overall verification block lists `cargo check --features has-nvim` but the
`Cargo.toml` does not (yet) define a `has-nvim` feature — that gating likely lands in
Plan 02 alongside the renderer. Skipped this step intentionally; everything else in
the verification block (lib tests, clippy, fmt) passes.

## Verification

| Gate                                                              | Result               |
| ----------------------------------------------------------------- | -------------------- |
| `cargo test --lib resolve_` (Task 1, 6 new tests)                 | 6 / 6 pass           |
| `cargo test --lib nvim_highlights::tests` (Task 2, 9 tests)       | 9 / 9 pass           |
| `cargo test --lib` (full lib suite)                               | 586 / 586 pass       |
| `cargo clippy --all-targets --all-features -- -D warnings`        | 0 warnings           |
| `cargo fmt --all -- --check`                                      | no diff              |
| `HIGHLIGHT_GROUPS.len() >= 262`                                   | 270 (8 above floor)  |

## Architecture Notes for Plan 02

- The `HIGHLIGHT_GROUPS` slice ordering is now stable and intentional: definitions
  appear before any link target. Plan 02's renderer can iterate the slice in order
  and emit `vim.api.nvim_set_hl(0, "<name>", <table>)` calls — nvim resolves links
  lazily, so the order matters only for documentation clarity, not correctness.
- Every `HighlightSpec` constructor is `const fn`, so the static slice carries zero
  runtime cost and the entire table is compile-time-validated.
- `HighlightSpec::link` cohabits with `fg`/`bg`/`style` in the same struct rather than
  living in an enum variant. Plan 02's renderer should branch on `spec.link.is_some()`
  and ignore fg/bg/style when emitting a link spec — matching how nvim itself ignores
  the other fields when `link` is set in the table arg.

## Self-Check: PASSED

Files created:
- `src/design/nvim_highlights.rs` — FOUND
- `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-01-SUMMARY.md` — being written now

Commits in branch history:
- `1bce769` test(17-01) — FOUND
- `bc22ffa` feat(17-01) Palette::resolve — FOUND
- `d8956e4` test(17-01) scaffold — FOUND
- `ee0b1d1` feat(17-01) populate HIGHLIGHT_GROUPS — FOUND

Lib tests: 586 / 586 green; clippy + fmt clean.
