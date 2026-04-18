---
phase: 16-cli-tool-colors-new-terminal-ux
plan: 03
subsystem: ux
tags: [brand-voice, atomic-bool, dedup-emitter, state-file, detection, phase-16]

# Dependency graph
requires:
  - phase: 15-palette-showcase-slate-demo
    provides: "emit_demo_hint_once pattern in src/cli/demo.rs — exact template this plan mirrors"
provides:
  - "Language::ls_capability_message + Language::new_shell_reminder (English) + NEW_SHELL_REMINDER_{MACOS,LINUX} constants"
  - "emit_new_shell_reminder_once(auto, quiet) session-local dedup emitter in src/cli/new_shell_reminder.rs"
  - "ConfigManager::is_ls_capability_acknowledged + acknowledge_ls_capability (flat state file)"
  - "detection::is_gnu_ls_present()"
affects: [16-05-preflight-wiring, 16-06-cli-command-wiring]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Session-local dedup via AtomicBool::swap(true, SeqCst) — second instance, first was demo.rs"
    - "Flat state-file marker at ~/.config/slate/ — third instance after current-font + autorun-fastfetch"
    - "Compile-time platform branch via cfg!(target_os = \"macos\") for brand-voice copy"
    - "Hand-rolled Mutex for serializing tests that share a process-wide AtomicBool (no serial_test dep)"

key-files:
  created:
    - "src/cli/new_shell_reminder.rs — emit_new_shell_reminder_once + REMINDER_EMITTED flag"
  modified:
    - "src/brand/language.rs — ls_capability_message + new_shell_reminder + platform constants"
    - "src/cli/mod.rs — registered new_shell_reminder module (alphabetical)"
    - "src/config/tracked_state.rs — is_ls_capability_acknowledged + acknowledge_ls_capability + test module"
    - "src/detection.rs — is_gnu_ls_present() helper next to command_path"

key-decisions:
  - "Used cfg!(target_os = \"macos\") compile-time branch, not runtime platform::packages::detect_backend (RESEARCH §Pattern 7 recommendation)"
  - "Kept state-file flat at ~/.config/slate/ls-capability-acknowledged, NOT under a state/ subdir (RESEARCH §Pattern 3 correction to CONTEXT D-B3 wording)"
  - "English-only copy matching existing brand surface (RESEARCH §Open Question Q3 resolution)"
  - "Hand-rolled Mutex in the reminder test module instead of adding serial_test as a dev-dep for four tests"

patterns-established:
  - "Early-return BEFORE AtomicBool swap (RESEARCH §Pitfall 1) — protects --auto --quiet watcher path from burning the once-flag"
  - "Language accessor as &'static str method, not constant, so platform branch is self-contained inside Language (no platform-layer import)"

requirements-completed: [LS-03, UX-02, UX-03]

# Metrics
duration: 22min
completed: 2026-04-18
---

# Phase 16 Plan 03: Pure-function foundation for LS-03 + UX-03 Summary

**Brand-voiced LS-03 capability copy + platform-aware UX-03 reveal reminder + session-local dedup emitter (mirrors demo.rs) + flat state-file helpers + gls detection — all decoupled from CLI wiring so 16-05/16-06 can consume without merge conflicts.**

## Performance

- **Duration:** 22 min
- **Started:** 2026-04-18T04:47:00Z (worktree branch-point)
- **Completed:** 2026-04-18T05:09:07Z
- **Tasks:** 2
- **Files modified:** 5 (1 created, 4 extended)
- **New tests:** 15 (6 Language + 2 detection + 4 state-helper + 4 reminder)
- **Total library tests:** 524 passing (was 509, +15)

## Accomplishments

