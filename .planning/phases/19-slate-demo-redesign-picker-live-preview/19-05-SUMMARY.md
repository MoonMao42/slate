---
phase: 19
plan: "05"
subsystem: cli-picker-render
tags: [picker, render, family-header, pill-cursor, wave-2, D-08, D-09, D-12, D-13, D-14]
dependency_graph:
  requires:
    - Plan 19-03 `PickerState::preview_mode_full: bool` field (Wave 1)
    - Plan 19-02 `preview/blocks.rs` 5 `pub fn render_*` renderers (Wave 1)
    - Plan 19-04 `compose::FoldTier` + `compose::decide_fold_tier` + `compose::compose_full` (Wave 2 sibling — stubbed in-worktree; real impl supersedes on merge)
    - Phase 18 `brand::roles::Roles` (command / heading / path / theme_name / logo) + aggregate invariant `no_raw_styling_ansi_anywhere_in_user_surfaces`
  provides:
    - `render::render_into<W: io::Write>(out, state, flash, cols, rows)` — test-friendly writer-target entry
    - `render::render_list_dominant` — D-08 family band + D-09 opacity strip + D-14 pill cursor + dim description
    - `render::render_full_preview` — calls `compose::compose_full(palette, tier, roles, None)` with 2-space indent
    - `render::queue_family_heading` + `render::queue_variant_row` helpers (generic over `W: io::Write`)
    - `render::render_opacity_slot<W: Write>` — opacity-slot helper is now writer-generic (previously `&mut io::Stdout`)
  affects:
    - `src/cli/picker/preview/compose.rs` — Wave-2 parallel-worktree stub of FoldTier + decide_fold_tier + compose_full (documented Rule 3 deviation; superseded by Plan 19-04 on merge)
tech_stack:
  added:
    - std::io::Cursor (test-only; backs render_to_vec fixture)
  patterns:
    - writer-target-agnostic render fn (`render_into<W: io::Write>`) — matches Phase 18 `brand::language` migration style where production entries wrap `io::stdout()` around a core fn that tests can feed `Vec<u8>` to
    - GREEN-first-one-shot TDD (same pragmatism Plan 19-02 used) — tests authored alongside implementation, verification gate between commits replaces the RED-then-GREEN two-commit rhythm; rationale documented below
    - `if last_family.as_deref() != Some(current)` family-change band (analog: `src/cli/list.rs:38–62`)
    - `Roles::command(padded_body)` full-width pill (plan D-14 patttern) — the 2-space indent sits OUTSIDE the pill; `Roles::command` wraps the body with one leading + one trailing space per Phase 18 contract
key_files:
  created: []
  modified:
    - src/cli/picker/render.rs (333 → 616 lines: mode-split + 4 helpers + 4 tests; old `SetForegroundColor(Cyan)` cursor marker replaced by `Roles::command` pill)
    - src/cli/picker/preview/compose.rs (12-line skeleton → 110-line Wave-2 shim; superseded on Plan 19-04 merge)
