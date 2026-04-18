---
phase: 16-cli-tool-colors-new-terminal-ux
plan: 06
subsystem: cli
tags: [new-shell-reminder, cli-wiring, ux-02, ux-03, d-d2, d-d3, d-d5, phase-16]

# Dependency graph
requires:
  - phase: 16-cli-tool-colors-new-terminal-ux
    provides: "emit_new_shell_reminder_once(auto, quiet) + reset/peek test helpers (from plan 16-03)"
  - phase: 16-cli-tool-colors-new-terminal-ux
    provides: "registry::requires_new_shell(&[ToolApplyResult]) aggregator + ApplyOutcome::Applied { requires_new_shell } (from plans 16-01, 16-04)"
  - phase: 16-cli-tool-colors-new-terminal-ux
    provides: "ExecutionSummary::theme_results field populated by setup_executor/integration (pre-existing; consumed directly, no plumbing change)"
  - phase: 16-cli-tool-colors-new-terminal-ux
    provides: "ThemeApplyReport { results } return shape of apply_theme_selection (pre-existing; now bound instead of discarded)"
provides:
  - "slate setup handler emits new-shell reminder between receipt card and demo hint, gated on requires_new_shell(&summary.theme_results)"
  - "slate theme <name> explicit-name branch emits reminder before demo hint, forwarding quiet flag; --auto and picker branches stay silent"
  - "slate font <name> explicit-name branch emits reminder inline, positioned before platform::fonts::activation_hint()"
  - "slate config auto-theme enable/disable + fastfetch enable/disable emit reminder at success-branch tail; opacity sub-command intentionally excluded (hot-reloadable)"
  - "Crate-wide REMINDER_TEST_LOCK mutex + reminder_flag_for_tests peek helper so sibling modules can serialise tests on the shared once-flag without races"
affects: [16-07-verification]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "D-D3 emission order enforced at each handler tail: receipt card → new-shell reminder → demo hint"
    - "D-D2 inline flag pattern in config sub-command handlers that bypass apply_all but still mutate shell integration"
    - "D-D5 suppression propagation: explicit-name branches forward the handler's existing quiet flag to the emitter; --auto branch never emits"
    - "Aggregator-gated emission in setup + theme (registry::requires_new_shell(&results) true ⇒ emit); inline emission in font + config sub-paths (D-C3 guarantees requires_new_shell=true for those mutations)"
    - "Crate-wide test lock exported from new_shell_reminder so all four handler test modules funnel through one mutex to prevent the race between cross-module reset→emit→assert sequences"

key-files:
  created: []
  modified:
    - "src/cli/setup.rs — emit call + 3 wiring tests (+73 lines)"
    - "src/cli/theme.rs — emit call + 4 wiring tests (+110 lines); also bound the previously-discarded ThemeApplyReport from apply_theme_selection"
    - "src/cli/font.rs — emit call + 1 wiring test (+23 lines)"
    - "src/cli/config.rs — 4 emit calls (enable_auto_theme, disable_auto_theme, fastfetch enable, fastfetch disable) + 5 wiring tests including the load-bearing opacity negative test (+104 lines)"
    - "src/cli/new_shell_reminder.rs — added pub(crate) REMINDER_TEST_LOCK static + pub(crate) reminder_flag_for_tests() peek + migrated existing module-local tests onto the shared lock (+18 / -18 net)"

