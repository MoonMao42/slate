---
phase: 17
plan: 02
subsystem: adapter-nvim
tags: [editor-adapter, nvim, render, tdd, snapshot, plan-02]
dependency_graph:
  requires:
    - "src/design/nvim_highlights.rs::HIGHLIGHT_GROUPS (Plan 01)"
    - "src/design/nvim_highlights.rs::HighlightSpec, Style (Plan 01)"
    - "src/theme/Palette::resolve (Plan 01 cascade)"
    - "src/adapter/palette_renderer::PaletteRenderer::hex_to_rgb"
  provides:
    - "src/adapter/nvim.rs::render_colorscheme(palette, variant_id) -> String"
    - "src/adapter/nvim.rs::render_shim(variant_id) -> String"
    - "src/adapter/nvim.rs::write_lua_entry (private helper)"
    - "src/adapter/nvim.rs::resolve_with_fallback (private helper)"
    - "snapshot gate: src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap"
  affects:
    - "Cargo.toml [dev-dependencies] — adds insta = \"1\""
tech-stack:
  added:
    - "insta 1 (dev-dep, snapshot testing)"
  patterns:
    - "pure-render-function (mirrors src/adapter/ls_colors.rs shape)"
    - "splice-target output (leading Lua comment + bare table literal)"
    - "invalid-hex fallback to nvim 'NONE' sentinel (no panic)"
    - "insta snapshot for canonical theme (catppuccin-mocha)"
key-files:
  created:
    - "src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap"
    - ".planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-02-SUMMARY.md"
  modified:
    - "src/adapter/nvim.rs (14 LOC skeleton → 432 LOC pure-render surface + 11 unit tests)"
    - "Cargo.toml (adds insta = \"1\" alphabetically in [dev-dependencies])"
    - "Cargo.lock (insta + transitive deps)"
decisions:
  - "Treesitter / LSP group names that start with '@' are emitted with bracketed-string-key syntax `[\"@..\"] = { .. }` because leading `@` + embedded dots break Lua's plain-identifier form. Ordinary names use dot-style `Name = { .. }`."
  - "Empty `HighlightSpec` (no fg / bg, `Style::None`) emits `{ }` rather than bare empty table; nvim accepts this as a no-op per `nvim_set_hl` contract."
  - "Invalid-hex test corrupts `Palette::background` directly (the field is `pub` and drives the `Background` SemanticColor role, which is bg of `Normal`). This guarantees the bad hex flows through at least one HighlightSpec and surfaces the `'NONE'` sentinel without relying on `#[ignore]`."
  - "Snapshot file lives at `src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap` — insta's default layout for tests inside `src/adapter/nvim.rs`. The plan's `<read_first>` mention of `tests/snapshots/` in the behavior section was superseded by the explicit acceptance-criterion path (`src/adapter/snapshots/`)."
  - "Committed the accepted snapshot directly (rather than going through `cargo insta review`) because `cargo-insta` is not installed globally on this runner; the promotion is mechanical (rename `.snap.new` → `.snap`)."
metrics:
  duration_seconds: 353
  duration_human: "5m 53s"
  completed_at: "2026-04-18T17:19:02Z"
  tasks_completed: 1
  tdd_phases: ["RED", "GREEN"]
  files_created: 2
  files_modified: 3
  lib_tests_before: 586
  lib_tests_after: 597
  new_tests_added: 11
  nvim_rs_loc_before: 14
  nvim_rs_loc_after: 432
---

# Phase 17 Plan 02: Render layer — render_colorscheme + render_shim Summary

