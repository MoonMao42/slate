---
phase: 19
plan: "01"
subsystem: cli-picker-preview
tags: [picker, demo, deletion, scaffolding, wave-0, housekeeping]
dependency_graph:
  requires: [Phase 18 brand Roles API + BrandEvent seam (shipped 2026-04-20)]
  provides:
    - src/cli/picker/preview/mod.rs (sub-module root)
    - src/cli/picker/preview/blocks.rs (empty skeleton — Plan 19-02 fills)
    - src/cli/picker/preview/compose.rs (empty skeleton — Plan 19-04 fills)
    - src/cli/picker/preview/starship_fork.rs (empty skeleton — Plan 19-06 fills)
    - src/cli/picker/rollback_guard.rs (empty skeleton — Plan 19-03 fills)
    - Phase-19 DEMO-03 retirement invariant locked in tests/theme_tests.rs
  affects:
    - src/main.rs (Commands::Demo variant + match arm deleted)
    - src/cli/demo.rs (stripped to 4-block renderer + 4 pure tests; migration source only)
    - src/cli/setup.rs (emit_demo_hint_once call site removed)
    - src/cli/theme.rs (emit_demo_hint_once call site removed)
    - src/cli/set.rs (suppress_demo_hint_for_this_process call removed)
    - src/brand/language.rs (DEMO_HINT + demo_size_error + 2 tests removed)
    - src/cli/new_shell_reminder.rs (docstring rewritten — no longer mirrors retired latch)
    - tests/integration_tests.rs (9 demo/hint tests removed; 1 retirement CLI smoke added)
    - benches/performance.rs (demo import + bench_demo_render removed)
    - .planning/REQUIREMENTS.md (DEMO-01/02 Superseded, DEMO-03 rewritten)
tech_stack:
  added: []
  patterns:
    - "destructive-sweep-plus-scaffold pattern (delete CLI surface + hint + tests in one wave; create empty sub-module with #[cfg(test)] mod tests hooks for subsequent waves)"
    - "retirement-invariant test pattern (filesystem walker + substring assertions — locks D-05/D-06 deletion at CI level)"
key_files:
  created:
    - src/cli/picker/preview/mod.rs
    - src/cli/picker/preview/blocks.rs
    - src/cli/picker/preview/compose.rs
    - src/cli/picker/preview/starship_fork.rs
    - src/cli/picker/rollback_guard.rs
    - .planning/REQUIREMENTS.md (force-added; pre-existed in main repo but .planning/ is .gitignored in worktrees per feedback_planning_untrack)
  modified:
    - src/main.rs
    - src/cli/picker/mod.rs
    - src/cli/demo.rs
    - src/cli/setup.rs
    - src/cli/theme.rs
    - src/cli/set.rs
    - src/brand/language.rs
    - src/cli/new_shell_reminder.rs
    - tests/theme_tests.rs
    - tests/integration_tests.rs
    - benches/performance.rs
decisions:
  - "Merge Task 19-01-01 (K/L: delete DEMO_HINT + demo_size_error) with Task 19-01-02 (delete Commands::Demo + handle + hint symbols) into one atomic commit — separating them leaves src/cli/demo.rs::handle() referencing deleted Language symbols mid-commit, failing the autonomous cargo-build gate (Rule 3: auto-fix blocking issues)."
  - "Keep 4-block renderer + TREE const + fg/span/RESET helpers + 4 pure-render tests in src/cli/demo.rs — Plan 19-02 Wave 1 migrates them wholesale into src/cli/picker/preview/blocks.rs."
  - "Replace 9-test Phase 15 demo/hint integration block in tests/integration_tests.rs with one Phase 19 CLI smoke (slate_demo_subcommand_is_retired_phase_19) asserting clap rejects 'slate demo' post-retirement."
  - "Omit ROADMAP.md Phase 15 footnote (Plan Task 19-01-03 section C) — worktree prompt explicitly instructs 'Do NOT modify ROADMAP.md'; orchestrator owns that update centrally after wave merge."
