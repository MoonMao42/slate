---
phase: 19
plan: "03"
subsystem: cli-picker-state-rollback
tags: [picker, state, rollback-guard, panic-hook, wave-1, D-11, D-12, V-03]
dependency_graph:
  requires:
    - Plan 19-01 scaffolding (src/cli/picker/rollback_guard.rs skeleton + #[cfg(test)] mod tests hook)
    - SlateEnv::with_home for test isolation (src/env.rs)
    - silent_preview_apply (src/cli/set.rs:66) as the rollback side-effect entrypoint
  provides:
    - PickerState::preview_mode_full field (public, default false per D-12)
    - PickerState::committed as Rc<Cell<bool>> shared with RollbackGuard
    - PickerState::committed_flag() accessor for event_loop wiring (Plan 19-07)
    - RollbackGuard::arm + Drop impl (D-11 triple-guard layer 2)
    - install_rollback_panic_hook free fn (D-11 triple-guard layer 3, panic=abort bypass)
    - install_rollback_panic_hook_with_sentinel cfg(test) variant (V-03 behavior proof)
  affects:
    - src/cli/picker/state.rs (struct type change: committed bool → Rc<Cell<bool>>)
    - src/cli/picker/rollback_guard.rs (skeleton → full impl)
    - src/env.rs (derive Clone — Rule 3 auto-fix, see Deviations)
tech_stack:
  added:
    - std::rc::Rc + std::cell::Cell (shared interior mutability for committed flag)
    - std::sync::atomic::AtomicBool + std::panic::catch_unwind (V-03 sentinel test)
  patterns:
    - "TerminalGuard-analog RAII Drop for side-effect rollback (src/cli/picker/event_loop.rs:47-53 is the parent idiom)"
    - "panic::take_hook chaining (RESEARCH §Pattern 1) — restore terminal BEFORE prev_hook(info) so backtrace prints to a sane surface"
    - "V-03 sentinel-variant cfg(test) fn — behavior-proving test via catch_unwind + AtomicBool flip instead of disk-touching silent_preview_apply"
key_files:
  created: []
  modified:
    - src/cli/picker/state.rs (+119 / -12 lines: struct field + Drop update + 4 new tests)
    - src/cli/picker/rollback_guard.rs (skeleton → 285 lines: struct + arm + Drop + prod hook + cfg(test) sentinel hook + 4 tests)
    - src/env.rs (+7 / -1 lines: derive(Clone) on SlateEnv)
decisions:
  - "Derived Clone on SlateEnv (Rule 3 auto-fix) rather than carrying &SlateEnv with lifetime — the lifetime approach would force a 'static bound on the Box<dyn Fn> panic-hook closure, which is incompatible with a borrowed env reference. Clone derive on a 5-field PathBuf container is zero-cost in our call pattern (arm() once at picker launch, clone used once) and needs no sibling edits."
  - "Kept the supplementary panic_hook_uses_take_hook_chain_pattern source-ordering test alongside the primary V-03 sentinel test — the sentinel proves hook EXECUTED, the ordering test catches REORDERING regressions. Both cheap; both valuable for different failure modes."
  - "Added #[allow(dead_code)] on RollbackGuard, RollbackGuard::arm, and install_rollback_panic_hook — Plan 19-07 (launch_picker wiring) removes the attribute. Alternative (leave clippy failing) would block Wave 1 merge; alternative (remove pub(crate) to hide) would make Plan 19-07 unable to reach them. allow is the minimum-blast-radius option."
  - "committed_flag() returns Rc::clone, not a borrow — RollbackGuard needs owned data (Drop runs after PickerState borrow would end); this is the whole reason committed is an Rc<Cell<bool>> rather than a plain bool."
metrics:
  duration: "~20min (2026-04-20 09:55–10:15 UTC)"
  tasks_completed: 2
  files_modified: 3
  files_created: 0
  commits: 2
  completed_date: "2026-04-20"
---

# Phase 19 Plan 03: PickerState extension + RollbackGuard triple-guard Summary

Extended `PickerState` with the `preview_mode_full: bool` field (D-12) and migrated its `committed` tracker to a shared `Rc<Cell<bool>>` so `RollbackGuard` (Plan 19-07 will wire the arm site) can observe the user's Enter-commit before its Drop runs. Populated the Plan 19-01 skeleton at `src/cli/picker/rollback_guard.rs` with the full D-11 triple-guard: `RollbackGuard` RAII struct + `install_rollback_panic_hook` free fn + a cfg(test) `install_rollback_panic_hook_with_sentinel` twin that flips an AtomicBool instead of calling `silent_preview_apply`, enabling the V-03 behavior-proving test.

## Commits

| Commit | Subject | Tasks |
| ------ | ------- | ----- |
| `60d8b27` | feat(19-03): extend PickerState with preview_mode_full + shared Rc<Cell<bool>> committed flag | Task 19-03-01 |
| `230c411` | feat(19-03): implement RollbackGuard + install_rollback_panic_hook with V-03 behavior-proving test | Task 19-03-02 |

## What Shipped

### `src/cli/picker/state.rs` diff

**Struct-level change** (L17-39, +11 lines):
- `committed: bool` → `committed: Rc<Cell<bool>>` with a docstring explaining the share semantics.
- Added `pub preview_mode_full: bool` at the end of the struct with its own D-12 docstring (list-dominant on launch; Tab toggles in Plan 19-05).

**Constructor update** (L70-79):
- `committed: Rc::new(Cell::new(false))` replaces the plain `false` literal.
- `preview_mode_full: false` appended after `opacity_override_in_session: false`.

**Accessor changes**:
- `is_committed(&self) -> bool` now calls `self.committed.get()` (L122-124).
- New `pub(super) fn committed_flag(&self) -> Rc<Cell<bool>>` (L129-135) returning `self.committed.clone()`. `#[allow(dead_code)]` until Plan 19-07 wires it into `launch_picker`.

**Mutator changes**:
- `commit(&mut self)` now `self.committed.set(true)` (L204-209).
- `revert(&mut self)` now `self.committed.set(false)` (L215-222).
- `Drop for PickerState` checks `!self.committed.get()` and the stale `TODO: Call rollback helper...` comment was replaced with a pointer to `rollback_guard.rs` (L237-246).

**4 new unit tests** (append to `mod tests`, L455-541):
- `preview_mode_full_defaults_to_list_dominant` — D-12 default.
- `family_headers_are_not_in_theme_ids` — D-08 invariant (no `◆` prefix, no bare family name).
- `section_header_not_selectable` — walks the full `move_down` cycle and asserts every visited id resolves via `ThemeRegistry::get(id).is_some()`.
- `committed_flag_shared_with_guard` — V-03 sharing contract: flip via `state.commit()` is observed through the pre-cloned `Rc<Cell<bool>>` handle.

**Existing test migration**: `test_picker_state_commit_flag` now reads `.committed.get()` instead of `.committed` (the direct bool access no longer compiles).

### `src/cli/picker/rollback_guard.rs` full implementation

Skeleton from Plan 19-01 (12 lines) → full module (285 lines).

**Module docstring** (L1-20) — explicitly enumerates the three layers of D-11:
1. Normal Esc path (existing in `event_loop.rs:101-107`).
2. Stack-unwind path: `RollbackGuard::drop`.
3. `panic = "abort"` path: `install_rollback_panic_hook`.

**`RollbackGuard` struct** (L27-55):
- Fields: `env: SlateEnv`, `original_theme_id: String`, `original_opacity: OpacityPreset`, `committed: Rc<Cell<bool>>`.
- `arm(env: &SlateEnv, original_theme_id: &str, original_opacity, committed)` clones the env by value so Drop runs independent of caller borrow.

**`Drop for RollbackGuard`** (L57-70):
- Short-circuits on `committed.get() == true`.
- Calls `let _ = crate::cli::set::silent_preview_apply(&self.env, &self.original_theme_id, self.original_opacity)` — silent rollback, never panics inside Drop.

**`install_rollback_panic_hook`** (L72-102):
- Chains `std::panic::take_hook()` via `set_hook(Box::new(move |info| ...))`.
- Exact call order inside the closure:
  1. `disable_raw_mode` (RESEARCH V7 info-disclosure mitigation)
  2. `LeaveAlternateScreen + Show` (exit alt-screen before backtrace prints)
  3. `silent_preview_apply(&env, &original_theme_id, original_opacity)` (managed/* rollback)
  4. `prev_hook(info)` (delegate to default backtrace handler; after return, `panic = "abort"` kicks in)

**`install_rollback_panic_hook_with_sentinel`** (L117-133, cfg(test) only):
- Identical body to production hook, but step 3 is replaced with `sentinel.store(true, SeqCst)`.
- Enables V-03 behavior-proof via `catch_unwind` without touching managed/* on disk.

**4 unit tests** (L135-265, all `--test-threads=1`):

| Test | What it proves |
| --- | --- |
| `rollback_guard_noop_when_committed` | Drop short-circuits when `committed.get() == true`; no panic, flag unchanged. |
| `rollback_guard_on_drop_when_not_committed` | Drop reaches the rollback branch when `committed.get() == false`; test_env gives it a throwaway home so `silent_preview_apply` has a valid sandbox; flag stays false post-drop. |
| `panic_hook_rollback_on_abort_profile` (V-03) | Installs sentinel-variant hook; `catch_unwind` around a deliberate panic; sentinel must be `true` after. **Proves the hook body EXECUTED the rollback branch** — this is the primary behavior contract under `panic = abort` release builds. |
| `panic_hook_uses_take_hook_chain_pattern` | SUPPLEMENTARY source-ordering check: parses the production fn's source block, asserts `disable_raw_mode < LeaveAlternateScreen < silent_preview_apply < prev_hook(info)` by `str::find` offset. Catches reorder regressions (V7 info-disclosure pre-condition). |

### `src/env.rs` derive(Clone)

Single-line diff: `#[derive(Clone)]` added above `pub struct SlateEnv`. The struct holds five owned `PathBuf`s — `Clone` is O(paths) with no shared state. See Deviations §Rule 3 for the rationale.

## V-03 Compliance Note

The previous `panic_hook_rollback_on_abort_profile` shape in the plan description was a source-grep disguised as a behavior test — it verified strings appeared in the file but never proved the hook body executed. This implementation closes that gap:

- **Sentinel variant** (`install_rollback_panic_hook_with_sentinel`): mirrors production control flow; only step 3 differs (AtomicBool flip vs disk write).
- **Test body**: `install_rollback_panic_hook_with_sentinel(sentinel.clone())` → `catch_unwind(|| panic!(...))` → `sentinel.load(SeqCst) == true`.
- **Failure mode coverage**: if any future refactor (a) captures wrong state, (b) short-circuits the closure, or (c) swaps `set_hook` order with `take_hook`, the sentinel stays `false` and the test fails. Under `panic = abort` release builds this would otherwise silently leave managed/* drifted.
- **Cleanup**: test restores a default `(|_| {})` hook on exit so later tests in the same binary don't inherit ours. `--test-threads=1` is mandatory for this test file because `panic::set_hook` is process-global.

## Verification Results

| Gate | Result | Details |
| --- | --- | --- |
| `cargo test --lib picker::state::tests` | ✅ GREEN | 19 passed (15 pre-existing + 4 new from Task 19-03-01) |
| `cargo test --lib picker::rollback_guard::tests -- --test-threads=1` | ✅ GREEN | 4 passed (all new from Task 19-03-02) |
| `cargo test --lib` (full suite) | ✅ GREEN | 776 passed / 0 failed / 0 ignored (was 768 pre-plan; delta +8 = 4 state + 4 rollback_guard) |
| `cargo test --test theme_tests` | ✅ GREEN | 12 passed, including Plan 19-01's `slate_demo_surface_stays_retired_post_phase_19` invariant |
| `cargo clippy --all-targets -- -D warnings` | ✅ GREEN | Zero warnings. Three `#[allow(dead_code)]` markers on RollbackGuard + arm + install_rollback_panic_hook (will drop in Plan 19-07 wiring) and one on `committed_flag` (same). |
| `cargo build --release` | ✅ GREEN | 48s cold; release-profile compile confirms `panic = "abort"` doesn't fight the design. |
| `rustfmt --check src/cli/picker/{state.rs,rollback_guard.rs} src/env.rs` | ✅ GREEN | Our three touched files are rustfmt-clean. Pre-existing drift in `src/brand/{render_context,roles}.rs` documented in Wave 1 SUMMARY (inherited; SCOPE BOUNDARY). |
| Grep: `install_rollback_panic_hook_with_sentinel` | ✅ FOUND | V-03 sentinel hook present. |
| Grep: `sentinel.store(true, Ordering::SeqCst)` | ✅ FOUND | Sentinel flip present in cfg(test) variant. |
| Grep: `std::panic::take_hook` | ✅ FOUND | Hook-chain pattern preserved. |
| Grep: `disable_raw_mode` | ✅ FOUND | Terminal-restore ordering invariant satisfied. |

## Deviations from Plan

### Rule 3 — Auto-fix blocking issue: derived Clone on SlateEnv

- **Found during:** Task 19-03-02 setup, reading `src/env.rs` before implementing `RollbackGuard`.
- **Issue:** Plan 19-03 `<interfaces>` block asserts "SlateEnv is `Clone` per existing uses in event_loop.rs (silent_preview_apply borrows &SlateEnv)" and the RollbackGuard `arm` target code calls `env.clone()`. However, the struct at `src/env.rs:10` was declared without `#[derive(Clone)]`. Attempting to implement `arm` as spec'd would fail `cargo build` at the `env: env.clone()` line.
- **Why Rule 3 (not Rule 4 architectural):** Clone on a pure owned-PathBuf container is a zero-semantic-risk addition. No field is shared state, no `&` reference lives across calls, and no trait bound anywhere else in the codebase assumes `!Clone`. The alternative — carrying `&SlateEnv` through the guard — would force a `'static` bound on the `Box<dyn Fn>` panic-hook closure (because `std::panic::set_hook` requires `'static + Send + Sync`), which **cannot** be satisfied by a borrowed reference. Clone is therefore the only implementation path that satisfies the interface contract.
- **Fix:** Added `#[derive(Clone)]` above the struct declaration with a docstring note explaining why (RollbackGuard + panic hook ownership requirements). Existing call sites are unaffected: they all pass `&SlateEnv` and continue to work.
- **Files modified:** `src/env.rs` (+7 / -1 lines for derive + docstring update).
- **Commit:** Bundled into `230c411` with the RollbackGuard implementation — separating them would leave `rollback_guard.rs` non-compiling mid-commit.

### `#[allow(dead_code)]` on three production symbols

- **Found during:** Task 19-03-02 first `cargo clippy --all-targets -- -D warnings` run, which failed on `committed_flag`, `RollbackGuard`, `arm`, and `install_rollback_panic_hook` as "never used".
- **Issue:** Plan 19-03 builds the rollback primitives in Wave 1; Plan 19-07 wires them into `launch_picker`. Between Wave 1 merge and Wave 3 (Plan 19-07), the production symbols are consumed only by their unit tests — clippy flags them as dead code under `-D warnings`.
- **Why Rule 3:** Without an attribute, Wave 1 cannot merge green. Alternatives considered:
  - Remove `pub(crate)` visibility: blocks Plan 19-07 from reaching them.
  - Use `#[cfg(test)]`: would exclude them from the release build, breaking the Wave 3 integration.
  - Gate behind a feature flag: adds flag surface no other module uses.
  - `#[allow(dead_code)]` with a comment pointer: minimal blast radius; Plan 19-07 simply removes the four markers when wiring lands.
- **Fix:** Four `#[allow(dead_code)] // Wired by Plan 19-07 (launch_picker)` markers on the struct, `arm`, `install_rollback_panic_hook`, and `committed_flag`.
- **Files modified:** `src/cli/picker/state.rs`, `src/cli/picker/rollback_guard.rs`.
- **Commit:** Both in their respective feature commits.

## Known Stubs

None. All production symbols in this plan have at least one behavior-proving unit test:
- `PickerState::committed_flag` → `committed_flag_shared_with_guard`
- `PickerState::preview_mode_full` default → `preview_mode_full_defaults_to_list_dominant`
- `RollbackGuard::arm` + Drop (committed path) → `rollback_guard_noop_when_committed`
- `RollbackGuard::arm` + Drop (uncommitted path) → `rollback_guard_on_drop_when_not_committed`
- `install_rollback_panic_hook` body → `panic_hook_rollback_on_abort_profile` (via sentinel mirror) + `panic_hook_uses_take_hook_chain_pattern` (source ordering)

## Call-Sites to Wire in Plan 19-07

Plan 19-07 (event_loop integration) will:

1. At `launch_picker` entry (insert after the existing `let _guard = TerminalGuard::enter()?;` at `event_loop.rs:66`):
   ```rust
   let committed = state.committed_flag();
   let _rollback = super::rollback_guard::RollbackGuard::arm(
       env, &starting_theme_id, starting_opacity, committed.clone(),
   );
   super::rollback_guard::install_rollback_panic_hook(
       env.clone(), starting_theme_id.clone(), starting_opacity,
   );
   ```
2. On the `ExitAction::Commit` branch (`event_loop.rs:93-100`), `state.commit()` already runs; because `PickerState::commit` now writes through the shared `Rc<Cell<bool>>`, the `_rollback` guard's Drop will short-circuit correctly.
3. Remove the four `#[allow(dead_code)]` markers once the wiring lands.

Plan 19-05 (render mode dispatch) will:

1. Read `state.preview_mode_full` at the top of `render::render(...)` and mode-split between `render_list_dominant` (existing, extended with family header band + full-width pill cursor) and a new `render_full_preview` fn (compose.rs orchestrator + ◆ Heading stack).

## Deferred Items

| Item | Reason | Owner |
| ---- | ------ | ----- |
| `cargo fmt --check` drift in `src/brand/{render_context,roles}.rs` | Pre-existing on base commit 27d870b (inherited from Wave 0 commit 437da1e per Wave 1 SUMMARY); SCOPE BOUNDARY. | Phase 20 SFX work (those files will be touched there), or a dedicated housekeeping commit. |
| `#[allow(dead_code)]` removal on RollbackGuard, arm, install_rollback_panic_hook, committed_flag | Attributes drop when Plan 19-07 wires the symbols into `launch_picker`. | Plan 19-07 |

## Self-Check: PASSED

- FOUND: `src/cli/picker/state.rs` (modified, +119 / -12)
- FOUND: `src/cli/picker/rollback_guard.rs` (populated, ~285 lines)
- FOUND: `src/env.rs` (derive(Clone) added)
- FOUND: commit `60d8b27` (Task 19-03-01)
- FOUND: commit `230c411` (Task 19-03-02)
- CONFIRMED: `cargo test --lib picker::state::tests` 19 passed
- CONFIRMED: `cargo test --lib picker::rollback_guard::tests -- --test-threads=1` 4 passed
- CONFIRMED: `cargo test --lib` 776 passed / 0 failed (was 768 pre-plan; delta +8)
- CONFIRMED: `cargo clippy --all-targets -- -D warnings` green
- CONFIRMED: `cargo build --release` green (48s)
- CONFIRMED: all four plan-required grep assertions pass (`install_rollback_panic_hook_with_sentinel`, `sentinel.store(true, Ordering::SeqCst)`, `std::panic::take_hook`, `disable_raw_mode`)
- CONFIRMED: Plan 19-01's `slate_demo_surface_stays_retired_post_phase_19` CI invariant still green (12/12 in `theme_tests`)