Landed the two pure-render surfaces that anchor the rest of Phase 17:
`render_colorscheme(palette, variant_id)` emits ONE variant's
highlight-group sub-table as a Lua comment + bare table literal
(the splice target Plan 03's loader consumes); `render_shim(variant_id)`
emits the 3-line shim that lives under `~/.config/nvim/colors/`. No I/O,
no adapter trait, no loader template — those come in Plans 03 and 05.
`src/adapter/nvim.rs` went from the 14-LOC Plan 00 skeleton to 432 LOC
of deterministic pure functions plus an 11-test `#[cfg(test)] mod tests`
block that runs in sub-second time.

## Output shape (locked, splice-target contract)

`render_colorscheme` emits:

```text
-- slate-managed palette for catppuccin-mocha
{
  Normal = { fg = '#cdd6f4', bg = '#1e1e2e' },
  NormalFloat = { fg = '#cdd6f4', bg = '#313244' },
  FloatBorder = { fg = '#585b70' },
  lCursor = { link = 'Cursor' },
  ["@comment"] = { fg = '#585b70', italic = true },
  ["@lsp.type.parameter"] = { fg = '#eebebe' },
  ...
}
```

This is **NOT** a standalone Lua module. Bare `{ ... }` at file-statement
level is a Lua parse error without `return` or assignment. It is a
**splice target** for Plan 03's loader template:

```lua
-- inside lua/slate/init.lua
local PALETTES = {
  ['catppuccin-mocha'] = <render_colorscheme output goes here>,
  ['catppuccin-frappe'] = <...>,
  -- 16 more
}
```

That is why `render_colorscheme_output_is_splice_target_shape` rejects
both `return {` and `local t =` wrapping as hard errors — wrapping would
break Plan 03's splice.

### Treesitter / LSP key quoting

Highlight-group names that start with `@` (treesitter captures, LSP
semantic tokens like `@lsp.type.parameter`) are emitted with Lua's
bracketed-string-key syntax, `[\"@name\"] = { .. }`, because the `@`
prefix plus embedded dots breaks Lua's plain-identifier form. Normal
names (`Normal`, `StatusLine`, `DiffAdd`) use the cleaner `Name = { .. }`
form. The choice is deterministic (keyed off `name.starts_with('@')`)
and captured in both the snapshot and
`render_colorscheme_emits_at_least_one_entry_per_highlight_group`.

### Invalid-hex degradation

`resolve_with_fallback` calls `PaletteRenderer::hex_to_rgb` on each hex
the palette resolves and emits the Lua sentinel `'NONE'` (which
`nvim_set_hl` treats as "unset") on parse failure. This is proved by
`invalid_hex_degrades_to_none_not_panic`, which corrupts
`Palette::background` to `"#notahex"` and asserts both no-panic AND the
presence of `bg = 'NONE'` in the output (guaranteed because `Normal.bg
= Background`).

## Why Plan 07 does NOT include a direct `luafile render_colorscheme(...)` test

Per the plan's own footnote and confirmed by implementation:
`render_colorscheme` emits `-- comment\n{ ... }` — a **bare table
literal** at file-statement level, which is a Lua parse error without
`return` or assignment. A hypothetical `nvim --headless -c 'luafile %'`
test on raw `render_colorscheme` output would fail **by construction**,
not because the renderer is wrong.

Plan 07's syntax gate instead parses through the loader:
`loader_lua_parses_via_luafile` sources the complete
`~/.config/nvim/lua/slate/init.lua` (Plan 03's output), which contains
all 18 variants spliced into the loader's `PALETTES = { ... }` block.
That is valid Lua, and any malformed sub-table rendered by
`render_colorscheme` would surface there as a parse error covering the
same ground a direct `luafile` would — without the bare-table false
positive. The Plan 02 coverage contract is accordingly:

1. `render_colorscheme_output_is_splice_target_shape` guards the
   output shape statically (string-level assertions on the leading
   `-- slate-managed palette for …\n{`, trailing `}`, absence of
   `return {` / `local t =` wrapping).
2. `render_colorscheme_emits_at_least_one_entry_per_highlight_group`
   guards per-entry coverage against regressions where the renderer
   silently skips groups.
3. `insta_snapshot_catppuccin_mocha` locks the full byte-for-byte
   output for one canonical theme so renderer drift surfaces in review.
4. `render_colorscheme_smoke_all_variants_size_bounded` cross-checks
   all 18 variants against a 5 KB–80 KB size envelope.
5. Plan 07's `loader_lua_parses_via_luafile` closes the syntax loop for
   all 18 spliced sub-tables together.

## What Shipped

### `src/adapter/nvim.rs` — 14 LOC → 432 LOC

- **Public surface**
  - `render_colorscheme(&Palette, &str) -> String` — leading
    `-- slate-managed palette for <id>\n` + bare `{ … }` table; LF only;
    deterministic; 5 KB–80 KB per variant across all 18 themes; invalid
    hex degrades to `'NONE'` via `resolve_with_fallback`.
  - `render_shim(&str) -> String` — exact 3-line shim
    (`-- slate-managed` comment + `vim.g.colors_name = 'slate-<id>'`
    + `require('slate').load('<id>')`); one `require('slate').load(...)`
    call per output.
- **Private helpers**
  - `write_lua_entry(&mut String, &str, &HighlightSpec, &Palette)` —
    single-entry formatter; branches on `@`-prefix for keying and
    `spec.link.is_some()` for link-style emission.
  - `resolve_with_fallback(&Palette, SemanticColor) -> String` — hex
    validator + `'NONE'` degrade.
- **Module doc** expanded from the Plan 00 five-line wave outline to a
  wave summary + the load-bearing splice-target invariant note.

### `Cargo.toml` + `Cargo.lock`

- Adds `insta = "1"` to `[dev-dependencies]` alphabetically (between
  `criterion` and `notify`). No feature flags; the default set is
  enough for `assert_snapshot!`.

### `src/adapter/snapshots/…nvim_render_colorscheme_catppuccin_mocha.snap`

- 278-line YAML-framed snapshot captures the full 270-entry output for
  catppuccin-mocha. Review once, diff-flagged thereafter.

## Per-Task Commits

| Task | Step  | Commit    | Description                                                                       |
| ---- | ----- | --------- | --------------------------------------------------------------------------------- |
| 1    | RED   | `94c3d18` | `test(17-02): add failing tests for render_colorscheme + render_shim`             |
| 1    | GREEN | `29d0929` | `feat(17-02): implement render_colorscheme + render_shim for Plan 02`             |

REFACTOR phase skipped: the GREEN implementation was already idiomatic
(static-slice iteration, `const fn` helpers from Plan 01, bounded
`String::with_capacity` allocations). No behaviour-preserving cleanup
was worth a third commit.

## Tests Added

| Test                                                              | Asserts                                                                            |
| ----------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `render_shim_matches_exact_shape`                                 | Byte-exact 3-line shim for `catppuccin-mocha`                                      |
| `render_shim_contains_single_require_slate_load_call_for_each_id` | Shim has exactly one `require('slate').load('<id>')` per variant (3 sampled)        |
| `render_colorscheme_is_deterministic`                             | Two calls on the same (palette, id) return byte-identical strings                  |
| `render_colorscheme_has_lf_line_endings_only`                     | No `\r` anywhere in the output                                                     |
| `render_colorscheme_output_is_splice_target_shape`                | Leading comment + `{`, trailing `}`, no `return {` / `local t =` wrapping          |
| `render_colorscheme_contains_variant_marker_comment`              | Output begins with `-- slate-managed palette for catppuccin-mocha\n`               |
| `render_colorscheme_smoke_all_variants_size_bounded`              | 5 KB ≤ len ≤ 80 KB for all 18 themes                                               |
| `render_includes_treesitter_and_lsp_keys`                         | Output contains `[\"@comment\"]`, `[\"@function\"]`, `[\"@lsp.type.parameter\"]`, `DiagnosticError`, `DiffAdd` |
| `render_colorscheme_emits_at_least_one_entry_per_highlight_group` | Every name in `HIGHLIGHT_GROUPS` appears in the output with correct key syntax     |
| `invalid_hex_degrades_to_none_not_panic`                          | Corrupting `Palette::background` surfaces `bg = 'NONE'` in the output              |
| `insta_snapshot_catppuccin_mocha`                                 | Full-output snapshot locked against drift                                          |

11 tests total, sub-second run time, all green under
`cargo test --lib adapter::nvim::tests`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] clippy `manual_split_once` fired on `out.splitn(2, '\n').nth(1).unwrap_or("")`**

- **Found during:** RED-phase gate (`cargo clippy --all-targets --all-features -- -D warnings`)
- **Issue:** Clippy's `manual-split-once` lint (default-warn, promoted
  to error by `-D warnings`) flagged the `splitn(2, '\n').nth(1)` idiom
  as a manual re-implementation of `split_once`.