metrics:
  duration: "~14min (01:28–01:41 UTC 2026-04-20)"
  tasks_completed: 3
  files_modified: 11
  files_created: 6
  commits: 2
  completed_date: "2026-04-20"
---

# Phase 19 Plan 01: Wave 0 destructive clean-up + scaffolding Summary

Retired the standalone `slate demo` CLI command + DEMO-02 hint infrastructure per D-05 / D-06, scaffolded 5 empty sub-module files (`src/cli/picker/preview/{mod,blocks,compose,starship_fork}.rs` + `rollback_guard.rs`) with `#[cfg(test)] mod tests` hooks for Waves 1-3 to fill, and locked the retirement at CI level via a `tests/theme_tests.rs` invariant that recursively scans `src/**/*.rs` for accidental re-introduction of the removed symbols.

## Commits

| Commit | Subject | Tasks |
| ------ | ------- | ----- |
| `682a21e` | refactor(19-01): retire slate demo command + DEMO-02 hint, scaffold picker preview sub-module | Tasks 19-01-01 + 19-01-02 (merged atomically — see Deviations) |
| `20bae95` | test(19-01): lock slate demo retirement + refresh REQUIREMENTS DEMO-01/02/03 | Task 19-01-03 |

## What Shipped

### 5 New Skeleton Files (ready for Waves 1-3)

- **`src/cli/picker/preview/mod.rs`** — re-exports `pub mod blocks; pub(super) mod compose; pub(super) mod starship_fork;`. Module docstring declares the SWATCH-RENDERER allowlist scope.
- **`src/cli/picker/preview/blocks.rs`** — empty skeleton with `#[cfg(test)] mod tests` hook. Module docstring carries the SWATCH-RENDERER allowlist marker + pointer to Plan 19-02 migration.
- **`src/cli/picker/preview/compose.rs`** — empty skeleton for responsive fold composer. Plan 19-04 fills.
- **`src/cli/picker/preview/starship_fork.rs`** — empty skeleton for D-04 Hybrid starship fork. Module docstring carries the V12 security note (per-subprocess `.env("STARSHIP_CONFIG", ...)`, NOT `std::env::set_var`).
- **`src/cli/picker/rollback_guard.rs`** — empty skeleton for RollbackGuard Drop + panic hook. Module docstring records RESEARCH Pitfall 1 (Cargo.toml:67 `panic = "abort"` means Drop does NOT run on panic in release builds; `std::panic::set_hook` companion is mandatory).

### Deleted Symbols (exact inventory at delete-time)

From `src/cli/demo.rs`:
- `static HINT_EMITTED: AtomicBool` (L21)
- `pub fn handle() -> Result<()>` (L41-70; 30 lines including size-gate + theme load + flush)
- `pub fn emit_demo_hint_once(auto: bool, quiet: bool)` (L375-384)
- `fn demo_hint_line() -> String` (L388-392)
- `fn hint_text(roles: Option<&Roles<'_>>, text: &str) -> String` (L394-399)
- `pub fn suppress_demo_hint_for_this_process()` (L404-406)
- Tests: `emit_demo_hint_once_auto_is_silent`, `emit_demo_hint_once_quiet_is_silent`, `suppress_demo_hint_marks_emitted_flag`, `demo_hint_line_carries_path_role_bytes`, `demo_hint_falls_back_to_plain_when_roles_absent` (5 hint-related tests; kept 4 pure-render tests + `demo_render_preserves_many_palette_swatches`)
- Unused imports after `handle` deletion: `ConfigManager`, `SlateEnv`, `SlateError`, `DEFAULT_THEME_ID`, `ThemeRegistry`, `AtomicBool`, `Ordering`, `brand::render_context::RenderContext`, `brand::roles::Roles`, `brand::Language`, `io::Write`

