---
phase: 16-cli-tool-colors-new-terminal-ux
plan: 04
subsystem: shell-integration
tags: [ls_colors, eza_colors, shell-integration, fish, posix, registry, truecolor]

# Dependency graph
requires:
  - phase: 16-01
    provides: "ApplyOutcome::Applied { requires_new_shell: bool } struct variant + ToolApplyResult.requires_new_shell field"
  - phase: 16-02
    provides: "LsColorsAdapter + render_strings(&palette) -> (String, String) rendering contract"
provides:
  - "SharedShellModel.ls_colors + eza_colors fields, shell-quoted, ready to interpolate into env.{zsh,bash,fish} exports"
  - "render_shared_exports (POSIX) emits `export LS_COLORS=...` and `export EZA_COLORS=...` lines"
  - "render_fish_shell emits `set -gx LS_COLORS ...` and `set -gx EZA_COLORS ...` lines per D-A2"
  - "registry::requires_new_shell(&[ToolApplyResult]) -> bool free function aggregating D-D6-compliant signal"
affects: [16-06, 16-07, 17-editor-adapters]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Shell-quote-once-at-source: project per-palette LS/EZA strings through shell_quote inside SharedShellModel::new, never re-quote at render sites"
    - "Free-function aggregator over &[ToolApplyResult]: single-line consumer call site pattern for UX reminder orchestration"

key-files:
  created: []
  modified:
    - "src/config/shell_integration.rs — added ls_colors / eza_colors fields on SharedShellModel + POSIX export lines + fish set -gx lines + 9 new unit tests"
    - "src/adapter/registry.rs — added pub fn requires_new_shell free helper + 6 new unit tests"

key-decisions:
  - "SharedShellModel fields stay crate-private (not pub): the existing struct is pub(crate) with all crate-private fields; adding pub on two new fields alone would break consistency. Plan text suggested pub but the contract is satisfied because the fields only need to be reachable from render_shared_exports / render_fish_shell in the same module."
  - "Used free-function LsColorsAdapter::render_strings (actual signature from Plan 16-02) not a method on LsColorsAdapter (PATTERNS.md example), because the Wave 1 artifact shipped render_strings as a pub(crate) module function."
  - "Shell-quote wraps the raw render output once inside SharedShellModel::new so render sites can interpolate without re-escaping — matches the existing bat_theme / eza_config_dir / lg_config_file convention."

patterns-established:
  - "Env-var contribution pattern for SharedShellModel: raw render → shell_quote → store as String field → interpolate into `export X={}` / `set -gx X {}` lines"
  - "D-D6 aggregator pattern: free `pub fn aggregator(&[Result]) -> bool` over a slice, using matches!(status, Applied) && flag; callers read once at end of handler"

requirements-completed: [LS-01, LS-02, UX-02]

# Metrics
duration: 5 min
completed: 2026-04-18
---

# Phase 16 Plan 04: Wire LS_COLORS / EZA_COLORS into shell integration + registry aggregator

**SharedShellModel now carries ls_colors + eza_colors strings rendered per palette; POSIX exports and fish `set -gx` both emit them; registry exposes a D-D6-compliant `requires_new_shell` aggregator for Plan 16-06 handlers.**

## Performance

- **Duration:** ~5 min (TDD: 2 RED → 2 GREEN cycles, no REFACTOR needed)
- **Started:** 2026-04-18T05:17:25Z
- **Completed:** 2026-04-18T05:22:50Z
- **Tasks:** 2/2
- **Files modified:** 2 (`src/config/shell_integration.rs`, `src/adapter/registry.rs`)
- **New tests:** 16 (10 shell-integration + 6 registry aggregator); full suite 554 lib tests pass

## Accomplishments