- **Fix:** Replaced with `out.split_once('\n').map(|x| x.1).unwrap_or("")`
  — same semantics, single allocation-free method call.
- **Files modified:** `src/adapter/nvim.rs` (test helper line 289)
- **Commit:** `94c3d18` (caught in RED phase)

### Plan Inconsistencies Noted (no code change)

- **Snapshot path wording.** The plan's behavior section mentions
  `tests/snapshots/nvim__render_colorscheme_catppuccin_mocha.snap` once
  while the acceptance criteria mandate `src/adapter/snapshots/` (the
  insta default layout for tests inside `src/adapter/nvim.rs`). The
  acceptance criteria were followed verbatim; the earlier wording was
  clearly a draft artifact superseded by the acceptance contract and
  `tests/snapshots/.gitkeep` was not populated (it is Plan 00's landing
  pad for future tests, not owned by this plan).
- **`cargo insta` invocation.** The plan mentions
  `cargo insta test --accept` / `cargo insta review`, but
  `cargo-insta` is not installed on this runner. The mechanical
  equivalent — renaming `slate_cli__adapter__nvim__tests__…snap.new`
  to `…snap` once the output was visually confirmed — was applied
  instead. Subsequent `cargo test` runs validate against the locked
  snapshot, so the drift-detection contract holds.