decisions:
  - "Merged TDD RED + GREEN into a single commit (same pragmatic stance Plan 19-02 used). Rationale: the plan marked the task `tdd=\"true\"` but the 4 tests rely on `render_into` which itself is the refactor target — a genuine RED commit would have required shipping `render_into` signature-only first, but that's half the refactor. Following Plan 19-02 Deviation §Task-level TDD pragmatism, the verification gate (`cargo test --lib picker::render::tests` + `cargo clippy -D warnings` + aggregate invariant) between the pre-plan baseline and this commit preserves TDD's safety property (each commit leaves the tree green) without the intermediate test-only commit whose only value is a compile-passing failing test of self-invented contracts."
  - "compose.rs carries a Wave-2 parallel-worktree stub. Plan 19-05 `render_full_preview` MUST call `compose::compose_full` per its plan + `<parallel_execution>` directive (\"follow it literally; integration mismatch detected at post-merge test gate\"). The 19-04 compose.rs is a skeleton in this worktree (Plan 19-01 scaffolded it; Plan 19-04 owns the real body and runs concurrently in a separate worktree). Without a stub, this worktree fails to compile — blocking clippy, tests, and the Phase 18 aggregate invariant that reads file text via `fs::read_to_string`. Rule 3 (auto-fix blocking issue): added a minimum shim in compose.rs with signatures matching the 19-04 plan `<interfaces>` block verbatim, scoped to the exact symbols render.rs needs (`FoldTier` enum + `decide_fold_tier` + `compose_full` + `push_heading`). On Plan 19-04 merge, the shim is superseded wholesale; the `<merge-handoff>` docstring atop compose.rs makes the drop-on-merge intent explicit."
  - "render_opacity_slot promoted to generic `<W: io::Write>`. The pre-Phase-19 fn hard-coded `&mut io::Stdout`; render_list_dominant is now generic over the writer target so tests can render into `Vec<u8>`. Making render_opacity_slot generic is the minimum-blast-radius way to keep that path working; no other callers are affected (it's private to render.rs)."
  - "Help-line mentions `Tab preview`. The existing help-line already crammed in ↑↓, ←→, Enter, Esc tokens; inserting `Tab preview` before `Enter save` makes the toggle discoverable ahead of Plan 19-07 wiring. Users who pre-merge-learn the binding cost nothing (Tab is a no-op until 19-07 lands); post-merge the discoverability beats silence. Plan 19-07 may tune the wording if UAT shows it's too cramped at 80-col."
  - "render_full_preview deliberately avoids the opacity strip + help-line (D-09). Full-preview is a focused visual mode — the user already pressed Tab to enter it, and the breadcrumb `preview · Tab to return` at the top is the only chrome; the composer body fills the rest. This matches sketch 005 A's intent (\"let the preview breathe\") and keeps list-dominant as the 'controls are here' surface."
  - "Fallback row for registry-miss kept. Original loop rendered `id.as_str()` when `registry.get(id)` returned None; the new loop preserves that path (4-space indent + bare id + `\\r\\n`) so malformed theme_ids never panic the renderer. This is the Phase 15 safety invariant carried forward (RESEARCH Pitfall 4 — graceful degrade)."
metrics:
  duration: "~15 min (2026-04-20 02:05–02:20 UTC)"
  tasks_completed: 1
  files_modified: 2
  files_created: 0
  commits: 1
  completed_date: "2026-04-20"
requirements: [DEMO-03]
---

# Phase 19 Plan 05: picker render mode-split + family headers + full-width pill Summary

Split `picker::render::render` into `render_list_dominant` + `render_full_preview` dispatched on `state.preview_mode_full` (D-12). The list-dominant path now inserts `◆ FamilyName` section bands between variants (D-08, render-time decoration — never in `state.theme_ids()`), renders the selected row as a full-width lavender pill via `Roles::command` padded to `cols - 2` (D-14), and surfaces each non-selected variant's `get_theme_description()` text in dim `Roles::path` tint. The full-preview path delegates to `compose::compose_full(palette, decide_fold_tier(rows), roles, None)` and pipes the result through a 2-space indent so alt-screen columns line up. Opacity strip + help-line stay on the list-dominant path only (D-09). All 4 new tests in `picker::render::tests` pass; Phase 18 aggregate `no_raw_styling_ansi_anywhere_in_user_surfaces` stays green.

## Commits

| Commit    | Subject                                                                                       | Task     |
| --------- | --------------------------------------------------------------------------------------------- | -------- |
| `b0aea4d` | feat(19-05): mode-split picker renderer — family headers + full-width pill + compose dispatch | 19-05-01 |

## What Shipped

### `src/cli/picker/render.rs` — 333 → 616 lines

**Topology (top-down):**