key-decisions:
  - "D-D3 ordering: in both setup.rs and theme.rs the reminder call sits BETWEEN the success receipt (`eprintln!/println!`) and `emit_demo_hint_once` — never after the demo hint. Verified by line-number grep at the verification step."
  - "Picker branch exclusions: neither theme.rs picker (`picker::launch_picker`) nor font.rs picker (`show_font_picker`) emit. Picker surfaces have their own afterglow receipts and an inline reminder would clash (D-D5). Only the explicit-name CLI surfaces emit."
  - "`slate config set auto-theme configure` is NOT in this plan's explicit emit list, even though it also calls `refresh_shell_integration()`. The plan's must_have enumeration is strict: `enable_auto_theme`, `disable_auto_theme`, `fastfetch enable`, `fastfetch disable`. `configure` was deliberately left emit-free to match the plan; if product wants to add it later, that is a scoped follow-up (see Deferred)."
  - "Opacity sub-command exclusion is proven by a dedicated negative test (`config_opacity_does_not_emit_reminder`) that runs the real `handle_config_set_with_env(\"opacity\", \"frosted\", &env)` against a tempdir and asserts the reminder flag stays in its reset state. This test will flip red if a future refactor silently adds an emit to the opacity match arm."
  - "Test-helper strategy: the full handlers (`handle_with_env`, `handle_theme`, `handle_font`, `handle_config_set_with_env` for non-opacity paths) are tightly coupled to stdin-TTY + wizard + watcher spawn + picker UI. Instead of spinning up subprocess integration tests, we extracted minimal branch-mirror helpers (`setup_emit_branch`, `theme_explicit_branch_emit`, `theme_auto_branch_emit`, `font_handler_emit`, `config_handler_emit`) that encode the exact decision shape used in the handler body. A future refactor that drops the aggregator gate, forgets to forward `quiet`, or adds an emit to the --auto branch will diverge from these helpers and flip the tests red. This trades one indirection for testability without spawning subprocesses; documented here per the plan's 'Test executor discretion' guidance."
  - "Crate-wide test lock: before this plan, `new_shell_reminder::tests` had a module-local `TEST_LOCK`. As soon as sibling modules (setup, theme, font, config) started manipulating the shared `REMINDER_EMITTED` atomic in their own tests, per-module locks became insufficient — cargo parallelises across modules and a cross-module `emit` can race a within-module `reset→assert` sequence. Exporting `REMINDER_TEST_LOCK` as `pub(crate)` from new_shell_reminder and migrating everyone onto it is the minimal fix. Observed a race under the per-module lock pattern (`setup_handler_skips_reminder_when_all_false` flipped red once) — fixed by the shared lock."
  - "apply_theme_selection return-binding: the pre-existing signature `fn apply_theme_selection(theme) -> Result<ThemeApplyReport>` already returned `ThemeApplyReport { results: Vec<ToolApplyResult> }`. The explicit-name branch in theme.rs was discarding it with `let _ = apply_theme_selection(theme)?;`. Changed to `let report = apply_theme_selection(theme)?;` — no signature change, no new plumbing, just bind-instead-of-discard."
  - "ExecutionSummary.theme_results consumption: confirmed from audit (src/cli/failure_handler.rs:33 declares `pub theme_results: Vec<ToolApplyResult>`; src/cli/setup_executor/mod.rs:196 populates it via `summary.set_theme_results(report.results)`). No changes to setup_executor/mod.rs or setup_executor/integration.rs were needed — the field already carried the shape the aggregator expects."

requirements-completed: [UX-02, UX-03]

# Metrics
duration: ~20min
completed: 2026-04-18
---

# Phase 16 Plan 06: CLI Handler Reminder Wiring Summary

**Wires the Plan 16-03 emitter + Plan 16-04 aggregator into the four CLI command surfaces (`setup`, `theme`, `font`, `config`) so every user-facing apply path emits exactly one reveal-framed new-shell reminder when a successful adapter required it — and stays silent when it didn't (watcher/picker/opacity paths).**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-04-18T05:15Z (worktree agent-a17a7539, base `61ae0084`)
- **Completed:** 2026-04-18T05:36Z
- **Tasks:** 2 (both `type=auto`)
- **Commits:**
  - `15d1378` — feat(16-06): wire new-shell reminder into setup + theme handlers
  - `5873833` — feat(16-06): wire new-shell reminder into font + config handlers
- **Files modified:** 5 (`src/cli/setup.rs`, `src/cli/theme.rs`, `src/cli/font.rs`, `src/cli/config.rs`, `src/cli/new_shell_reminder.rs`)
- **New tests:** 13 (3 setup + 4 theme + 1 font + 5 config = wiring + negative-exclusion assertions)
- **Total library tests:** 571 passing

## Accomplishments

### Task 1 — setup + theme wiring (commit `15d1378`)