### Auth Gates

None — pure-Rust implementation, no external auth.

### Out of Scope (deferred for later plans)

- `NvimAdapter` struct + `ToolAdapter` impl → Plan 05 (Wave 5).
- `render_loader` + `write_state_file` → Plan 03 (Wave 3).
- Plugin highlight groups + `lualine_theme` → Plan 04 (Wave 4).

## Verification

| Gate                                                              | Result                      |
| ----------------------------------------------------------------- | --------------------------- |
| `cargo test --lib adapter::nvim::tests`                           | 11 / 11 pass                |
| `cargo test --lib`                                                | 597 / 597 pass (+11)        |
| `cargo test --all`                                                | all suites green            |
| `cargo fmt --all -- --check`                                      | no diff                     |
| `cargo clippy --all-targets --all-features -- -D warnings`        | 0 warnings                  |
| `wc -l src/adapter/nvim.rs`                                       | 432 (≥ 200 floor)           |
| `grep -c "pub fn render_colorscheme" src/adapter/nvim.rs`         | 1                           |
| `grep -c "pub fn render_shim" src/adapter/nvim.rs`                | 1                           |
| Snapshot file present                                             | src/adapter/snapshots/…snap |

## Self-Check: PASSED

**Created files exist:**
- `src/adapter/snapshots/slate_cli__adapter__nvim__tests__nvim_render_colorscheme_catppuccin_mocha.snap` — FOUND
- `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-02-SUMMARY.md` — being written now

**Modified files reflect changes:**
- `src/adapter/nvim.rs` — 432 LOC, `pub fn render_colorscheme` and `pub fn render_shim` present
- `Cargo.toml` — `insta = "1"` present in `[dev-dependencies]`
- `Cargo.lock` — insta + transitive deps locked

**Commits on branch:**
- `94c3d18` — `test(17-02): add failing tests for render_colorscheme + render_shim` — FOUND
- `29d0929` — `feat(17-02): implement render_colorscheme + render_shim for Plan 02` — FOUND

Both hashes verified via `git log --oneline 21bdbab..HEAD`. All gates
green; Plan 02 is complete.
