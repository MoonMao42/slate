---
phase: 19
plan: 08
subsystem: picker-integration
tags: [picker, integration, uat, bench, wave-6]
dependency_graph:
  requires:
    - 19-05 (PickerState + opacity axis)
    - 19-06 (starship_fork pure plumbing)
    - 19-07 (event_loop + RollbackGuard)
    - 19-09 (apply_theme_with_env env-injection stack)
  provides:
    - integration coverage for VALIDATION rows 1, 3, 5, 12, 14
    - criterion bench picker_starship_fork_latency (row 13)
    - manual UAT checklist (rows M1-M5)
  affects:
    - src/cli/picker/preview/mod.rs (visibility bump)
    - src/cli/picker/preview/starship_fork.rs (pub(crate)→pub)
tech_stack:
  added:
    - tempfile (tests — TempDir)
    - criterion (bench extension — already in dev-deps)
  patterns:
    - SlateEnv::with_home(tempdir) for env injection
    - Dependency-injected starship_bin: Option<&Path> (no PATH mutation)
    - Hard assertions on managed/* existence (V-08 fix)
key_files:
  created:
    - tests/picker_full_preview_integration.rs
    - tests/picker_starship_fork_fixture.rs
    - .planning/phases/19-slate-demo-redesign-picker-live-preview/19-UAT-CHECKLIST.md
  modified:
    - tests/theme_tests.rs (appended picker_launches_with_family_grouping)
    - benches/performance.rs (added bench_starship_fork_latency)
    - src/cli/picker/preview/mod.rs (starship_fork: pub(super)→pub)
    - src/cli/picker/preview/starship_fork.rs (fork_starship_prompt + StarshipForkError: pub(crate)→pub)
decisions:
  - Hard-assert managed/ghostty/theme.conf existence replaces V-08 silent-return escape hatch — if the tempdir SlateEnv wiring breaks, the test fails loudly
  - Seed Ghostty integration config stub in Esc-rollback test so adapter's `if !integration.exists() { Skipped }` branch doesn't mask the real contract under test
  - Picker state tests exercise `silent_preview_apply` directly (not through the picker event loop) because crossterm alt-screen needs a real PTY — PTY-bound scenarios moved to the UAT checklist
metrics:
  duration: 12m
  completed_date: 2026-04-20
  completed_utc: 2026-04-20T04:23:29Z
requirements: [DEMO-03]
---

# Phase 19 Plan 08: Wave-6 Integration Gate Summary

Shipped Wave 4/6 integration coverage for the picker full-preview
pipeline: three new integration tests exercising tempdir-injected
`SlateEnv`, two starship-fork fixture tests using dependency injection
(no PATH mutation), a criterion latency bench for the fork path, and a
5-item manual UAT checklist that `/gsd-verify-work 19` consumes as the
final human gate.

One-liner: **Plan 19-08 closes the automated Wave-6 integration rows
(1, 3, 5, 12, 14) + bench (13), promotes the starship-fork API from
`pub(crate)` to `pub` for cross-crate drive, and ships the 5-item manual
UAT checklist — zero `std::env::set_var`, zero subprocess indirection,
hard assertions on managed/* state (V-08 fix).**

## Task Breakdown

### Task 19-08-01 — Full-preview integration tests (commit `29644c2`)

- `tests/theme_tests.rs::picker_launches_with_family_grouping`
  (VALIDATION row 1) — walks `PickerState::theme_ids()`, resolves each
  id via `ThemeRegistry::get`, and asserts the family-index in
  `FAMILY_SORT_ORDER` is monotonically non-decreasing. Asserts ≥ 2
  distinct families visited.
- `tests/picker_full_preview_integration.rs` (new file, 3 tests):
  - `picker_nav_does_not_persist_current_file` (row 3, D-10 layer 1) —
    pre-seeds `~/.config/slate/current`, runs two `move_down` + one
    `move_up` on `PickerState`, asserts content AND mtime stable.
  - `picker_esc_rolls_back_managed_ghostty` (row 5, D-11 layer 1) —
    seeds Ghostty integration config, runs `silent_preview_apply(&env,
    "catppuccin-mocha", Solid)`, hard-asserts
    `managed/ghostty/theme.conf` exists, drifts to
    `catppuccin-frappe/Frosted`, rolls back, asserts baseline ==
    rolled_back byte-for-byte.
  - `picker_tab_enters_full_mode_without_persisting` — toggles
    `preview_mode_full`, asserts `current` file untouched.
- All tests use `SlateEnv::with_home(tempdir.path().to_path_buf())` —
  zero `std::env::set_var`, zero `Command::new`.

### Task 19-08-02 — Starship-fork integration + criterion bench (commit `5869544`)

- Visibility bump: `pub(super) mod starship_fork` →
  `pub mod starship_fork`; `pub(crate) fn fork_starship_prompt` →
  `pub fn ...`; `pub(crate) enum StarshipForkError` → `pub enum ...`.
  Plan 19-08 explicit scope; narrow surface (one fn + one enum). Phase
  20 can reuse the API.
- `tests/picker_starship_fork_fixture.rs` (new file, 2 tests):
  - `fork_missing_binary_falls_back` (VALIDATION row 12) — injects
    `Some(&PathBuf::from("/nonexistent/bin/starship"))`; asserts
    `Err(StarshipForkError::NotInstalled)`. No PATH mutation.
  - `fork_rejects_path_outside_managed_dir_integration` (row 14 / V-11
    fix) — `managed_dir` is `env.managed_subdir("managed")`,
    `managed_toml` is `/etc/passwd`; asserts
    `Err(StarshipForkError::PathNotAllowed)`.
- `benches/performance.rs::bench_starship_fork_latency` — criterion
  bench using `fork_starship_prompt(&managed_toml, &managed_dir, 80,
  None)`. Documents D-04 30-80 ms envelope + 200 ms regression alarm.
  Hard threshold enforcement deferred to a future `cargo bench` wrapper
  that parses criterion output (Phase 20 candidate). `cargo bench
  --bench performance -- picker_starship_fork_latency --test` → Success.

### Task 19-08-03 — UAT checklist (commit `87b04a7`)

- `.planning/phases/19-slate-demo-redesign-picker-live-preview/19-UAT-CHECKLIST.md`
  with 5 manual verification rows:
  - UAT-1 Ghostty real-reload smoothness (D-01, DEMO-03 #2)
  - UAT-2 Tab full-mode starship fork matches real prompt (D-04)
  - UAT-3 Esc rollback visually reverts Ghostty bg (D-11 layer 1)
  - UAT-4 Ctrl+C mid-nav → managed/* rolled back (D-11 layer 2)
  - UAT-5 `slate demo` command no longer exists (D-05)
- Each row: goal, numbered steps, expected outcome, pass/fail slot,
  notes slot. Sign-off footer (tester/date/build/overall).
- Committed with `git add -f` since `.planning/` is gitignored.

## Verification Log

| Check | Result |
|---|---|
| `cargo test --test theme_tests picker_launches_with_family_grouping` | 1/1 pass |
| `cargo test --test picker_full_preview_integration` | 3/3 pass |
| `cargo test --test picker_starship_fork_fixture` | 2/2 pass |
| `cargo test --test theme_tests` (full suite) | 13/13 pass |
| `cargo test --lib` (full library) | 810/0 pass |
| Phase 18 aggregate `no_raw_styling_ansi_anywhere_in_user_surfaces` | pass |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo build --release` | clean |
| `cargo bench --bench performance --no-run` | compiles |
| `cargo bench --bench performance -- picker_starship_fork_latency --test` | Success |
| Grep sweep `std::env::set_var|PathGuard|PATH_LOCK|Command::new` in new test files | only comments |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Adapted hard-assert path to real Ghostty managed layout**

- **Found during:** Task 19-08-01, first `cargo test` run on
  `picker_esc_rolls_back_managed_ghostty`.
- **Issue:** Plan 19-08 described the managed Ghostty config as
  `managed/ghostty-config` (a hyphenated single filename), but the
  production adapter writes `managed/ghostty/theme.conf`,
  `opacity.conf`, `blur.conf`, `font.conf` (subdirectory layout).
- **Fix:** Helper `managed_ghostty_theme_path` resolves to
  `env.managed_subdir("managed").join("ghostty").join("theme.conf")`,
  matching the adapter's actual
  `config_manager.write_managed_file("ghostty", "theme.conf", ...)`
  call.
- **Files modified:** `tests/picker_full_preview_integration.rs`.
- **Commit:** `29644c2`.

**2. [Rule 3 - Blocking] Seed Ghostty integration config so adapter writes managed/***

- **Found during:** Task 19-08-01, second `cargo test` run — after
  fixing the path, the hard-assert still fired because the adapter was
  returning `Skipped(MissingIntegrationConfig)`.
- **Issue:** `apply_theme_with_env` bails early with
  `SkipReason::MissingIntegrationConfig` if
  `~/.config/ghostty/config` doesn't exist. A fresh tempdir is empty,
  so the adapter never wrote managed/* and the hard assert tripped —
  not because the env-injection stack was broken, but because the
  adapter was legitimately skipping an opt-in surface.
- **Fix:** Added `seed_ghostty_integration(&env)` helper that touches
  `{tempdir}/.config/ghostty/config` with a placeholder comment before
  calling `silent_preview_apply`. Mirrors real-world "user has opted in
  to Slate-managed Ghostty" state.
- **Files modified:** `tests/picker_full_preview_integration.rs`.
- **Commit:** `29644c2` (folded into the Task-01 commit — both
  discovered before commit).

**3. [Rule 2 - Missing critical] Force-add `.planning/` SUMMARY via `git add -f`**

- **Found during:** Task 19-08-03 commit step.
- **Issue:** `.planning/` is gitignored project-wide; `git add
  .planning/phases/.../19-UAT-CHECKLIST.md` was a silent no-op.
- **Fix:** Used `git add -f` (matches the pattern the previous Phase
  19 plans used for their SUMMARY files — see `git log -- .planning/`
  showing all SUMMARY commits). This SUMMARY will also need `-f`.
- **Files modified:** none (tooling change).
- **Commit:** `87b04a7`.

## V-01 / V-08 / V-11 Compliance

- **V-01 (no PATH mutation in tests):** Both new test files grep clean
  for `std::env::set_var`, `PathGuard`, `PATH_LOCK`, and `Command::new`
  — only comment mentions remain. Fork tests inject
  `Some(&PathBuf::from("/nonexistent/bin/starship"))` via the 4-arg
  `fork_starship_prompt` signature.
- **V-08 (no silent-pass escape hatches):** `picker_esc_rolls_back_managed_ghostty`
  uses `assert!(managed_ghostty.exists(), "test setup invariant: ...")`
  — the plan's prior
  `if !managed_ghostty.exists() { eprintln!; return; }` pattern is
  explicitly replaced. If the env-injection stack ever regresses, the
  test fails loudly instead of masking the bug.
- **V-11 (V12 guard at integration scope):**
  `fork_rejects_path_outside_managed_dir_integration` drives the
  `PathNotAllowed` branch through the `pub` API with a
  `SlateEnv`-derived `managed_dir`. The existing unit test
  (`config_path_is_managed_only` inside `starship_fork::tests`) covers
  the same branch at the `pub(crate)` layer; the integration companion
  ensures the guard survives the visibility bump.

## Phase 19 Test Count Rollup

| Scope | Count | Source |
|---|---|---|
| Library unit tests | 810 | `cargo test --lib` |
| Integration tests (this plan only) | 5 | 3 full-preview + 2 fork fixture |
| theme_tests entry added this plan | 1 | `picker_launches_with_family_grouping` |
| Criterion benches (total) | 2 | `apply_theme_all_adapters` + `picker_starship_fork_latency` |
| Manual UAT rows | 5 | `19-UAT-CHECKLIST.md` |

## Known Stubs

None — all tests drive live code paths end-to-end. The UAT checklist is
intentionally a human-driven surface, not a stub.

## Follow-ups

- **Hard bench threshold enforcement** — deferred to Phase 20 (or a
  post-v2.2 maintenance plan). Criterion does not fail on latency; a
  wrapper script parsing the JSON output would add the 200 ms alarm.
- **UAT-4 panic hook verification on `panic = "abort"` profiles** — if
  the user's release profile is switched to `panic = "abort"`, the
  triple-guard layer 2 panic path will no-op. Worth documenting in the
  CHANGELOG before v2.2 ships if that profile change lands.

## Self-Check: PASSED

- `tests/picker_full_preview_integration.rs` — FOUND
- `tests/picker_starship_fork_fixture.rs` — FOUND
- `.planning/phases/19-slate-demo-redesign-picker-live-preview/19-UAT-CHECKLIST.md` — FOUND
- Commits `29644c2`, `5869544`, `87b04a7` — all present in `git log`
- All automated verification commands in the log above returned green
