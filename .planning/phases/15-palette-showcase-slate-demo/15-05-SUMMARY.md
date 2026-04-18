---
phase: 15-palette-showcase-slate-demo
plan: 05
subsystem: cli
tags: [slate, rust, integration-tests, bench, verification, d-b4, demo-01, demo-02]

# Dependency graph
requires:
  - phase: 15
    plan: 03
    provides: "src/cli/demo.rs::render_to_string + render_covers_all_ansi_slots unit test (D-B4 gate at unit level)"
  - phase: 15
    plan: 04
    provides: "Commands::Demo wiring + demo-hint call sites in setup.rs / theme.rs / set.rs"
  - phase: 15
    plan: 00
    provides: "10 #[ignore]'d demo_* scaffold tests + bench_demo_render scaffold in benches/performance.rs"
provides:
  - "10 live demo_* integration tests (no #[ignore]) exercising DEMO-01 render surface, DEMO-02 hint policy, size-gate rejection, and D-B4 integration-level 16/16 ANSI slot coverage"
  - "demo_touches_all_ansi_slots integration gate using `assert_eq!(hit, 16, …)` — belt-and-suspenders with Plan 03's unit-level `render_covers_all_ansi_slots`; both fail together if Plan 03's sample data regresses"
  - "demo_size_gate_rejects integration gate that works deterministically on macOS + Linux CI: setsid(2) + TERM/COLUMNS/LINES scrub closes both crossterm fallback paths (/dev/tty + tput)"
  - "Bug fix in src/cli/theme.rs: `emit_demo_hint_once(false, quiet)` now forwards --quiet per D-C1, replacing Plan 04's hard-coded `(false, false)`"
affects:
  - "Phase 15 /gsd-verify-work gate — all automated gates (unit, integration, bench, clippy, fmt) now pass cleanly"
  - "DEMO-01 closed — library render + CLI size gate + 16/16 ANSI slot coverage all locked"
  - "DEMO-02 closed — hint emits at setup + theme <id>, suppressed by --quiet/--auto/slate-set, all 5 edges live-tested"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Deterministic size-gate testing via std::process::Command + pre_exec(setsid) + TERM/COLUMNS/LINES env scrub. Closes both crossterm fallback paths (/dev/tty + tput) so the gate fires even on developer machines with an active controlling terminal. First time this codebase uses unsafe pre_exec in an integration test — rationale documented inline."
    - "ANSI-stripping substring assertions: strip_ansi_for_tests helper mirrors demo.rs's unit-test idiom. Tests assert against visible text (post-strip) so per-span RESET escapes between coloured words don't break substring matches like `type User` or `HEAD -> main`."

key-files:
  created: []
  modified:
    - "tests/integration_tests.rs — 10 #[ignore]'d demo_* stubs replaced with real assertions; strip_ansi_for_tests helper added; demo_size_gate_rejects rewritten to use std::process::Command + setsid(2) for deterministic TTY detachment (#[cfg(unix)])"
    - "src/cli/theme.rs — bug fix: `emit_demo_hint_once(false, quiet)` instead of `(false, false)`, so `slate theme <id> --quiet` actually suppresses the hint per D-C1"

