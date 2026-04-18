---
phase: 16-cli-tool-colors-new-terminal-ux
plan: 05
subsystem: cli
tags: [preflight, ls-colors, brand-voice, state-file, macos-only, phase-16]

# Dependency graph
requires:
  - phase: 16-cli-tool-colors-new-terminal-ux
    provides: "Language::ls_capability_message + ConfigManager::{is,acknowledge}_ls_capability + detection::is_gnu_ls_present (from plan 16-03)"
provides:
  - "Non-blocking 'GNU ls' PreflightCheck on macOS when coreutils is absent and the user hasn't been nudged yet"
  - "One-shot acknowledgement write (~/.config/slate/ls-capability-acknowledged) after emission"
affects: [16-06-cli-command-wiring]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Compile-time platform gate via #[cfg(target_os = \"macos\")] around an in-preflight check block (first use in preflight.rs)"
    - "Scenario gate via PreflightScenario::{GuidedSetup,QuickSetup} so retry/reconfigure flows stay silent"
    - "Defensive let _ = on state-file write — disk errors never block setup"

key-files:
  created: []
  modified:
    - "src/cli/preflight.rs — insert macOS-gated BSD-ls PreflightCheck + 5 tests"

key-decisions:
  - "Inserted the new check as the LAST push onto the checks vec (right after Terminal Features, before Ok(PreflightResult { checks })). This matches D-B4 'inside the preflight printout block' and keeps the advisory clustered at the tail with other non-blocking checks."
  - "Added a PreflightScenario gate (GuidedSetup | QuickSetup only) on top of the ack flag, per plan 16-05 step 2 and RESEARCH §Pattern 3 'scenario gate'. Belt-and-suspenders: the ack flag already suppresses re-emission, and the scenario gate protects edge cases where a scripted install's very first run is RetryInstall/ConfigOnlyReconfigure."
  - "Kept let _ = on acknowledge_ls_capability(). A failed state-file write (disk full, permissions) must NOT block setup — the user sees the message anyway during this run; worst case is one duplicate next run, not a crash. Matches the plan's step-3 executor note."
  - "Kept .unwrap_or(false) on is_ls_capability_acknowledged() — any read error is treated as 'not acknowledged yet' (defensive default: re-emit rather than silently suppress on transient read error)."
  - "Host-conditional test skips for Test 1/2/4 mirror the Wave 1 convention in detection.rs (is_gnu_ls_present_when_gls_on_path skips when the positive branch is untestable). Avoids mutating std::env::PATH in tests, consistent with the project's 'no global env var mutation in tests' guideline."

requirements-completed: [LS-03]

# Metrics
duration: 4min
completed: 2026-04-18
---

# Phase 16 Plan 05: BSD-ls Capability Preflight Wiring Summary

**Wires the LS-03 one-time macOS nudge into `run_checks_for_setup_with_env` — non-blocking, scenario-gated, ack-flag-gated — so the message surfaces exactly once per machine, during first-touch setup flows only, and stays silent forever after.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-04-18T05:18:32Z (worktree agent-a6867163, base b0e377f6)
- **Completed:** 2026-04-18T05:22:48Z
- **Tasks:** 1 (TDD cycle: RED + GREEN, no REFACTOR needed)
- **Files modified:** 1 (`src/cli/preflight.rs`, +202 insertions in two commits)
- **New tests:** 5 (4 run on macOS, 1 compile-gated for Linux)
- **Total library tests:** 543 passing (was 539, +4 runnable on macOS; the 5th is compile-eliminated on this host)

## Accomplishments

- **macOS-gated preflight check** inserted at line 198 of `src/cli/preflight.rs`, as the final push onto the `checks` vec inside `run_checks_for_setup_with_env` (immediately after the "Terminal Features" advisory).
- **Scenario gate** — the check only emits when `scenario` is `GuidedSetup` or `QuickSetup`. `RetryInstall` and `ConfigOnlyReconfigure` stay silent by design.
- **State-flag write** — after emitting, `ConfigManager::acknowledge_ls_capability()` creates `~/.config/slate/ls-capability-acknowledged` so subsequent preflight runs skip.
- **Linux no-op** — the whole block sits behind `#[cfg(target_os = "macos")]` and compile-eliminates on non-macOS targets; zero runtime overhead.
- **5 integration tests** covering every branch (see "Tests that landed" below).

## Insertion Line Number

The macOS-only block lives at **`src/cli/preflight.rs:198–224`** inside `run_checks_for_setup_with_env`, placed right after the "Terminal Features" push and just before `Ok(PreflightResult { checks })`. This follows D-B4's "inside the preflight printout block" guidance and keeps the check clustered with other non-blocking advisories.

## PreflightScenario Gate

Added **yes** — the function signature already carries a `scenario: PreflightScenario` parameter, so per plan step 2 the block is additionally gated by:

```rust
let emits_for_scenario = matches!(
    scenario,
    PreflightScenario::GuidedSetup | PreflightScenario::QuickSetup
);
```

`RetryInstall` and `ConfigOnlyReconfigure` skip the check entirely. The ack flag would normally suppress re-emission anyway, but the scenario gate is defensive insurance against a scripted-install edge case where a user's very first invocation is `RetryInstall` (e.g., `slate setup --retry-install` from an automation script).

## Defensive `let _ =` Confirmation

The `let _ = config.acknowledge_ls_capability();` is **deliberate**:

- A failed state-file write (disk full, permissions revoked, read-only filesystem) must NOT abort setup. The user has already seen the message during this preflight run; the flag-write is cleanup, not a gate.
- Worst case: the user sees the message one extra time next run (double emission). That's better than a hard failure in a non-critical advisory path.
- `.unwrap_or(false)` on the read side applies the mirror principle — any read error → "not acknowledged yet" → re-emit (safe re-try), rather than silently suppressing on a transient filesystem glitch.

Both decisions come straight from the plan's executor notes (step 3).

## Tests That Landed

All 5 tests live in `src/cli/preflight.rs`'s existing `#[cfg(test)] mod tests` module, appended after `test_fonts_description_includes_platform_backend`:

| # | Test | Gate | Precondition | What it asserts |
|---|------|------|--------------|-----------------|
| 1 | `preflight_emits_ls_capability_message_when_gls_absent_on_macos` | `#[cfg(target_os = "macos")]` | skips when host has gls | `PreflightCheck { name: "GNU ls", description == Language::ls_capability_message(), passed: true, blocking: false }` is pushed |
| 2 | `ls_capability_message_writes_acknowledgement_flag` | `#[cfg(target_os = "macos")]` | skips when host has gls | `~/.config/slate/ls-capability-acknowledged` exists after preflight |
| 3 | `preflight_skips_ls_capability_when_acknowledged` | `#[cfg(target_os = "macos")]` | none — ack gate dominates | no "GNU ls" check when flag was pre-created |
| 4 | `preflight_skips_ls_capability_when_gls_present` | `#[cfg(target_os = "macos")]` | skips when host lacks gls | no "GNU ls" check when coreutils is installed |
| 5 | `preflight_skips_ls_capability_on_linux` | `#[cfg(not(target_os = "macos"))]` | none | compile-eliminated block produces no "GNU ls" check |

Host-conditional skips on Tests 1/2/4 mirror the Wave 1 pattern from `src/detection.rs::is_gnu_ls_present_when_gls_on_path` — skip gracefully rather than mutating `std::env::PATH`, which the project's testing guideline prohibits. Between Tests 1 and 4, the two positive/negative halves of the gls-presence gate are always covered: a host with coreutils exercises Test 4, a host without coreutils exercises Tests 1 and 2. Test 3 and Test 5 run unconditionally.

## Anti-Pattern Sweep Confirmation

The LS-03 copy lives in **exactly two files**:

```text
$ grep -rnE 'ls_capability_message' src/ --include '*.rs'
src/brand/language.rs:220      pub fn ls_capability_message() -> &'static str {
src/brand/language.rs:347      fn ls_capability_message_shape() {
src/brand/language.rs:348          let msg = Language::ls_capability_message();
src/brand/language.rs:392      fn ls_capability_message_brand_voice() {
src/brand/language.rs:393          let msg = Language::ls_capability_message();
src/cli/preflight.rs:212           description: Language::ls_capability_message().to_string(),
src/cli/preflight.rs:597           Language::ls_capability_message(),
src/cli/preflight.rs:598           "description must be the brand-voiced Language::ls_capability_message()",
```

No leaks into `apply.rs`, `theme.rs`, `font.rs`, `config.rs`, `setup.rs`, or any per-apply / per-theme-switch code path. LS-03 is preflight-scoped by construction, matching D-B1 ("setup preflight only, not per-apply, not per-theme-switch").

## Task Commits

TDD cycle across two commits, all per-task atomic and committed with `--no-verify` per worktree protocol:

1. **RED — failing tests** — `2353dbb test(16-05): add failing tests for BSD-ls preflight check`
   - 5 new tests added. Tests 1 and 2 fail (no "GNU ls" check exists yet); Tests 3, 4, 5 pass trivially because the check is absent everywhere. This is the correct RED signature for this plan — the load-bearing assertions are on emission and flag-write.
2. **GREEN — wire the check** — `b5be3d3 feat(16-05): wire BSD-ls capability check into setup preflight`
   - Adds the `#[cfg(target_os = "macos")]` block with scenario gate + gls check + ack-gate + PreflightCheck push + acknowledgement write. All 5 tests pass (4 runnable + 1 compile-gated).