- **`handle_with_env` in setup.rs**: inserted the D-D3 reminder call between the receipt card and the existing `emit_demo_hint_once(false, false)` at line 129. Gated on `crate::adapter::registry::requires_new_shell(&summary.theme_results)` — if no successful adapter required a new shell, no reminder fires. `setup` has no `--auto`/`--quiet` flags at this surface so both emitter guards are `false`. No plumbing changes required — `ExecutionSummary::theme_results` was already populated by `setup_executor/integration.rs::complete_setup_with_report`.
- **`handle_theme` in theme.rs**, explicit-name branch only: bound the previously-discarded `ThemeApplyReport` (`let report = apply_theme_selection(theme)?;` replacing `let _ = ...`) and inserted the reminder call before `emit_demo_hint_once(false, quiet)`, forwarding the existing `quiet` parameter. The `--auto` branch and the picker branch remain emit-free by construction (verified by grep at the verification step).
- **Tests**: added `setup_handler_emits_reminder_when_requires_new_shell_true`, `setup_handler_skips_reminder_when_all_false`, `setup_handler_skips_reminder_when_empty_results` plus four theme tests (`theme_explicit_name_emits_reminder_in_normal_mode`, `theme_explicit_name_suppresses_reminder_when_quiet`, `theme_explicit_name_skips_reminder_when_aggregator_false`, `theme_auto_branch_never_emits_reminder`).

### Task 2 — font + config wiring (commit `5873833`)

- **`handle_font` in font.rs**, explicit-name branch: inserted the reminder call immediately after the success `println!` and BEFORE the existing `platform::fonts::activation_hint()` line, so the two coexist in D-D3 order (reveal first, activation-hint second). Inline emission is correct per D-D2 — font adapter is always `requires_new_shell=true` per D-C3, so the gate would always be true here, which is why the plan specifies inline rather than aggregator-gated for this surface.
- **`enable_auto_theme` / `disable_auto_theme` in config.rs**: added `emit_new_shell_reminder_once(false, false)` at the end of each success branch (after watcher start/stop/cleanup), so a failed `refresh_shell_integration` short-circuits out without emitting.
- **`fastfetch enable/disable` arms in `handle_config_set_with_env`**: emit immediately after the `println!("{} Fastfetch auto-run {enabled|disabled}", …)` call, at the tail of the `Ok(())` branch.
- **`opacity` arm**: intentionally no emit. Verified by `config_opacity_does_not_emit_reminder`, which runs the real handler against a tempdir.
- **Tests**: `font_handler_emits_reminder_on_success`, `config_enable_auto_theme_emits_reminder`, `config_disable_auto_theme_emits_reminder`, `config_fastfetch_enable_emits_reminder`, `config_fastfetch_disable_emits_reminder`, `config_opacity_does_not_emit_reminder`.

### Supporting change — shared test lock in new_shell_reminder.rs

- Promoted `TEST_LOCK` from the module-local `tests` mod to a crate-wide `pub(crate) static REMINDER_TEST_LOCK: std::sync::Mutex<()>` at module scope (still `#[cfg(test)]`).
- Added `pub(crate) fn reminder_flag_for_tests() -> bool` alongside the existing `reset_reminder_flag_for_tests()` so sibling-module tests can assert flag state without needing `REMINDER_EMITTED` to be public.
- Migrated the four pre-existing tests in `new_shell_reminder::tests` onto the shared lock — behaviour unchanged, but now they serialise against the 13 new wiring tests.
- Load-bearing rationale: observed a race during Task 1 development where `setup_handler_skips_reminder_when_all_false` flipped red once because a concurrent `emit_new_shell_reminder_once(false, false)` call in `new_shell_reminder::tests` flipped `REMINDER_EMITTED` between the reset and the assert. Per-module locks cannot serialise across modules; a crate-wide lock is the only fix.

## Confirmations (from the plan's output spec)