key-decisions:
  - "Used setsid(2) + TERM/COLUMNS/LINES scrub rather than trusting assert_cmd's PTY-less assumption. Plan's inline note said 'If your CI harness happens to give a PTY (unlikely but possible), this test may flip — document the assumption in a comment.' On macOS developer machines the test DOES flip because crossterm's `size()` opens /dev/tty directly (bypassing piped stdout). The fix is to make the test robust in BOTH environments rather than document an environment-dependent assumption. New test uses std::process::Command with pre_exec(libc::setsid) + env scrubs, and is gated `#[cfg(unix)]` since setsid is Unix-specific. Works uniformly on macOS dev, Linux CI, and Windows (where cfg(unix) skips it)."
  - "Fixed theme.rs to forward `quiet` to `emit_demo_hint_once` as a Rule 1 bug repair. Plan 04's summary justified hard-coding `(false, false)` on the grounds that 'clap's argument parsing never routes --quiet + <name> to this branch without also bypassing the hint via user intent.' That argument is wrong: `slate theme catppuccin-mocha --quiet` cleanly reaches the `else if let Some(name)` branch, and Plan 15's D-C1 / VALIDATION.md demands --quiet suppression there. The fix is two characters (`false` → `quiet`); the auto branch exits before this line so auto=false remains correct."
  - "Used strip_ansi_for_tests helper in demo_renders_all_blocks rather than changing the plan's prescribed substrings ('type User', 'HEAD -> main'). The renderer emits each coloured word as its own `\\x1b[38;2;…m<word>\\x1b[0m` span, so 'type User' is NOT literally present in the raw output. Stripping first and asserting on visible text matches Plan 03's own unit-test idiom (render_to_string_contains_all_four_blocks uses strip_ansi) and preserves the plan's intent (verify blocks render) without weakening the assertion."
  - "Task 15-05-02 makes no source changes (plan-prescribed: 'No source code changes in this task unless cargo fmt finds formatting drift or clippy flags an issue'). All gates were already green after Task 15-05-01's commit, so no Task-2 commit exists. SUMMARY.md records the bench run + gate results."

patterns-established:
  - "Rule 1 bug-fix on a downstream bug discovered during integration testing: when a prior plan's implementation decision contradicts a planned integration test's contract, the bug is in the implementation, not the test. The executor fixes at the source (theme.rs line 115) rather than relaxing the test, and records the deviation in the summary so the verifier can audit."
  - "Robust integration testing of TTY-gated surfaces: don't rely on the runner's environment. Force the desired TTY state (detached / attached / specific size) via pre_exec + env scrub, gated by #[cfg(unix)] when the technique is platform-specific."

requirements-completed: [DEMO-01, DEMO-02]

# Metrics
duration: ~11min
completed: 2026-04-18
---

# Phase 15 Plan 05: Integration Tests & Bench Gate Summary

**Filled in all 10 `#[ignore]`'d demo_* integration test stubs from Plan 00 with real assertions against the wired DEMO-01 render surface and DEMO-02 hint policy. `demo_touches_all_ansi_slots` enforces the D-B4 contract at integration level with `assert_eq!(hit, 16, …)` — strict, matching Plan 03's unit-level gate. `demo_size_gate_rejects` uses std::process::Command + setsid(2) + TERM/COLUMNS/LINES scrub so the gate fires deterministically on both macOS developer machines and Linux CI. A two-character bug fix in theme.rs (`(false, false)` → `(false, quiet)`) makes `slate theme <id> --quiet` actually suppress the hint per D-C1. All gates green: `cargo test` 509 + 78 + … passes (0 failures), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean, `bench_demo_render` mean 6.84 µs (0.000684% of the 1 s budget).**

## Performance

- **Duration:** ~11 minutes
- **Started:** 2026-04-18T02:45:46Z
- **Completed:** 2026-04-18T02:56:30Z
- **Tasks:** 2 (Task 15-05-01 produced a single commit; Task 15-05-02 is a verification gate with no code changes)
- **Files created:** 0
- **Files modified:** 2 (`tests/integration_tests.rs`, `src/cli/theme.rs`)

## Accomplishments

### Task 15-05-01 — 10 demo_* integration tests live, strict D-B4 gate enforced