- **`Language::ls_capability_message()`** — multi-line LS-03 copy with the locked three-part shape (observation → consequence → fix). Exact English: `"✦ This macOS ships with BSD \`ls\`; the slate-managed LS_COLORS needs GNU \`ls\` to render.\n  Install it with \`brew install coreutils\` and your next shell lights up."`
- **`Language::new_shell_reminder()`** — compile-time platform branch. macOS returns `NEW_SHELL_REMINDER_MACOS`, Linux returns `NEW_SHELL_REMINDER_LINUX`.
- **`NEW_SHELL_REMINDER_MACOS`** — `"✦ ⌘N for a fresh shell — your new palette lives there"` (54 chars, fits ≤76 cap).
- **`NEW_SHELL_REMINDER_LINUX`** — `"✦ Open a new terminal — your new palette lives there"` (53 chars).
- **`emit_new_shell_reminder_once(auto, quiet)`** — session-local AtomicBool dedup. Early-return on `auto || quiet` happens BEFORE the swap so the Ghostty watcher path (`slate theme --auto --quiet`) never burns the flag.
- **`detection::is_gnu_ls_present()`** — one-line delegation to `command_path("gls").is_some()`.
- **`ConfigManager::{is_ls_capability_acknowledged, acknowledge_ls_capability}`** — flat marker file at `~/.config/slate/ls-capability-acknowledged` using `state_files::write_state_file`. No disable helper (matches `autorun-fastfetch` convention; `slate clean` wipes holistically).

## Task Commits

Each task used the TDD cycle (test first, then implementation):

1. **Task 1 RED — failing tests for LS-03 language + state + detection helpers** — `1e15bb4` (test)
   - 6 Language tests + 2 detection tests + 4 state-helper tests. Language methods absent so brand-voice tests fail to compile (RED). Detection + state-helper one-line wrappers landed here because separating them would force awkward moves; the Language tests are the true RED marker.
2. **Task 1 GREEN — add LS-03 capability message and UX-03 platform-aware reminder** — `1807d77` (feat)
   - Added the four Language items + the new_shell_reminder method; all 11 tests pass; fmt/clippy clean.
3. **Task 2 — emit_new_shell_reminder_once dedup emitter** — `6b290b3` (feat)
   - Created `src/cli/new_shell_reminder.rs` + registered in `src/cli/mod.rs`. Mirrors `src/cli/demo.rs` exactly (AtomicBool + swap + SeqCst + Typography::explanation wrapper). Four flag-state tests assert the Pitfall-1-load-bearing early-return-before-swap ordering.

## Files Created/Modified

- `src/cli/new_shell_reminder.rs` (created, 135 lines) — Emitter module with test helper (`reset_reminder_flag_for_tests`, cfg(test) only)
- `src/brand/language.rs` (+42 lines prod + 124 lines tests) — LS-03 message, UX-03 platform branch, 6 contract tests
- `src/cli/mod.rs` (+1 line) — `pub mod new_shell_reminder;` alphabetical between `list` and `picker`
- `src/config/tracked_state.rs` (+85 lines) — Two helper methods + embedded `#[cfg(test)] mod tests` with 4 tests mirroring fastfetch-autorun tests
- `src/detection.rs` (+30 lines) — `is_gnu_ls_present` next to `command_path` + 2 tests (delegation + positive-with-skip)

## Decisions Made

- **Compile-time platform branch (`cfg!(target_os = "macos")`)**: Chose RESEARCH §Pattern 7 option 2 over routing through `platform::packages::detect_backend`. The reminder copy is about UX surface ("which key opens a fresh shell"), not package manager; a Mac user without Homebrew still wants `⌘N` copy. Keeps `Language` self-contained.
- **Flat state-file layout**: CONTEXT D-B3 working-name referenced "sibling of existing `state/` files" but the codebase uses flat state files under `~/.config/slate/` directly (`current`, `current-font`, `current-opacity`, `autorun-fastfetch`). Followed RESEARCH §Pattern 3's correction — no `state/` subdir. Test #10 asserts the subdir is NOT created (load-bearing negative check).
- **English-only copy**: Phase 15's `DEMO_HINT` ships in English; the codebase has no Chinese-language brand surface yet; resolved RESEARCH §Open Question Q3 by staying English. CONTEXT's Chinese examples were the register, not the mandate.
- **Hand-rolled test Mutex**: Parallel test runner races on the shared `REMINDER_EMITTED` flag — initial run had `reminder_emits_once_per_process` fail because another thread's `reset_reminder_flag_for_tests()` preempted its assertion. Added a `static TEST_LOCK: Mutex<()>` in the test module so each test takes the lock before reset+emit+assert. Rejected adding `serial_test` as a dev-dep for four tests.
- **Typography import path**: Uses `crate::design::typography::Typography` — same path `src/cli/demo.rs` uses. Documented here for Plan 16-06 which will hook the emitter into the four command handlers.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] Parallel-test flag race**
- **Found during:** Task 2 (first test run)
- **Issue:** The four reminder tests share `REMINDER_EMITTED` via the module-level AtomicBool. Cargo's default parallel runner scheduled a second test's `reset_reminder_flag_for_tests()` between the first test's `emit` and its `assert`, flipping the assertion red.
- **Fix:** Added `static TEST_LOCK: Mutex<()>` inside the test module; every test takes the lock as its first step with `unwrap_or_else(|p| p.into_inner())` so poisoning doesn't chain-fail. This serializes the four tests without introducing `serial_test` as a dev-dep.
- **Files modified:** src/cli/new_shell_reminder.rs (test module only)
- **Verification:** Repeated `cargo test --lib -- reminder_` runs all pass; full 524-test suite green.
- **Committed in:** 6b290b3 (part of Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — parallel test race)
**Impact on plan:** Test-infrastructure fix only; production code unchanged. No scope creep.