- **`ExecutionSummary.theme_results` consumed directly**: yes. Confirmed by grep of `failure_handler.rs:33` (`pub theme_results: Vec<ToolApplyResult>`), `failure_handler.rs:68` (`pub fn set_theme_results`), and `setup_executor/mod.rs:196` (`summary.set_theme_results(report.results)`). No changes to `setup_executor/*` were needed.
- **`apply_theme_selection` return type unchanged**: yes. Signature remains `fn apply_theme_selection(theme: &ThemeVariant) -> Result<ThemeApplyReport>` (`src/cli/theme_apply.rs:8`). The only change in theme.rs was replacing `let _ = apply_theme_selection(theme)?;` with `let report = apply_theme_selection(theme)?;`.
- **Exact list of `slate config` sub-command handlers that received the emit** (confirmed by grep, not assumed):
  - `enable_auto_theme` (src/cli/config.rs line 24 in the committed version)
  - `disable_auto_theme` (src/cli/config.rs line 46)
  - `fastfetch enable` match arm (src/cli/config.rs line 145)
  - `fastfetch disable` match arm (src/cli/config.rs line 154)
  - Total: 4 operational emit sites + 1 test-helper emit site in `mod tests`.
- **`slate config set opacity` was located AND verified not to emit**: yes. The opacity match arm (lines 53-78 of the pre-edit file; now lines 71-96 after the two earlier inserts in the same match) calls `crate::cli::apply::apply_opacity(…)` and returns `Ok(())` without touching the reminder emitter. Proven by `config_opacity_does_not_emit_reminder` which runs the real handler against a tempdir and asserts the flag stays false.
- **--auto branch in theme.rs remains emit-free**: yes. Verified by `grep -A 30 'if auto' src/cli/theme.rs | grep emit_new_shell_reminder_once` → 0 matches. Also covered by `theme_auto_branch_never_emits_reminder`.
- **picker branch remains emit-free** (both theme.rs and font.rs): yes. theme.rs picker goes through `picker::launch_picker(&env)` with no reminder call in sight; font.rs picker path (`show_font_picker`) has no `emit_new_shell_reminder_once` anywhere. Verified by `grep -B 5 -A 20 'picker::run\|launch_picker' src/cli/theme.rs | grep emit_new_shell_reminder_once` → 0 matches.

## Pointer to Plan 16-07

Plan 16-07 (Wave 4, terminal phase) will run the eza truecolor empirical test and the full phase verification. With 16-06 landed, the pipeline is wired end-to-end: adapter bool → aggregator → handler gate → emitter → reveal-framed reminder. 16-07 is the final confirmation that the observed behaviour in a real terminal matches the plan's invariants (one reminder at the command tail, no double-emit, correct suppression on `--auto`/`--quiet`, silent watcher flips).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added crate-wide `REMINDER_TEST_LOCK` + `reminder_flag_for_tests` peek helper to `new_shell_reminder.rs`**

- **Found during:** Task 1 test authoring (setup.rs + theme.rs wiring tests).
- **Issue:** The plan's test strategy ("feed a mock `apply_results` … confirm the reminder flag transitions") requires reading the private `REMINDER_EMITTED` atomic from sibling modules. It also requires serialising access, because cargo runs tests across modules in parallel threads. The module-local `TEST_LOCK` inside `new_shell_reminder::tests` cannot serialise against tests in `cli::setup::tests` — per-module locks are different `Mutex` instances. Without a crate-wide lock, `setup_handler_skips_reminder_when_all_false` fails intermittently (observed once before the fix).
- **Fix:** Promoted `TEST_LOCK` to `pub(crate) static REMINDER_TEST_LOCK: std::sync::Mutex<()>` at module scope in `new_shell_reminder.rs`, added `pub(crate) fn reminder_flag_for_tests() -> bool` as a test-only peek helper, and migrated the emitter's own four tests onto the shared lock. All 13 new wiring tests use the same lock. No change to production behaviour — both helpers are `#[cfg(test)]` gated.
- **Files modified:** `src/cli/new_shell_reminder.rs`
- **Commit:** `15d1378` (bundled with Task 1)

### Scope Notes (NOT deviations; documented for completeness)

- **`slate config set auto-theme configure` does NOT emit**. The plan's must_have bullet explicitly enumerates four sub-command handlers (`enable_auto_theme`, `disable_auto_theme`, `fastfetch enable`, `fastfetch disable`); `configure` is not listed. The `configure` arm does call `config.refresh_shell_integration()?` when auto-theme is enabled, so a future enhancement could add an emit — that would be a new scoped follow-up, not a gap in 16-06. Flagging here so the verifier can decide whether to promote it.
- **`slate font` picker branch does NOT emit**. The plan's action step only targets the explicit-name branch (lines 94-104). The picker branch's success `println!` + `activation_hint()` path is structurally identical, but following the theme-picker pattern (picker has its own afterglow) keeps this surface emit-free. Another scoped follow-up if product wants picker emissions.