From `src/brand/language.rs`:
- `pub const DEMO_HINT: &str` (L172-176; 5 lines incl. docstring)
- `pub fn demo_size_error(cols: u16, rows: u16) -> String` (L178-185; 8 lines incl. docstring + body)
- Test `test_demo_hint_format` (L338-354)
- Test `test_demo_size_error_mentions_required_and_actual` (L356-366)
- Adjusted `ls_capability_message` docstring to drop the "mirrors `demo_size_error`" reference (was pointing at a now-deleted symbol)

From `src/cli/setup.rs`:
- `crate::cli::demo::emit_demo_hint_once(false, false);` (L183) + 2-line comment above it

From `src/cli/theme.rs`:
- 6-line `// DEMO-02 (D-C1): hint only on explicit...` comment block + `crate::cli::demo::emit_demo_hint_once(false, quiet);` (L200-208)

From `src/cli/set.rs`:
- 5-line `// D-C3: suppress the Phase 15 DEMO-02 hint...` comment block + `crate::cli::demo::suppress_demo_hint_for_this_process();` (L14-19)

From `src/main.rs`:
- `/// Showcase your palette with a curated demo` + `Demo,` variant (L85-86)
- `Some(Commands::Demo) => cli::demo::handle(),` match arm (L159)

From `src/cli/new_shell_reminder.rs`:
- Updated docstring (L5-8) to stop pointing at the retired demo.rs latch.

From `tests/integration_tests.rs`:
- 9 tests deleted wholesale (L379-755; ~375 lines): `demo_renders_all_blocks`, `demo_size_gate_rejects`, `demo_size_gate_accepts_minimum`, `demo_touches_all_ansi_slots`, `demo_hint_setup_emits_once`, `demo_hint_theme_guards`, `demo_hint_theme_quiet_suppresses`, `demo_hint_theme_auto_suppresses`, `demo_hint_no_stack_with_set_deprecation`, `demo_sub_second_budget` — plus `strip_ansi_for_tests` helper (only consumed by the deleted block).
- Replaced with one 18-line Phase 19 CLI smoke `slate_demo_subcommand_is_retired_phase_19`.

From `benches/performance.rs`:
- `use slate_cli::cli::demo;` (L3)
- `fn bench_demo_render(c: &mut Criterion)` (L29-38) + its entry in the `criterion_group!` macro.

### REQUIREMENTS.md Rewrite (CONTEXT §canonical_refs)

- DEMO-01 / DEMO-02 flipped to `[x] (Superseded by Phase 19 / DEMO-03 on 2026-04-20)` with historical context ("shipped Phase 15 2026-04-18 and retired 2026-04-20").
- DEMO-03 body rewritten verbatim from CONTEXT — picker as unified entry, full-stack live preview, triple-guarded rollback, responsive fold, Hybrid starship, side-effect-free Tab, 4-block renderer relocated to `src/cli/picker/preview/blocks.rs`.
- Traceability table updated: DEMO-01/02 rows show `Phase 15 → Phase 19` with `Superseded by DEMO-03 (2026-04-20)` disposition. DEMO-03 flipped from `Pending` to `In progress`.

### CI Invariant Test

`tests/theme_tests.rs::slate_demo_surface_stays_retired_post_phase_19`:
- Recursively walks `src/**/*.rs` (skipping `target/`, `.git/`, and any dotfile directory).
- Asserts that none of `Commands::Demo`, `emit_demo_hint_once`, `suppress_demo_hint_for_this_process`, `Language::DEMO_HINT`, `pub const DEMO_HINT` reappear in the bundled source.
- Ships as a companion to `brand::migration::tests::no_raw_styling_ansi_anywhere_in_user_surfaces` (Phase 18 aggregate invariant).
- Added to the `tests/theme_tests.rs` suite (12 tests total, all green).

## Verification Results