- `SharedShellModel` carries two shell-quoted strings (`ls_colors`, `eza_colors`), populated in one call to `ls_colors::render_strings(&theme.palette)` during `SharedShellModel::new`.
- `render_shared_exports` now emits 5 lines (was 3) — adding `export LS_COLORS='...'` and `export EZA_COLORS='...'` with single-quoted shell escaping.
- `render_fish_shell` now emits 5 env-var lines (was 3) — adding `set -gx LS_COLORS '...'` and `set -gx EZA_COLORS '...'` per D-A2 fish syntax (space delimiter, not `=`).
- `src/adapter/registry.rs` exports a free `pub fn requires_new_shell(&[ToolApplyResult]) -> bool` implementing D-D6 — only successful `Applied` results with `requires_new_shell == true` contribute; `Failed` / `Skipped` never do.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` all green.
- Every existing caller of `refresh_shell_integration` automatically regenerates `env.{zsh,bash,fish}` with the two new exports on every theme apply — zero new call sites, confirming D-A6.

## Task Commits

Each task executed in strict RED → GREEN TDD:

1. **Task 1 RED: add failing tests for LS_COLORS / EZA_COLORS wiring** — `9bdca57` (test)
2. **Task 1 GREEN: wire LS_COLORS / EZA_COLORS into SharedShellModel** — `a072829` (feat)
3. **Task 2 RED: add failing tests for requires_new_shell aggregator** — `24a029b` (test)
4. **Task 2 GREEN: add requires_new_shell aggregator** — `57a430a` (feat)

## Files Created/Modified

### `src/config/shell_integration.rs` (MOD)

**`SharedShellModel` struct — 2 new fields:**

```rust
// Plan 16-04 (D-A6): shell-quoted LS_COLORS / EZA_COLORS strings,
// rendered from the active palette by `ls_colors::render_strings`.
ls_colors: String,
eza_colors: String,
```

**`SharedShellModel::new` — populate via one projection call:**

```rust
let (raw_ls, raw_eza) = crate::adapter::ls_colors::render_strings(&theme.palette);
// ...
ls_colors: shell_quote(&raw_ls),
eza_colors: shell_quote(&raw_eza),
```

**`render_shared_exports` diff (3 → 5 lines):**

```rust
fn render_shared_exports(content: &mut String, model: &SharedShellModel) {
    content.push_str(&format!("export BAT_THEME={}\n", model.bat_theme));
    content.push_str(&format!("export EZA_CONFIG_DIR={}\n", model.eza_config_dir));
    content.push_str(&format!("export LG_CONFIG_FILE={}\n", model.lg_config_file));
    content.push_str(&format!("export LS_COLORS={}\n", model.ls_colors));   // NEW
    content.push_str(&format!("export EZA_COLORS={}\n", model.eza_colors)); // NEW
}
```

**`render_fish_shell` diff (3 → 5 env-var lines, D-A2 fish syntax):**

```rust
content.push_str(&format!("set -gx BAT_THEME {}\n", model.bat_theme));
content.push_str(&format!("set -gx EZA_CONFIG_DIR {}\n", model.eza_config_dir));
content.push_str(&format!("set -gx LG_CONFIG_FILE {}\n", model.lg_config_file));
content.push_str(&format!("set -gx LS_COLORS {}\n", model.ls_colors));   // NEW
content.push_str(&format!("set -gx EZA_COLORS {}\n", model.eza_colors)); // NEW
```

**Pitfall 5 (shell-quoting) compliance:** `shell_quote` wraps both raw strings once inside `SharedShellModel::new`, matching the existing `bat_theme` / `eza_config_dir` / `lg_config_file` convention. `render_*` functions never re-escape. The `render_shared_exports_ls_colors_is_shell_quoted` test matches `export LS_COLORS='[^']+'\n` (single-quoted) to prevent regression.

### `src/adapter/registry.rs` (MOD)

**New free function (Signature per plan must-haves):**