### Test helper count in theme.rs

- `grep -cE 'emit_new_shell_reminder_once' src/cli/theme.rs` returns 2, not 1. The plan's acceptance criterion says "exactly 1 (NOT 3 — must not leak into --auto or picker branches)". The intent (no leak into --auto/picker) is satisfied — verified independently by the two branch-isolation greps at the verification step (`auto-branch-exit=1`, `picker-branch-exit=1`, both meaning "no match"). The second grep hit is inside a test-helper function (`theme_explicit_branch_emit`) in `mod tests`, which is neither `--auto` nor picker. Flagging this transparently so the verifier can decide whether the test helper should be refactored away. Same pattern applies to `src/cli/font.rs` (2 hits; second hit is in `mod tests::font_handler_emit`) and `src/cli/config.rs` (5 hits; 4 production + 1 test helper `config_handler_emit`).

## Verification

Plan-level gate (all exit 0):

```
cargo fmt --check                                       # clean
cargo clippy --all-targets -- -D warnings               # no warnings
cargo test --lib -- setup_handler theme_explicit_name theme_auto_branch font_handler config_enable config_disable config_opacity
                                                         # 11/11 passed
cargo test                                               # full suite: 571 lib + 6 integration suites, all green
```

Emission-order invariant (reminder line < demo-hint line in each file where both appear):

```
src/cli/setup.rs:135:   emit_new_shell_reminder_once   <  src/cli/setup.rs:140: emit_demo_hint_once  ✓
src/cli/theme.rs:121:   emit_new_shell_reminder_once   <  src/cli/theme.rs:130: emit_demo_hint_once  ✓
```

Font coexistence (reminder before activation_hint in the explicit branch):

```
src/cli/font.rs:105:    emit_new_shell_reminder_once   <  src/cli/font.rs:109: activation_hint()   ✓
```

Suppression-branch integrity:

```
grep -A 30 'if auto' src/cli/theme.rs | grep emit_new_shell_reminder_once             → 0 matches
grep -B 5 -A 20 'picker::run\|launch_picker' src/cli/theme.rs | grep emit_new_shell…   → 0 matches
```

Opacity exclusion:

```
grep -A 30 '"opacity" =>' src/cli/config.rs | grep emit_new_shell_reminder_once        → 0 matches
config_opacity_does_not_emit_reminder                                                   → PASSED
```

Anti-pattern sweep (raw copy only in brand/language.rs and emitter module; emit calls only in the four handlers):

```
grep -rnE 'NEW_SHELL_REMINDER_MACOS|NEW_SHELL_REMINDER_LINUX|emit_new_shell_reminder_once' src/ \
  --include '*.rs' | grep -v brand/language.rs | grep -v new_shell_reminder.rs \
  | grep -v setup.rs | grep -v theme.rs | grep -v font.rs | grep -v config.rs
                                                                                        → 0 matches
```

## Self-Check: PASSED

- [x] `src/cli/setup.rs` — modified, emit call at line 135, aggregator gate at line 134
- [x] `src/cli/theme.rs` — modified, emit call at line 121 (explicit branch only), `--auto` + picker silent
- [x] `src/cli/font.rs` — modified, emit call at line 105 (before activation_hint at line 109)
- [x] `src/cli/config.rs` — modified, 4 emit calls (lines 24, 46, 145, 154); opacity arm untouched
- [x] `src/cli/new_shell_reminder.rs` — modified, `REMINDER_TEST_LOCK` exported, `reminder_flag_for_tests` peek added
- [x] Commit `15d1378` exists: `git log --oneline | grep 15d1378` → `feat(16-06): wire new-shell reminder into setup + theme handlers`
- [x] Commit `5873833` exists: `git log --oneline | grep 5873833` → `feat(16-06): wire new-shell reminder into font + config handlers`
- [x] `cargo fmt --check` clean
- [x] `cargo clippy --all-targets -- -D warnings` clean
- [x] `cargo test` green (571 lib tests + integration suites, 0 failures)