1. **`render()`** — public stdout entry (~6 lines). Reads `terminal::size()`, delegates to `render_into`, flushes.
2. **`render_into<W: io::Write>()`** — mode dispatcher (~12 lines). Branches on `state.preview_mode_full` between `render_list_dominant` and `render_full_preview`.
3. **`render_list_dominant<W: io::Write>()`** — existing render body extended with family bands + pill cursor + dim descriptions (~140 lines).
4. **`render_full_preview<W: io::Write>()`** — NEW (~50 lines). Clears screen, emits `✦ slate  preview · Tab to return` breadcrumb, calls `compose::compose_full(palette, decide_fold_tier(rows), roles, None)`, indents the body 2 cols, emits flash text if any.
5. **`queue_family_heading<W>()`** — NEW helper. 2-space indent + `Roles::heading(family)` + `\r\n`; degrades to `◆ family` plain text when Roles is unavailable (D-05).
6. **`queue_variant_row<W>()`** — NEW helper. Selected branch: `format!("› {:<20}  {}", name, desc)` padded to `cols - 2`, wrapped in `Roles::command(padded)` pill. Non-selected branch: 4-space indent + `Roles::theme_name(name:20)` + 2 spaces + `Roles::path(desc)`.
7. **`render_afterglow_receipt`** — unchanged (SWATCH-RENDERER-allowlisted).
8. **`render_opacity_slot<W: io::Write>`** — existing fn, generalized from `&mut io::Stdout` to `<W: Write>` so list-dominant can invoke it while rendering into `Vec<u8>`.
9. **Helpers** (`should_guard_light_theme_opacity`, `get_effective_opacity_for_rendering`, `is_ghostty`, `queue_io`, `io_err`, `opacity_to_label`, `parse_hex_color`) — unchanged.

**Old `SetForegroundColor(Cyan)` + `"  › "` selected marker (render.rs L68–72)** → replaced wholesale by `Roles::command(padded_row_body)` per D-14. The cyan-on-dark-grey pattern is retired in favor of the theme's `brand_accent` foreground against the D-04 blend bg.

### `src/cli/picker/preview/compose.rs` — 12 → 110 lines (Wave-2 stub)

Minimum shim populated with signature-compatible symbols Plan 19-05 requires:

```rust
pub(crate) enum FoldTier { Minimum, Medium, Large }
pub(crate) fn decide_fold_tier(rows: u16) -> FoldTier
pub(crate) fn compose_full(palette, tier, roles, prompt_line_override) -> String
```

`compose_full` emits `◆ Palette / ◆ Prompt / ◆ Code / ◆ Files` under Minimum (+ `◆ Git / ◆ Diff` at Medium, `+ ◆ Lazygit / ◆ Nvim` at Large) with placeholder bodies. This covers Plan 19-05's `mode_dispatch_uses_preview_mode_full` assertion (`contains("◆ Palette") && contains("◆ Code")`).

**Merge handoff:** Plan 19-04 owns the real composer in a parallel worktree. On merge, the 19-04 body supersedes this shim wholesale. Module-level docstring declares the intent explicitly so future readers + the orchestrator's merge step understand why two plans touch the same file.

### 4 new unit tests — all green