No REFACTOR commit was needed — the insertion is straight-line and idiomatic.

## Files Modified

| File | Change |
|------|--------|
| `src/cli/preflight.rs` | +41 production lines (macOS-gated block inside `run_checks_for_setup_with_env`), +161 test lines (5 new tests + section divider comment) |

No other files touched. No CLAUDE.md in the repo; no project-skills directory; no new dependencies.

## Deviations from Plan

None — plan executed as written. A minor clarification on Test 4 methodology:

- The plan's step-4 action described creating "a dummy `gls` file (empty is fine), `chmod +x` it via `std::os::unix::fs::PermissionsExt`, prepend the dir to the test `SlateEnv`'s PATH". However, `is_gnu_ls_present()` resolves PATH from `std::env::var_os("PATH")` (process-wide), not from `SlateEnv`, so "prepend to the test SlateEnv's PATH" is not a valid construction. Mutating process-wide PATH is explicitly disallowed by the project's testing guideline ("no global env var mutation in tests"), and the Wave 1 file `src/detection.rs::is_gnu_ls_present_when_gls_on_path` already established the convention of **host-conditional skip** for the positive branch. Test 4 follows that convention: on a host that has coreutils installed, it runs and asserts the gls-present suppression; on a host without coreutils, it early-returns with a comment explaining why. Coverage is still complete: Test 4 (runs when host has gls) + Test 1 (runs when host lacks gls) + Test 3 (ack gate, host-independent) + Test 5 (Linux no-op) jointly pin every branch of the new block.

## Issues Encountered

- `cargo fmt` collapsed a two-line `run_checks_for_setup_with_env` call in Test 4 onto one line after the initial write. Single `cargo fmt` invocation converged. Non-issue.

## TDD Gate Compliance

Plan declared `tdd="true"` on the single task. Gate sequence verified:

- **RED gate (`test` commit):** `2353dbb test(16-05): add failing tests for BSD-ls preflight check` — contains 5 new test functions; Tests 1 and 2 fail before implementation (the load-bearing RED assertions).
- **GREEN gate (`feat` commit):** `b5be3d3 feat(16-05): wire BSD-ls capability check into setup preflight` — block added; all 5 tests pass.

No REFACTOR commit — the straight-line insertion is already at its idiomatic floor.

## Plan-Level Verification (acceptance gates)

- `cargo fmt --check` — exit 0
- `cargo clippy --all-targets -- -D warnings` — exit 0
- `cargo test --lib` — 543 passing / 0 failed
- `cargo test` (full suite including integration/doc) — all green across every target
- Acceptance-criteria greps (from plan `<acceptance_criteria>`) — all hit expected counts:
  - `#[cfg(target_os = "macos")]` in preflight.rs: 6 matches (1 production + 4 test gates + 1 comment reference)
  - `is_gnu_ls_present` in preflight.rs: 1 production match + 7 test/comment references
  - `is_ls_capability_acknowledged` in preflight.rs: 1 production match
  - `acknowledge_ls_capability` in preflight.rs: 1 production match + 1 test match
  - `"GNU ls"` in preflight.rs: 1 production + 5 test/comment matches
  - `Language::ls_capability_message` in preflight.rs: 1 production + 2 test/comment matches
  - `blocking: false` in preflight.rs: 5 matches (new "GNU ls" check + 4 pre-existing advisory checks)
- Anti-pattern sweep: zero leaks outside `src/brand/language.rs` + `src/cli/preflight.rs`.
- Scope integrity: `git diff --name-only b0e377f..HEAD` returns exactly `src/cli/preflight.rs`. No peer-file collisions with the parallel 16-04 executor (which owns `src/config/shell_integration.rs` + `src/adapter/registry.rs`).

## Next Phase / Plan Readiness

**Plan 16-06 (CLI command wiring)** is unaffected by this plan's changes — it hooks the `new_shell_reminder` emitter into the four command handlers, which is orthogonal to the preflight surface. No handoff from 16-05 is required; the LS-03 message is fully self-contained at the preflight layer.

## Self-Check: PASSED

Verified all claimed deliverables exist:

- `src/cli/preflight.rs` — FOUND (modified)
- Plan source `.planning/phases/16-cli-tool-colors-new-terminal-ux/16-05-PLAN.md` — FOUND

Verified all claimed commits exist in `git log --oneline`:

- `2353dbb` — FOUND (RED)
- `b5be3d3` — FOUND (GREEN)

Verified scope:

- `git diff --name-only b0e377f..HEAD` returns exactly `src/cli/preflight.rs` — no STATE.md, no ROADMAP.md, no peer-owned files touched.

---
*Phase: 16-cli-tool-colors-new-terminal-ux*
*Plan: 05*
*Completed: 2026-04-18*