- Replaced all 10 `#[ignore]`'d stubs in `tests/integration_tests.rs` with real assertions. Names match VALIDATION.md task IDs exactly. Zero `#[ignore]` attributes remain on any `demo_*` function (verified by `grep -B1 '^fn demo_' | grep -q '#\[ignore\]'` → non-zero exit).
- `demo_renders_all_blocks` — library-level `render_to_string(&palette)` produces all four block markers (`type User`, `my-portfolio`, `HEAD -> main`, `72%`) plus at least one ANSI 24-bit FG escape. Uses `strip_ansi_for_tests` helper before substring checks because the renderer emits each coloured word as its own span separated by `\x1b[0m` resets.
- `demo_size_gate_rejects` — `#[cfg(unix)]` test that rebuilds the child process via `std::process::Command::new(env!("CARGO_BIN_EXE_slate"))` with `pre_exec(libc::setsid)` and `env_remove("TERM"/"COLUMNS"/"LINES")`. setsid detaches the child from the controlling terminal so `/dev/tty` returns ENXIO; the env scrub ensures crossterm's `tput cols / lines` fallback also fails. Result: `size()` returns `Err`, `unwrap_or((0, 0))` fires the gate, exit is non-zero, and combined stdout+stderr contains both `"80"` and `"slate demo"` from `Language::demo_size_error`.
- `demo_size_gate_accepts_minimum` — library-level `render_to_string` produces non-empty output whose every line (post-ANSI-strip) fits within 80 visible cols.
- **`demo_touches_all_ansi_slots`** — D-B4 integration-level gate. Collects every distinct RGB triplet emitted as `\x1b[38;2;R;G;Bm` or `\x1b[48;2;R;G;Bm` from `render_to_string`'s output, then maps each of the catppuccin-mocha palette's 16 ANSI slots (black/red/green/yellow/blue/magenta/cyan/white + bright variants) back through `PaletteRenderer::hex_to_rgb` and asserts `assert_eq!(hit, 16, …)` — **strict equality**, not `>=`. Plan 03's "Locked sample data" design is therefore regression-locked at BOTH unit and integration layers: if sample data drifts and any slot stops lighting up, both `render_covers_all_ansi_slots` (unit) and `demo_touches_all_ansi_slots` (integration) fail together, so the regression cannot slip through a single-layer hole.
- `demo_hint_setup_emits_once` — `slate setup --quick` stdout/stderr combined contains at least one `slate demo` literal (from `Language::DEMO_HINT`).
- `demo_hint_theme_guards` — `slate theme catppuccin-mocha` stdout contains `slate demo`.
- `demo_hint_theme_quiet_suppresses` — `slate theme catppuccin-mocha --quiet` stdout does NOT contain `slate demo`. This test drove the theme.rs bug fix (see Deviations below).
- `demo_hint_theme_auto_suppresses` — `slate theme --auto` stdout does NOT contain `slate demo`, regardless of auto-theme success (CI auto-resolve can fail; policy is still "never emit").
- `demo_hint_no_stack_with_set_deprecation` — `slate set catppuccin-mocha` prints the `'slate set' is transitioning` deprecation tip AND does NOT print the demo hint (D-C3 non-interference preserved via the `suppress_demo_hint_for_this_process()` pre-suppression in set.rs from Plan 04).
- `demo_sub_second_budget` — 10× `render_to_string` loop completes in <500 ms (observed ~70 µs for 10 iterations — three orders of magnitude under budget).

### Task 15-05-02 — Full gate green

- `cargo bench --bench performance bench_demo_render --no-run` → exit 0 (bench compiles).
- `cargo bench --bench performance demo_render_all_blocks` → criterion report `demo_render_all_blocks  time:   [6.8247 µs 6.8424 µs 6.8633 µs]`. **Mean: 6.84 µs** — 0.000684% of the 1 s SLA. Room for ~146 000 renders per second without approaching the budget.
- `cargo fmt --check` → exit 0.
- `cargo clippy --all-targets -- -D warnings` → exit 0 over the full workspace.
- `cargo test` full workspace → exit 0. Per-binary counts: 509 lib tests, 78 integration_tests (10 new demo_* + 68 pre-existing), 5 + 8 + 6 + 10 + 11 + 3 + 11 + 6 + 1 in other binaries; 1 pre-existing ignored test elsewhere (not demo-related). **Total: all green.**
- Full `cargo test` wall-clock: ~16 s.