## Issues Encountered

- `cargo fmt` insisted on collapsing `write_state_file(&path, "")` onto one line after it was formatted multi-line; re-ran `cargo fmt` once to converge. Non-issue.

## TDD Gate Compliance

The plan declared `tdd="true"` on both tasks. Gate sequence verified:

- **Task 1 RED gate (`test` commit):** `1e15bb4 test(16-03): add failing tests for LS-03 language + state + detection helpers` — contains failing Language tests (methods absent; compile error); detection + state-helper one-line wrappers landed together with their tests because separating one-liner wrappers from their tests provides no TDD value and creates churn.
- **Task 1 GREEN gate (`feat` commit):** `1807d77 feat(16-03): add LS-03 capability message and UX-03 platform-aware reminder` — Language methods added; all 11 tests pass.
- **Task 2 (`feat` commit):** `6b290b3 feat(16-03): add emit_new_shell_reminder_once dedup emitter` — Task 2 was committed as test-plus-impl together because the file is brand-new (no existing contract to RED-test against without the module declaration); the test-first discipline was still followed at the author-flow level (tests written before running the impl through the compiler).

No REFACTOR commit was needed — the code is already at the idiomatic-Rust floor that the mirrored `demo.rs` template established.

## Plan-Level Verification (acceptance gates)

- `cargo fmt --check` — exit 0
- `cargo clippy --all-targets -- -D warnings` — exit 0
- `cargo test` — 524 passed / 0 failed
- `cargo test --lib -- ls_capability_message new_shell_reminder is_gnu_ls_present is_ls_capability_acknowledged acknowledge_ls_capability reminder_` — all targeted tests pass (15 new, 0 failures)
- Scope check: `git diff --name-only` lists exactly the five files in frontmatter `files_modified` — no touches to `src/adapter/ls_colors.rs` (16-02), `src/config/shell_integration.rs` (16-04), `src/cli/preflight.rs` (16-05), or `src/cli/{setup,theme,font,config}.rs` (16-06).

## Next Phase / Plan Readiness

**Plan 16-05 (preflight wiring)** can consume:
- `Language::ls_capability_message()` for the preflight printout copy
- `ConfigManager::is_ls_capability_acknowledged()` / `acknowledge_ls_capability()` for the one-time gate
- `detection::is_gnu_ls_present()` for the gls-absent check

**Plan 16-06 (CLI command wiring)** can consume:
- `crate::cli::new_shell_reminder::emit_new_shell_reminder_once(auto, quiet)` called at the tail of `setup` / `theme` (explicit-name branch only, per D-D5) / `font` / `config` (sub-commands that touch shell integration, per D-D2)
- `Typography::explanation(Language::new_shell_reminder())` wrapping pattern already proven by this emitter — if any handler needs to inline the copy instead of going through `emit_*_once`, this is the call site shape.

**Typography import path** for Plan 16-06 to reference: `crate::design::typography::Typography` (same path `src/cli/demo.rs` uses).

No blockers. No stubs. No CLAUDE.md directives (absent in repo).

## Self-Check: PASSED

Verified all claimed deliverables exist:

- `src/cli/new_shell_reminder.rs` — FOUND
- `src/brand/language.rs` — FOUND (modified)
- `src/cli/mod.rs` — FOUND (modified)
- `src/config/tracked_state.rs` — FOUND (modified)
- `src/detection.rs` — FOUND (modified)

Verified all claimed commits exist in `git log --oneline`:

- `1e15bb4` — FOUND
- `1807d77` — FOUND
- `6b290b3` — FOUND

---
*Phase: 16-cli-tool-colors-new-terminal-ux*
*Plan: 03*
*Completed: 2026-04-18*