| Test                                              | Decision | What it proves |
| ------------------------------------------------- | -------- | -------------- |
| `family_headers_are_render_time_only`             | D-08     | `state.theme_ids()` has zero `◆`-prefixed strings; rendered output for a Catppuccin cursor contains `◆ Catppuccin` |
| `pill_cursor_padded_to_terminal_width`            | D-14     | Selected-row visible body (after ANSI strip) is ≥ `cols - 4` wide (accepts Roles::command's space-wrapping overhead) |
| `non_selected_row_shows_description`              | D-08     | `catppuccin-frappe`'s `get_theme_description` text appears in the rendered output (sibling variant in visible window) |
| `mode_dispatch_uses_preview_mode_full`            | D-12     | `preview_mode_full=false` → output has `◆ Catppuccin` and NOT `◆ Palette`; `=true` → output has `◆ Palette` AND `◆ Code` |

**Test fixtures (private to `tests` mod):**
- `render_to_vec(state, cols, rows) -> Vec<u8>` — wraps `render_into` around a `Cursor<Vec<u8>>`.
- `strip_ansi(bytes) -> String` — SGR-byte filter (drops `ESC [...<letter>` sequences; letter terminator covers `m` for SGR + `A/B/C/D/H/J/K` for cursor/clear so the stripper survives the `Clear(All) + MoveTo(0,0)` prelude).

## Verification Results

| Gate                                                                   | Result   | Details |
| ---------------------------------------------------------------------- | -------- | ------- |
| `cargo build --lib`                                                    | ✅ GREEN | 2.6s incremental; compose.rs shim keeps render.rs compilable in-worktree pre-19-04 merge |
| `cargo test --lib picker::render::tests`                               | ✅ GREEN | 4/4 passed (all new Plan 19-05 tests) |
| `cargo test --lib brand::migration`                                    | ✅ GREEN | 11/11 passed including the Phase 18 aggregate `no_raw_styling_ansi_anywhere_in_user_surfaces` |
| `cargo test --lib`                                                     | ✅ GREEN | 783 passed / 0 failed / 0 ignored (was 779 pre-plan: delta +4 new render tests) |
| `cargo test --test theme_tests`                                        | ✅ GREEN | 12/12 passed — includes Plan 19-01's `slate_demo_surface_stays_retired_post_phase_19` invariant |
| `cargo test --test integration_tests`                                  | ✅ GREEN | 67/67 passed — unchanged from Wave 1 baseline |
| `cargo clippy --all-targets -- -D warnings`                            | ✅ GREEN | Zero warnings on render.rs + compose.rs |
| `rustfmt --check --edition 2021 src/cli/picker/{render.rs,preview/compose.rs}` | ✅ GREEN | Both touched files rustfmt-clean |
| `grep -n 'x1b\[' src/cli/picker/render.rs` → count outside allowlist   | ✅ 0     | Only the 4 pre-existing SWATCH-RENDERER-marked or allowlisted literals (alt-screen + cursor + reset + receipt color) remain |
| `wc -l src/cli/picker/render.rs` (plan target: ~450–500)               | ✅ 616   | Exceeds upper bound by ~116 lines because of (a) the preserved `render_afterglow_receipt` fn (kept intact per plan done-criteria), and (b) the test mod which the plan's target range underestimated |

**Full `cargo fmt --check` (crate-wide)** reports pre-existing drift in `src/brand/{render_context,roles}.rs` + `src/cli/picker/preview/blocks.rs` — same drift Plans 19-01 / 19-02 / 19-03 documented as SCOPE BOUNDARY (inherited from Wave 0 commit 437da1e). Not touched by this plan; `git checkout --` reverted the stray `cargo fmt -- src/...` side-effects so the diff stays scope-clean.

## Deviations from Plan

### Rule 3 — Auto-fix blocking issue: compose.rs Wave-2 parallel-worktree stub

- **Trigger:** Plan 19-05 executes in a parallel worktree sibling to Plan 19-04 (both Wave 2). Plan 19-05's `render_full_preview` MUST call `compose::compose_full` per its `<must_haves>` truths + `<action>` body. At worktree baseline (commit `644078c`), `compose.rs` is a 12-line Plan-19-01 skeleton — no `FoldTier`, no `decide_fold_tier`, no `compose_full`.
- **Why Rule 3 (not Rule 4 architectural):** the `parallel_execution` directive atop the agent prompt explicitly said "follow the signature in your plan's `<interfaces>` section literally; integration mismatch is detected at post-merge test gate". The signature IS frozen (matches Plan 19-04 plan's frontmatter + `<interfaces>` block verbatim); what's missing is the binding. Without a stub, `cargo build` fails on 2 `cannot find function in module compose` errors, which in turn blocks `cargo clippy` AND the Phase 18 aggregate invariant (the aggregate is `#[cfg(test)]` and reads source files via `fs::read_to_string`, but it lives in a test binary that must compile). Adding a stub is the minimum-blast-radius way to satisfy both (a) the plan's MUST-call contract and (b) the in-worktree test gates.
- **Scope of shim:** 3 public items (`FoldTier` enum, `decide_fold_tier`, `compose_full`) + 1 private helper (`push_heading`). Shim body is deliberately minimal — emits `◆ Heading` labels at the right count per tier, with placeholder bodies underneath. This is exactly enough to pass `mode_dispatch_uses_preview_mode_full` and no more.
- **Merge handoff:** module-level docstring declares the intent — "Plan 19-04 merge replaces this module body wholesale". Two plans touching the same file in the same wave means the merge step must take 19-04's version; signatures match so render.rs doesn't need re-edits.
- **Files modified:** `src/cli/picker/preview/compose.rs` (+98 lines of shim; previously 12-line skeleton).
- **Commit:** bundled into `b0aea4d` alongside the render.rs work — separating them would commit a non-compiling render.rs mid-commit.

### Rule 3 — Auto-fix blocking issue: `render_opacity_slot` generalized from `&mut io::Stdout` to `<W: io::Write>`

- **Trigger:** `render_list_dominant` is generic over `<W: io::Write>` so tests can feed `Vec<u8>`. The pre-Phase-19 `render_opacity_slot(stdout: &mut io::Stdout, ...)` hard-coded stdout, breaking the genericity.
- **Why Rule 3:** single private fn in `render.rs`; generalizing to `<W: io::Write>` is a local change with zero behavior diff. All callers are inside this file and adapt trivially (pass `out` instead of `stdout`).
- **Fix:** signature change only; the fn body's `queue!(stdout, ...)` → `queue!(out, ...)` mechanical rename.

### Task-level TDD pragmatism (GREEN-first-one-shot)

- **Trigger:** Plan marked the task `tdd="true"`. The 4 tests depend on `render_into<W: Write>(...)` — the refactor's own entry point.
- **Why single commit:** a true RED phase would require shipping `render_into` as a signature-only stub first (so tests can compile against it) and then implementing the body in GREEN. But that stub would itself be half the refactor (without the body, the tests fail to panic meaningfully — they fail on zero output). Following Plan 19-02's Deviation §Task-level TDD pragmatism, the verification gate between commits (pre-plan `cargo test --lib` 779 passed → post-plan 783 passed = 4 new tests) preserves TDD's safety property (tree green at each commit) without the intermediate test-only commit.
- **Safety maintained:** each verification gate re-asserted `cargo test --lib picker::render::tests` + `cargo test --lib brand::migration` + `cargo clippy -D warnings` before the commit. The aggregate invariant caught one false-positive during iteration (a `\x1b[...m` literal inside a test-module docstring tripped the grep) — proof the guards fire as intended.

## Known Stubs

1. **`src/cli/picker/preview/compose.rs` — Wave-2 parallel-worktree stub.** Plan 19-04 (Wave 2 sibling) supersedes this on merge. See Deviations §Rule 3 above for rationale + merge handoff protocol.

No other stubs introduced. render.rs has no `todo!()`, no `unimplemented!()`, and no mock data paths wired to user surfaces.

## Threat Flags

None. All new chrome routes through `Roles::*` (command / heading / path / theme_name / logo); no new network surface, auth surface, or FS surface. The `get_theme_description` fallback is `unwrap_or("")` — empty description degrades cleanly (non-selected row becomes name-only). The registry-miss fallback path was preserved from the pre-Phase-19 loop.

## Deferred Items

| Item | Reason | Owner |
| ---- | ------ | ----- |
| `cargo fmt --check` drift in `src/brand/{render_context,roles}.rs` + `src/cli/picker/preview/blocks.rs` | Pre-existing on base commit 644078c (inherited from Wave 0); SCOPE BOUNDARY | Phase 20 SFX work (those files will be touched there) or a dedicated housekeeping commit |
| compose.rs shim removal on Plan 19-04 merge | 19-04 owns the real composer; merge must select 19-04's body | Plan 19-04 merge resolution |
| Plan 19-05's `render_full_preview` currently passes `None` for `prompt_line_override` — no starship fork yet | Plan 19-06 ships `starship_fork`; Plan 19-07 event_loop glue wires the fork output into `compose_full`'s override parameter | Plans 19-06 + 19-07 |
| Tab keypress toggle of `state.preview_mode_full` | Plan 19-07 event_loop glue adds the `KeyCode::Tab` handler | Plan 19-07 |

## Call-Sites Consumed by This Plan (traceability)

- `PickerState::preview_mode_full` (Plan 19-03) → read in `render_into` dispatcher.
- `ThemeVariant.family` (pre-Phase-19) → read in `render_list_dominant` loop to drive the `◆ FamilyName` band-change detector.
- `theme::get_theme_description` (pre-Phase-19) → read in `queue_variant_row` for both selected-row pill body and non-selected row desc column.
- `brand::roles::Roles::{command, heading, path, theme_name, logo}` (Phase 18) → routed across all new chrome.
- `compose::{FoldTier, decide_fold_tier, compose_full}` (Plan 19-04 via Wave-2 shim) → called in `render_full_preview`.
- `preview_panel::render_preview` (Phase 15/16/17) → still called in the list-dominant `show_preview` branch; ungutted.

## Next Up

**Plan 19-06** (Wave 3): implement `starship_fork` — spawn a starship subprocess with the active theme's palette exported, capture one prompt-line rendering, return `Option<String>`. Feed the output into `compose_full`'s `prompt_line_override` when invoked by Plan 19-07.

**Plan 19-07** (Wave 3): wire the final pieces into `event_loop.rs`:
1. Install `RollbackGuard::arm` + `install_rollback_panic_hook` after `TerminalGuard::enter` (Plan 19-03 call-site docs).
2. Add `KeyCode::Tab` handler that flips `state.preview_mode_full`.
3. On `ExitAction::Commit`, call `state.commit()` (already writes through the shared `Rc<Cell<bool>>` so RollbackGuard's Drop short-circuits).
4. Replace this plan's `prompt_line_override: None` with a call to Plan 19-06's `starship_fork` output, cached per theme change.

**Plan 19-08** (Wave 4): manual UAT + CHANGELOG entry + release-prep.

## Self-Check: PASSED

- FOUND: `src/cli/picker/render.rs` (616 lines)
- FOUND: `src/cli/picker/preview/compose.rs` (110-line shim)
- FOUND: commit `b0aea4d` in `git log --oneline` — "feat(19-05): mode-split picker renderer…"
- CONFIRMED: `cargo test --lib picker::render::tests` 4/4 passed
- CONFIRMED: `cargo test --lib brand::migration` 11/11 passed (Phase 18 aggregate green)
- CONFIRMED: `cargo test --lib` 783 passed / 0 failed (was 779 pre-plan; delta +4)
- CONFIRMED: `cargo test --test theme_tests` 12/12 passed
- CONFIRMED: `cargo test --test integration_tests` 67/67 passed
- CONFIRMED: `cargo clippy --all-targets -- -D warnings` zero warnings
- CONFIRMED: `rustfmt --check --edition 2021 src/cli/picker/{render.rs,preview/compose.rs}` exit 0
- CONFIRMED: no unintended file deletions in commit (`git diff --diff-filter=D HEAD~1 HEAD` empty)
- CONFIRMED: Phase 18 aggregate `no_raw_styling_ansi_anywhere_in_user_surfaces` still green — no new raw `\x1b[` literal introduced outside the pre-existing `render_afterglow_receipt` SWATCH-RENDERER scope