## Task Commits

| Task ID     | Commit    | Type  | Title |
| ----------- | --------- | ----- | ----- |
| 15-05-01    | `8be0538` | test  | `test(15-05): fill in 10 demo_* integration tests with real assertions` (also includes the Rule 1 bug fix in theme.rs) |
| 15-05-02    | —         | —     | Verification gate only; no code changes — all files clean after Task 15-05-01. |

## Files Created/Modified

- `tests/integration_tests.rs` — **modified**; replaced 81 lines of `#[ignore]`'d stubs with 333 lines of real assertions + the `strip_ansi_for_tests` helper. 10 `demo_*` functions are now `#[test]` only (no `#[ignore]`). `demo_size_gate_rejects` is `#[cfg(unix)]` because `setsid(2)` is Unix-specific.
- `src/cli/theme.rs` — **modified**; 1-line behaviour change + 3-line justification comment. `emit_demo_hint_once(false, false)` → `emit_demo_hint_once(false, quiet)` at line 115 of the `Some(name)` branch.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `slate theme <id> --quiet` did not suppress the demo hint**

- **Found during:** Task 15-05-01 — `demo_hint_theme_quiet_suppresses` failed.
- **Issue:** Plan 04 hard-coded `crate::cli::demo::emit_demo_hint_once(false, false)` in `src/cli/theme.rs` (the `else if let Some(name)` branch). Plan 04's summary justified this as "symmetric form" on the argument that clap wouldn't route `--quiet` + `<name>` to that branch. That argument is wrong: `slate theme catppuccin-mocha --quiet` cleanly routes there, and D-C1 / Plan 15 VALIDATION.md demands `--quiet` suppression.
- **Fix:** Changed `(false, false)` → `(false, quiet)`. The `auto` branch exits (via the `if auto { … Ok(()) }` arm at lines 67–98) before this line, so `auto=false` remains correct. Added a 3-line justification comment at the call site.
- **Files modified:** `src/cli/theme.rs` (line 115 + surrounding comment).
- **Commit:** `8be0538` (alongside the test fill-in).

**2. [Rule 1 - Bug] `demo_renders_all_blocks` substring assertions wouldn't match**