| Gate | Result | Details |
| ---- | ------ | ------- |
| `cargo build --release` | ✅ GREEN | 19-49s incremental (cold 49s) |
| `cargo clippy --all-targets -- -D warnings` | ✅ GREEN | Zero warnings across src + tests + benches |
| `cargo test --lib` | ✅ GREEN | 768 passed / 0 failed / 0 ignored |
| `cargo test --test theme_tests` | ✅ GREEN | 12 passed (original 11 + new `slate_demo_surface_stays_retired_post_phase_19`) |
| `cargo test --test integration_tests` | ✅ GREEN | 67 passed (demo/hint block excised — was 76 before, difference = 9 tests deleted + 0 added because the new CLI smoke is `#[test]` not inside that count window; recount showed 67 post-deletion + 1 new = 67 confirms we dropped 10 and added 1 in that binary) |
| Grep: `emit_demo_hint_once` / `suppress_demo_hint_for_this_process` in src/cli/{setup,theme,set}.rs | ✅ CLEAN | Zero results |
| `Commands::Demo` reference in src/ | ✅ CLEAN | Zero results (locked by retirement test) |
| `cargo fmt --check` | ⚠️ PRE-EXISTING DRIFT | 4 diff locations: `src/brand/render_context.rs:81,118,133` + `src/brand/roles.rs:271`. Confirmed present on the base commit 437da1e (ran `cd /Users/maokaiyue/Projects/slate && cargo fmt --check` and reproduced identical diff). Out of scope per SCOPE BOUNDARY — not introduced by this plan. Logged below under Deferred Items. |

## Deviations from Plan

### Rule 3 — Auto-fix blocking issues (merged Task 19-01-01 + 19-01-02)

- **Found during:** Task 19-01-01 K/L step (delete `Language::DEMO_HINT` + `demo_size_error`).
- **Issue:** Plan Task 19-01-01 instructs deleting the Language symbols as step K/L, but Task 19-01-02 (delete demo.rs `handle` + hint symbols) is scheduled as a separate commit. `src/cli/demo.rs::handle()` directly consumes `Language::demo_size_error(cols, rows)` at L48, and `emit_demo_hint_once` / `demo_hint_line` consume `Language::DEMO_HINT`. Deleting the Language symbols while the demo.rs consumers are still present fails `cargo build --release` at the Task 19-01-01 verify gate.
- **Fix:** Merge the atomic unit. One commit (`682a21e`) performs both the Task 19-01-01 K/L deletion AND the Task 19-01-02 demo.rs + main.rs + integration tests + bench sweep. The 4-block renderer + `TREE` const + `fg` / `span` / `RESET` helpers + 4 pure-render tests are preserved as the Plan 19-02 migration source.
- **Files modified:** 10 source files + 2 build artifacts (see Commits table).
- **Commit:** `682a21e`.

### ROADMAP.md Phase 15 footnote intentionally omitted

- **Found during:** Task 19-01-03 section C.
- **Issue:** Plan Task 19-01-03 C instructs appending a "superseded by Phase 19" footnote under the Phase 15 ROADMAP.md bullet. Worktree executor prompt explicitly forbids modifying ROADMAP.md: "Do NOT modify STATE.md or ROADMAP.md — the orchestrator owns those writes after all worktree agents in the wave complete."
- **Resolution:** Deferred to orchestrator's post-merge central write. The Plan's footnote text is captured here for that downstream handoff:
  ```markdown
  - [x] **Phase 15: Palette Showcase — `slate demo`** — curated single-screen payoff render + contextual hint surfacing (completed 2026-04-18)
    *— CLI surface superseded by Phase 19 `slate theme` picker on 2026-04-20 per DEMO-03; the 4-block renderer continues to serve as a picker preview component under `src/cli/picker/preview/blocks.rs`.*
  ```

### `.planning/REQUIREMENTS.md` committed via `--force` add

