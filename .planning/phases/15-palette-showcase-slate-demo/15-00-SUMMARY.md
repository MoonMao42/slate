---
phase: 15-palette-showcase-slate-demo
plan: 00
subsystem: infra
tags: [slate, rust, cli, demo, scaffolding, semantic-color, file-type-classification]

# Dependency graph
requires:
  - phase: 14
    provides: v2.1 shared-core Palette / SemanticColor / ThemeRegistry infrastructure
provides:
  - "14 new SemanticColor variants (6 syntax highlighting + 8 file-type) in src/cli/picker/preview_panel.rs"
  - "Palette::resolve extended with 14 placeholder arms (self.foreground.clone()) so exhaustive match compiles"
  - "src/design/file_type_colors.rs stub: FileKind enum + classify() + extension_map() (all compile-only stubs)"
  - "src/cli/demo.rs stub: handle(), render_to_string(palette), emit_demo_hint_once(auto, quiet), suppress_demo_hint_for_this_process() with HINT_EMITTED AtomicBool gate primed"
  - "10 #[ignore] integration-test stubs in tests/integration_tests.rs (demo_* and demo_hint_*) with exact names VALIDATION.md expects"
  - "bench_demo_render scaffold in benches/performance.rs wired into criterion_group!"
affects:
  - 15-01 (Palette::resolve real slot assignments)
  - 15-02 (file_type_colors::classify + extension_map real bodies)
  - 15-03 (demo::render_to_string + demo::handle full renderer)
  - 15-04 (hint emitter wiring from setup.rs / theme.rs)
  - 15-05 (integration test bodies)
  - 16 (LS_COLORS / EZA_COLORS consumes file_type_colors)
  - 17 (future editor adapter reuses syntax SemanticColor variants)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Exhaustive SemanticColor match: Rust forces every variant to be resolved by Palette::resolve — compile-time safety net for downstream waves."
    - "Wave-0 scaffolding: stubbed enums + stubbed fn bodies (self.foreground.clone() placeholders; unimplemented!()) so parallel waves compile against stable contracts."
    - "Process-local AtomicBool dedup primitive primed in demo.rs for Plan 03 hint emitter."

key-files:
  created:
    - "src/cli/demo.rs — slate demo command module (stubs for handle / render_to_string / emit_demo_hint_once / suppress_demo_hint_for_this_process)"
    - "src/design/file_type_colors.rs — shared classifier module (FileKind + classify + extension_map stubs)"
  modified:
    - "src/cli/picker/preview_panel.rs — +14 SemanticColor variants"
    - "src/theme/mod.rs — +14 placeholder resolve arms"
    - "src/design/mod.rs — pub mod file_type_colors"
    - "src/cli/mod.rs — pub mod demo"
    - "tests/integration_tests.rs — +10 #[ignore] demo_* test stubs"
    - "benches/performance.rs — bench_demo_render scaffold + wired criterion_group"

key-decisions:
  - "Kept placeholder arms at self.foreground.clone() instead of guessing palette slots — keeps the substitution surface for Plan 01 explicit and greps cleanly."
  - "Placed demo_* integration test stubs at top level (not inside mod tests block) so the verification grep '^fn demo_' counts exactly 10, per the plan's explicit verification contract."
  - "Omitted `use super::*;` from demo.rs inline #[cfg(test)] tests because the stub test bodies are empty — adding an unused import would trip clippy -D warnings."

patterns-established:
  - "Wave 0 scaffolding as a first-class plan type: stubs compile, downstream waves fill bodies against stable types."
  - "suppress_demo_hint_for_this_process() exposed as a public API up-front so set.rs can call it without a cross-plan coordination patch in Plan 04."

requirements-completed: []  # Plan 15-00 is pure scaffolding; DEMO-01 and DEMO-02 close in Plans 15-03..05, not here.

# Metrics
duration: ~10min
completed: 2026-04-18
---

# Phase 15 Plan 00: Wave 0 Scaffolding Summary

**Stubbed 14 SemanticColor variants + file_type_colors module + slate demo module + 10 integration-test stubs + criterion bench scaffold — all compiling clean so Plans 15-01, 15-02, and 15-03 can proceed in parallel against stable contracts.**

## Performance