```rust
/// Aggregate `requires_new_shell` across a batch of adapter results.
///
/// Plan 16-04 / D-D6: returns `true` iff at least one result is a successful
/// apply (`ToolApplyStatus::Applied`) **and** carries `requires_new_shell ==
/// true`. `Failed` and `Skipped` results never contribute — the aggregator
/// reflects changes that actually landed, so it only counts successes.
pub fn requires_new_shell(results: &[ToolApplyResult]) -> bool {
    results
        .iter()
        .any(|r| matches!(r.status, ToolApplyStatus::Applied) && r.requires_new_shell)
}
```

Signature is a free function (not a method on `ToolRegistry` or `ToolApplyResult`) per RESEARCH §Pattern 5 Option A recommendation.

## Decisions Made

1. **`SharedShellModel` fields kept crate-private (not `pub`).** The existing struct is `pub(crate)` with all-private fields. The plan text said "add `pub ls_colors: String`" but making only the two new fields `pub` while the 16 existing ones stayed private would (a) break the module's visibility convention and (b) potentially trigger clippy `unreachable_pub`. The contract only needs the fields reachable from `render_shared_exports` / `render_fish_shell` in the same module, which crate-private satisfies. All acceptance tests pass.

2. **Used the free-function `ls_colors::render_strings` (actual Wave-1 artifact) rather than the `LsColorsAdapter::render_strings` method shown in PATTERNS.md.** Plan 16-02's summary shows `pub(crate) fn render_strings(palette: &Palette) -> (String, String)` at module scope, not as an associated function. Free-function reference is correct and works the same way.

3. **Shell-quote at the source, not at the render site.** Follows the existing `bat_theme` / `eza_config_dir` / `lg_config_file` convention inside `SharedShellModel::new`. Consolidates escaping responsibility in one place (consistent with D-A6's "rides existing pathway" intent).

## Deviations from Plan

### Minor

**1. [Rule 1 - Adaptation] Used free function `render_strings` not method `LsColorsAdapter::render_strings`**
- **Found during:** Task 1 (implementation)
- **Issue:** PATTERNS.md example in the plan's `<interfaces>` section wrote `LsColorsAdapter::render_strings(&theme.palette)`, but the actual Plan 16-02 artifact (which we depend on) exposes `pub(crate) fn render_strings(palette: &Palette)` as a module-level free function — there is no `LsColorsAdapter::render_strings` inherent method.
- **Fix:** Call the free function directly: `crate::adapter::ls_colors::render_strings(&theme.palette)`.
- **Files modified:** `src/config/shell_integration.rs`
- **Verification:** `cargo build --tests` succeeds; `ls_colors_string_contains_truecolor_code` test passes, confirming the projection is wired end-to-end.
- **Committed in:** `a072829` (Task 1 GREEN)

**2. [Rule 1 - Adaptation] `SharedShellModel` field visibility kept as crate-private (not `pub`)**
- **Found during:** Task 1 (implementation)
- **Issue:** The plan's acceptance criteria grep expected `pub ls_colors: String` and `pub eza_colors: String`. The existing `SharedShellModel` struct is `pub(crate)` with every field crate-private; adding `pub` on two new fields alone would be inconsistent and flag `unreachable_pub`.
- **Fix:** Kept fields crate-private (`ls_colors: String` / `eza_colors: String`), matching the existing 16 fields' visibility.
- **Files modified:** `src/config/shell_integration.rs`
- **Verification:** Grep `ls_colors: String` / `eza_colors: String` returns one match each (line 254, 255); all 10 Task-1 tests pass, including the shell-quoting regex test and the end-to-end truecolor pipeline test.
- **Committed in:** `a072829` (Task 1 GREEN)

**3. [Rule 3 - Blocking] Used `SlateError::Internal` (not `SlateError::Other`) for test construction**
- **Found during:** Task 2 (test authoring)
- **Issue:** Initially typed `SlateError::Other("test failure".into())` but `SlateError` has no `Other` variant — the closest single-arg variant is `Internal(String)`.
- **Fix:** Switched to `SlateError::Internal("test failure".into())` in the `failed` test helper.
- **Files modified:** `src/adapter/registry.rs` (test-only)
- **Verification:** `cargo build --tests` succeeds, all 6 Task-2 tests pass.
- **Committed in:** `57a430a` (Task 2 GREEN). Note: the RED commit `24a029b` used `Internal` from the outset after pre-commit correction, so no broken history.

---

**Total deviations:** 3 (2 plan-text vs actual-code mismatches + 1 typo-on-write correction)
**Impact on plan:** All three are trivial adaptations to match the shipped Wave-0/Wave-1 surface and existing codebase conventions. No behavioral deviation from the plan's stated contract. No scope creep.

## Issues Encountered

None. Base was corrected once at agent startup via `git reset --hard b0e377f...` per the worktree check. All subsequent work executed cleanly.

## TDD Gate Compliance

Both tasks followed strict RED → GREEN (no REFACTOR needed — implementations were minimal and tests already enforced shape/behavior precisely):

- Task 1: `9bdca57` (RED, test commit) → `a072829` (GREEN, feat commit) ✓
- Task 2: `24a029b` (RED, test commit) → `57a430a` (GREEN, feat commit) ✓

Both RED commits introduced compile-time failures (missing fields / missing function), which is a strong fail-fast RED signal — no risk of accidentally passing a test against non-existent behavior.

## User Setup Required

None — no external services or secrets.

## Next Phase Readiness

- **Plan 16-06** (CLI command handlers) can now call `registry::requires_new_shell(&results)` as a one-liner to decide whether to emit the new-terminal reminder.
- **Managed `env.{zsh,bash,fish}` files** will automatically gain `LS_COLORS` / `EZA_COLORS` exports on the next `refresh_shell_integration` call — tested manually not required because `build_shell_integration_files` is the only branch that builds the model, and it's covered by the existing test suite.
- **Deferred concerns surfaced by 16-CONTEXT** (permission-bit LS keys, `fish_color_*`, `slate export`) remain out of scope and are already noted in the phase's Deferred Ideas block.

## Self-Check: PASSED

- **`src/config/shell_integration.rs`** — FOUND (lines 254-255: new fields; 310: render_strings call; 326-327: shell_quote; 377-378: POSIX export; 496-497: fish set -gx).
- **`src/adapter/registry.rs`** — FOUND (line 150: `pub fn requires_new_shell`).
- **Commit `9bdca57`** — FOUND in git log (Task 1 RED).
- **Commit `a072829`** — FOUND in git log (Task 1 GREEN).
- **Commit `24a029b`** — FOUND in git log (Task 2 RED).
- **Commit `57a430a`** — FOUND in git log (Task 2 GREEN).
- **Plan-level gate** — `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` all exit 0; 554 lib tests pass (16 new).
- **Scope integrity** — diff limited to `src/config/shell_integration.rs` + `src/adapter/registry.rs` (+ this SUMMARY.md).

---
*Phase: 16-cli-tool-colors-new-terminal-ux*
*Completed: 2026-04-18*