- **Found during:** Task 19-01-03 A/B.
- **Issue:** `.planning/` is in `.gitignore:20` per user memory `feedback_planning_untrack` ("Run `git rm --cached -r .planning/` after each phase to undo executor force-adds"). Worktree executor prompt REQUIRES REQUIREMENTS.md to be committed: "in worktree mode the git_commit_metadata step ... commits SUMMARY.md and REQUIREMENTS.md only."
- **Resolution:** Force-added (`git add --force .planning/REQUIREMENTS.md`). Orchestrator's phase-close hook will re-run `git rm --cached -r .planning/` per the user's established convention.

## Known Stubs

- **`src/cli/picker/preview/blocks.rs`**: empty skeleton. Intentional — Plan 19-02 Wave 1 populates with the migrated 4-block renderer.
- **`src/cli/picker/preview/compose.rs`**: empty skeleton. Intentional — Plan 19-04 Wave 2 populates with `FoldTier` + `decide_fold_tier` + `compose_mini` / `compose_full`.
- **`src/cli/picker/preview/starship_fork.rs`**: empty skeleton. Intentional — Plan 19-06 Wave 3 populates with `fork_starship_prompt` + `StarshipForkError` enum.
- **`src/cli/picker/rollback_guard.rs`**: empty skeleton. Intentional — Plan 19-03 Wave 1 populates with `RollbackGuard: Drop` + `install_rollback_panic_hook`.

All four stubs reference their filling plans in module docstrings so no mystery pointers. None ship in user-facing paths until Wave 4 integration (Plan 19-08).

## Deferred Items

| Item | Reason | Owner |
| ---- | ------ | ----- |
| ROADMAP.md Phase 15 footnote | Worktree executor may not touch ROADMAP.md | Orchestrator post-merge |
| `cargo fmt --check` drift in `src/brand/{render_context,roles}.rs` | Pre-existing on base commit 437da1e; out of scope per SCOPE BOUNDARY | Whoever touches those files next (likely Phase 20 SFX work, since those files won't be edited in Phase 19 plans 02-08) |
| Sketch 004-picker-layout theme color reference (picker lacks in-alt-screen theme color reference) | Unrelated to deletion + scaffolding scope; tracked separately in memory `project_phase6_picker_ux_debt` — resolved downstream in Plan 19-05 render mode dispatch | Plan 19-05 Wave 2 |

## Next Up (Plan 19-02)

- Read `src/cli/demo.rs` as the migration source (`render_code_block` / `render_tree_block` / `render_git_log_block` / `render_progress_block` + `TREE` const + `fg` / `span` / `RESET` helpers + 4 pure-render tests).
- Copy verbatim into `src/cli/picker/preview/blocks.rs`.
- Add new `render_palette_swatch` fn (D-03 — double-row 16-cell + named labels).
- `git rm src/cli/demo.rs` + remove `pub mod demo;` from `src/cli/mod.rs`.
- `cargo build --release` + clippy + fmt + full test suite green at end.

## Self-Check: PASSED

- FOUND: `src/cli/picker/preview/mod.rs`
- FOUND: `src/cli/picker/preview/blocks.rs`
- FOUND: `src/cli/picker/preview/compose.rs`
- FOUND: `src/cli/picker/preview/starship_fork.rs`
- FOUND: `src/cli/picker/rollback_guard.rs`
- FOUND: `tests/theme_tests.rs::slate_demo_surface_stays_retired_post_phase_19`
- FOUND: `.planning/REQUIREMENTS.md` (DEMO-01/02 Superseded + DEMO-03 rewritten)
- FOUND: commit `682a21e` (Tasks 19-01-01 + 19-01-02)
- FOUND: commit `20bae95` (Task 19-01-03)
- CONFIRMED: zero `Commands::Demo` / `emit_demo_hint_once` / `suppress_demo_hint_for_this_process` / `Language::DEMO_HINT` / `pub const DEMO_HINT` references in `src/**/*.rs` (locked by the new invariant test)
- CONFIRMED: `cargo build --release` + `cargo clippy --all-targets -- -D warnings` + `cargo test --lib` + `cargo test --test theme_tests` + `cargo test --test integration_tests` all green