- **Found during:** Task 15-05-01 — first test run failed with "code block must be present".
- **Issue:** Plan-prescribed substrings `"type User"` and `"HEAD -> main"` are NOT literally present in `render_to_string`'s raw output because the renderer emits each coloured word as its own `\x1b[38;2;R;G;Bm<word>\x1b[0m` span. Between "type" and "User" there's a `\x1b[0m ` (reset + space), breaking the substring match.
- **Fix:** Added a `strip_ansi_for_tests` helper (mirroring demo.rs's unit-test `strip_ansi` idiom) and asserted on the stripped visible text. Preserves the plan's intent (verify all four blocks render) without weakening any assertion.
- **Files modified:** `tests/integration_tests.rs`.
- **Commit:** `8be0538`.

**3. [Rule 1 - Bug] `demo_size_gate_rejects` didn't fire on macOS developer machines**

- **Found during:** Task 15-05-01 — test failed with `output.status.success() == true`.
- **Issue:** Plan's premise — "Under assert_cmd there's no PTY, so crossterm::terminal::size() returns Err" — is false on macOS. crossterm's `size()` uses `tty_fd()` which opens `/dev/tty` directly, bypassing the child's piped stdout. On a developer machine with an active controlling terminal, the child inherits that terminal's `/dev/tty` and gets a valid size (80×24 or larger). Even when `/dev/tty` fails, crossterm has a second fallback via `tput cols / tput lines` which reads from `TERM`/terminfo and returns a synthesised default.
- **Fix:** Rewrote the test to use `std::process::Command::new(env!("CARGO_BIN_EXE_slate"))` with (a) `pre_exec(libc::setsid)` to create a new session with no controlling terminal, and (b) `env_remove("TERM"/"COLUMNS"/"LINES")` to defeat the tput fallback. Both fallbacks now return Err, `unwrap_or((0, 0))` fires the gate, and exit is non-zero. Gated `#[cfg(unix)]` because `setsid(2)` is Unix-specific — Windows CI will skip it; that's acceptable because slate is macOS+Linux-only per PROJECT.md.
- **Files modified:** `tests/integration_tests.rs`.
- **Commit:** `8be0538`.

### Auth Gates

None.

## Known Stubs

None. All 10 demo_* tests are real, live gates with no TODO/placeholder/skip paths.

## D-B4 Gate Confirmation

- **Unit level** (`src/cli/demo.rs::tests::render_covers_all_ansi_slots`): passing — every one of the 16 ANSI slots has an emitted RGB triplet in the rendered string.
- **Integration level** (`tests/integration_tests.rs::demo_touches_all_ansi_slots`): passing with `assert_eq!(hit, 16, …)` — strict equality, not `>=`. No "practical floor" comment, no `>=12` fallback.
- **Implication:** If Plan 03's sample data regresses such that any slot stops lighting up, BOTH tests fail together. The D-B4 contract is now regression-locked across two independent layers.

## Verification Results

| Gate | Command | Result |
|------|---------|--------|
| Build | `cargo build --tests --quiet` | exit 0 |
| Demo tests only | `cargo test --test integration_tests demo_` | **10/10 passed** |
| Strict D-B4 | `cargo test --test integration_tests demo_touches_all_ansi_slots` | passed |
| Integration suite | `cargo test --test integration_tests` | 78/78 passed |
| Full workspace | `cargo test` | all green (509 lib + 78 integration + others; ~16 s wall-clock) |
| Lints | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| Formatting | `cargo fmt --check` | exit 0 |
| Bench compiles | `cargo bench --bench performance bench_demo_render --no-run` | exit 0 |
| Bench runs | `cargo bench --bench performance demo_render_all_blocks` | mean **6.84 µs** |

## Bench Detail — `demo_render_all_blocks`

```
demo_render_all_blocks  time:   [6.8247 µs 6.8424 µs 6.8633 µs]
Found 9 outliers among 100 measurements (9.00%)
  4 (4.00%) high mild
  5 (5.00%) high severe
```

- **Mean:** 6.84 µs per full 4-block render.
- **Budget:** 1 s (1 000 000 µs). Headroom: ~146 000×.
- **Verdict:** Rendering cost is not a scaling concern; the bench exists as a regression guard against accidental O(n²) regressions or heap churn in future refactors.

## Phase 15 Readiness

All automated gates green. Phase 15 is ready for `/gsd-verify-work`:

- DEMO-01 (palette showcase command) — render surface wired end-to-end, size gate integration-tested deterministically, D-B4 16/16 ANSI slot coverage locked at both unit and integration level.
- DEMO-02 (session-local demo hint) — 5 edges (setup emit, theme-id emit, --quiet suppress, --auto suppress, slate-set pre-suppress) all live-tested; D-C3 non-interference with SLATE_SET_DEPRECATION_TIP confirmed.

## Self-Check

- File exists: `tests/integration_tests.rs` → FOUND
- File exists: `src/cli/theme.rs` → FOUND
- Commit exists: `8be0538` → FOUND
- 10 `fn demo_*` functions: confirmed by `grep -c '^fn demo_' tests/integration_tests.rs` → 10
- 0 `#[ignore]` attributes: confirmed by `grep -c '#\[ignore\]' tests/integration_tests.rs` → 0
- Strict `assert_eq!(hit, 16, …)`: confirmed by the plan's exact grep pattern
- No relaxed `>=12` fallback: confirmed absent
- Bench mean recorded: 6.84 µs
- Cargo test full-suite wall-clock: ~16 s
- Clippy + fmt: both exit 0

## Self-Check: PASSED