- **Duration:** ~10 minutes
- **Started:** 2026-04-18T00:49:00Z
- **Completed:** 2026-04-18T00:49:24Z
- **Tasks:** 2
- **Files created:** 2 (src/cli/demo.rs, src/design/file_type_colors.rs)
- **Files modified:** 6

## Accomplishments

- Extended `SemanticColor` enum with 14 new variants in two grouped sections (6 syntax: `Keyword`, `String`, `Comment`, `Function`, `Number`, `Type`; 8 file-type: `FileArchive`, `FileImage`, `FileMedia`, `FileAudio`, `FileCode`, `FileDocs`, `FileConfig`, `FileHidden`).
- Extended `Palette::resolve` with 14 placeholder arms (every arm returns `self.foreground.clone()`) so the exhaustive match compiles; real palette-slot assignments ship in Plan 01.
- Created `src/design/file_type_colors.rs` exposing `FileKind { Regular, Directory, Symlink, Executable }` plus stubbed `classify()` (returns `SemanticColor::FileDocs`) and `extension_map()` (returns `&[]`).
- Created `src/cli/demo.rs` with four public stubs: `handle()` (panics via `unimplemented!`), `render_to_string(&Palette) -> String` (returns empty string), `emit_demo_hint_once(auto, quiet)` (no-op), and `suppress_demo_hint_for_this_process()` (sets the `HINT_EMITTED` AtomicBool to true). Two `#[ignore]`'d inline tests.
- Wired both new modules into their parent `mod.rs` files (alphabetical order: `file_type_colors` between `colors` and `presets`; `demo` between `config` and `failure_handler`).
- Appended 10 `#[ignore]`'d top-level integration test stubs to `tests/integration_tests.rs` with the exact names VALIDATION.md expects (`demo_renders_all_blocks`, `demo_size_gate_rejects`, `demo_size_gate_accepts_minimum`, `demo_touches_all_ansi_slots`, `demo_hint_setup_emits_once`, `demo_hint_theme_guards`, `demo_hint_theme_quiet_suppresses`, `demo_hint_theme_auto_suppresses`, `demo_hint_no_stack_with_set_deprecation`, `demo_sub_second_budget`).
- Added `bench_demo_render` to `benches/performance.rs`, imported the new `slate_cli::cli::demo` path, and registered it in `criterion_group!(benches, bench_apply_theme, bench_demo_render)`.

## Task Commits

Each task was committed atomically (no-verify, per worktree parallel-execution convention):

1. **Task 15-00-01: Extend SemanticColor enum + stub Palette::resolve arms + create design/file_type_colors stub** — `55cf60b` (feat)
2. **Task 15-00-02: Create src/cli/demo.rs stubs + wire cli/mod.rs + add integration test stubs + bench scaffold** — `13dc2aa` (feat)

## Files Created/Modified

- `src/cli/picker/preview_panel.rs` — added 14 new SemanticColor variants in two grouped sections
- `src/theme/mod.rs` — added 14 placeholder arms to Palette::resolve
- `src/design/mod.rs` — added `pub mod file_type_colors;` (alphabetical)
- `src/design/file_type_colors.rs` — **created**; FileKind enum + stubbed classify() + extension_map()
- `src/cli/mod.rs` — added `pub mod demo;` (alphabetical)
- `src/cli/demo.rs` — **created**; handle, render_to_string, emit_demo_hint_once, suppress_demo_hint_for_this_process stubs + HINT_EMITTED AtomicBool
- `tests/integration_tests.rs` — +10 `#[ignore]` top-level `demo_*` test stubs
- `benches/performance.rs` — added `use slate_cli::cli::demo;`, `bench_demo_render`, and wired into criterion_group

## Decisions Made

- **Placeholder arms stay at `self.foreground.clone()`** — downstream grep `SemanticColor::<variant> => self.foreground.clone()` cleanly identifies every slot Plan 01 must replace. Choosing a guessed palette slot now would create spurious "already done" confusion in Plan 01.
- **`demo_*` stubs at top level of `integration_tests.rs`** — the plan's verification command `grep -c '^fn demo_' tests/integration_tests.rs` requires top-level placement (submodule functions are indented). Inserted right before the first submodule (`tool_selection_tests`) so they stay with the other top-level tests.
- **Dropped `use super::*;` from `demo.rs` inline test mod** — the stub test bodies are empty, so the import would be unused and tripped `cargo clippy -- -D warnings`. Added back in Plan 03 when real tests reference the helpers.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Warning-as-error] Removed unused `use super::*;` inside `src/cli/demo.rs::tests`**
- **Found during:** Task 15-00-02 (clippy gate)
- **Issue:** The PLAN.md task action template included `use super::*;` inside the inline `#[cfg(test)] mod tests`, but the two stub test bodies are empty placeholders that don't reference any super item. `cargo clippy -- -D warnings` flags this as `unused_imports`, which the user's code-quality gate requires to be fixed (not suppressed).
- **Fix:** Omitted the `use super::*;` line. Plan 03 (Wave 2) will re-add it alongside real test bodies that reference `render_to_string` / `emit_demo_hint_once`.
- **Files modified:** src/cli/demo.rs
- **Verification:** `cargo clippy --all-targets -- -D warnings` exits 0.
- **Committed in:** 13dc2aa (Task 15-00-02 commit)

**2. [Rule 3 - Blocking fmt] Auto-formatted two `demo_hint_*` test stubs where chain exceeded column width**
- **Found during:** Task 15-00-02 (fmt gate)
- **Issue:** rustfmt wanted to line-break `slate_cmd_isolated(&tempdir).args([...]).output()` in `demo_hint_setup_emits_once` and `demo_hint_theme_auto_suppresses` because the chain exceeded default width.
- **Fix:** Ran `cargo fmt`; chain-broken style applied consistently with the other demo_hint_* stubs.
- **Files modified:** tests/integration_tests.rs
- **Verification:** `cargo fmt --check` exits 0; `grep -c '^fn demo_' tests/integration_tests.rs` still prints 10.
- **Committed in:** 13dc2aa (Task 15-00-02 commit)

---

**Total deviations:** 2 auto-fixed (1 clippy cleanup, 1 fmt normalization)
**Impact on plan:** Both are micro-adjustments to the exact text of the plan's code blocks — they do not change any task's shape, file list, or exported API surface. No scope creep.

## Issues Encountered

None — both tasks executed linearly with no blockers.

## User Setup Required

None — pure scaffolding; no external services, environment variables, or manual steps.

## Next Phase Readiness

Plans 15-01 / 15-02 / 15-03 can now proceed in parallel (Wave 1 fan-out) against these stable contracts:

- **Plan 15-01 (Palette::resolve real slots):** greps `SemanticColor::<Variant> => self.foreground.clone()` in `src/theme/mod.rs` and replaces each placeholder with the real palette slot from RESEARCH.md §Standard Stack.
- **Plan 15-02 (file_type_colors bodies):** replaces stubbed `classify()` + `extension_map()` in `src/design/file_type_colors.rs` with the real mapping table.
- **Plan 15-03 (demo renderer):** fills `render_to_string()` + `handle()` + `emit_demo_hint_once()` bodies in `src/cli/demo.rs`; the bench scaffold will start producing measurements as soon as `render_to_string` becomes non-empty.

All gates passing:
- `cargo build` — 0 errors
- `cargo build --benches` — 0 errors
- `cargo test --test integration_tests --no-run` — 10 demo_* stubs compile
- `cargo test --lib` — 431 passed, 2 ignored (the new demo.rs inline stubs), 0 failed
- `cargo clippy --all-targets -- -D warnings` — 0 warnings
- `cargo fmt --check` — 0 diffs

## Self-Check

- [x] `src/cli/demo.rs` exists → FOUND
- [x] `src/design/file_type_colors.rs` exists → FOUND
- [x] `pub mod demo;` in `src/cli/mod.rs` → FOUND
- [x] `pub mod file_type_colors;` in `src/design/mod.rs` → FOUND
- [x] 14 new SemanticColor variants in `src/cli/picker/preview_panel.rs` → FOUND
- [x] 14 placeholder arms in `src/theme/mod.rs` Palette::resolve → FOUND
- [x] 10 top-level `fn demo_*` in `tests/integration_tests.rs` → FOUND (count: 10)
- [x] `bench_demo_render` + updated `criterion_group` in `benches/performance.rs` → FOUND
- [x] Task 15-00-01 commit `55cf60b` → FOUND in git log
- [x] Task 15-00-02 commit `13dc2aa` → FOUND in git log

## Self-Check: PASSED

---
*Phase: 15-palette-showcase-slate-demo*
*Completed: 2026-04-18*
