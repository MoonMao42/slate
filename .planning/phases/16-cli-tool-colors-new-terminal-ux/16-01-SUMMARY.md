---
phase: 16
plan: 01
subsystem: adapter
tags: [apply-outcome, requires-new-shell, data-model, foundation, d-c3]
dependency-graph:
  requires:
    - "Phase 15 palette/renderer infra (unchanged)"
    - "existing ApplyOutcome::Applied unit variant (now superseded)"
  provides:
    - "ApplyOutcome::Applied { requires_new_shell: bool } struct variant"
    - "ApplyOutcome::applied_no_shell() / applied_needs_new_shell() constructors"
    - "ToolApplyResult.requires_new_shell: bool field"
    - "Every adapter truthfully declares its new-shell signal per D-C3"
  affects:
    - "Plan 16-02 (LsColorsAdapter) — builds on the new struct variant for its own constructor"
    - "Plan 16-04 (registry aggregator + shell-integration wiring) — consumes ToolApplyResult.requires_new_shell"
    - "All 9 ToolApplyStatus::Applied consumer sites — proven untouched by this migration"
tech-stack:
  added: []
  patterns:
    - "Struct-variant data-plus-metadata encoding for enum variants (RESEARCH §Pattern 4)"
    - "Named convenience constructors on data enums (applied_no_shell, applied_needs_new_shell)"
key-files:
  created: []
  modified:
    - "src/adapter/mod.rs"
    - "src/adapter/registry.rs"
    - "src/adapter/starship.rs"
    - "src/adapter/bat.rs"
    - "src/adapter/eza.rs"
    - "src/adapter/lazygit.rs"
    - "src/adapter/zsh_highlight.rs"
    - "src/adapter/fastfetch.rs"
    - "src/adapter/font.rs"
    - "src/adapter/delta.rs"
    - "src/adapter/ghostty.rs"
    - "src/adapter/alacritty.rs"
    - "src/adapter/kitty.rs"
    - "src/adapter/tmux.rs"
    - "src/cli/apply.rs"
    - "src/cli/failure_handler.rs"
decisions:
  - "Struct-variant chosen over sibling-variant (AppliedRequiresNewShell) — locked by RESEARCH §Pattern 4"
  - "Every adapter uses applied_no_shell() / applied_needs_new_shell() constructors rather than inline struct literals for readability"
  - "ToolApplyStatus stays a unit-variant enum — zero churn across 10 consumer sites in cli/apply.rs, cli/failure_handler.rs, cli/setup_executor/integration.rs"
  - "ToolApplyResult.requires_new_shell defaults to false for Skipped/Failed outcomes (no change was made, so no new shell is needed)"
  - "Registry match arm collapsed to a tuple destructure (status, requires_new_shell) so all four branches populate the new field uniformly"
metrics:
  duration: "5m38s"
  completed: "2026-04-18"
---

# Phase 16 Plan 01: ApplyOutcome requires_new_shell Foundation Summary

One-liner: Migrated `ApplyOutcome::Applied` from unit variant to struct variant `{ requires_new_shell: bool }` across all 13 adapter call sites + 1 registry match arm + 2 cli/ test fixtures, laying the Phase 16 foundation for UX-01 new-terminal reminders without disturbing any `ToolApplyStatus` consumer.

## What Shipped

**ApplyOutcome variant shape** — `src/adapter/mod.rs`

```rust
pub enum ApplyOutcome {
    Applied { requires_new_shell: bool },
    Skipped(SkipReason),
}

impl ApplyOutcome {
    pub const fn applied_no_shell() -> Self { Self::Applied { requires_new_shell: false } }
    pub const fn applied_needs_new_shell() -> Self { Self::Applied { requires_new_shell: true } }
}
```

**13 adapter sites migrated** — per the D-C3 declaration matrix:

| Adapter | Call site (approx) | Declaration | Constructor used |
|---------|--------------------|-------------|------------------|
| starship | `starship.rs:254` (post-edit ~256) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| bat | `bat.rs:65` (post-edit ~66) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| eza | `eza.rs:52` (post-edit ~54) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| lazygit | `lazygit.rs:120` (post-edit ~122) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| zsh_highlight | `zsh_highlight.rs:93` (post-edit ~95) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| fastfetch | `fastfetch.rs:72` (post-edit ~74) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| font | `font.rs:297` (post-edit ~300) | `requires_new_shell: true` | `applied_needs_new_shell()` |
| delta | `delta.rs:106` (post-edit ~108) | `requires_new_shell: false` | `applied_no_shell()` |
| ghostty | `ghostty.rs:348` (post-edit ~350) | `requires_new_shell: false` | `applied_no_shell()` |
| alacritty | `alacritty.rs:272` (post-edit ~274) | `requires_new_shell: false` | `applied_no_shell()` |
| kitty | `kitty.rs:264` (post-edit ~266) | `requires_new_shell: false` | `applied_no_shell()` |
| tmux | `tmux.rs:117` (post-edit ~119) | `requires_new_shell: false` | `applied_no_shell()` |
| registry test mock | `registry.rs:175` (post-edit ~189) | `requires_new_shell: false` | inline struct literal |

7 adapters declare `true`, 5 declare `false`, 1 test mock declares `false` — matching the plan's success criteria exactly.

**Registry match arm** — `src/adapter/registry.rs` (lines ~100–120)

The single match site in `apply_theme_with_filter` was extended to destructure the new variant and carry `requires_new_shell` into `ToolApplyResult`:

```rust
let (status, requires_new_shell) = match adapter.is_installed() {
    Ok(false) => (ToolApplyStatus::Skipped(SkipReason::NotInstalled), false),
    Ok(true) => match adapter.apply_theme(theme) {
        Ok(ApplyOutcome::Applied { requires_new_shell }) => (ToolApplyStatus::Applied, requires_new_shell),
        Ok(ApplyOutcome::Skipped(reason)) => (ToolApplyStatus::Skipped(reason), false),
        Err(err) => (ToolApplyStatus::Failed(err), false),
    },
    Err(err) => (ToolApplyStatus::Failed(err), false),
};
ToolApplyResult { tool_name, status, requires_new_shell }
```

`ToolApplyResult` gained a single field: `pub requires_new_shell: bool`.

**ToolApplyStatus untouched** — confirmed via grep and by the absence of any build/test churn in the 10 consumer sites across `cli/apply.rs`, `cli/failure_handler.rs`, and `cli/setup_executor/integration.rs`. `ToolApplyStatus` stays `{ Applied, Skipped(SkipReason), Failed(SlateError) }` — all unit variants.

## D-C3 Rationale (per-adapter)

Each adapter's declaration has a code-level rationale comment above the constructor call:

- **starship / bat / eza / lazygit / zsh_highlight / fastfetch / font** → `requires_new_shell: true` because their effective value flows through shell init (env var export in `env.zsh`/`env.bash`/`env.fish`, sourced script, or font-family picked up at terminal launch). A currently-running shell holds stale values.
- **delta** → `requires_new_shell: false` because delta is invoked fresh per pager call; git reads its config on every invocation.
- **ghostty / alacritty / kitty** → `requires_new_shell: false` because each has a live-reload mechanism (ghostty AppleScript, alacritty `live_config_reload`, kitty `kitten @ set-colors`) that refreshes the current window without a new shell.
- **tmux** → `requires_new_shell: false` because `tmux source-file` against the running server refreshes existing sessions.

## Tests

- `cargo fmt --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --lib` — 431 passed, 0 failed, 0 ignored
- `cargo test` (full suite) — every binary green; integration harnesses (wcag_audit, tmux_seven_elements, themes, etc.) all pass; no tests disabled or ignored.

No new tests were added in this plan by design: it is a data-shape migration whose correctness is established by (a) the compiler enforcing exhaustiveness at all 13 constructor sites and the registry match arm, and (b) the existing adapter-level + registry-level test suite continuing to pass against the new variant shape. Plan 16-04 will add the behavioral tests that assert `requires_new_shell` aggregation produces the correct UX-01 reminder.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking fix] Updated 6 ToolApplyResult literal construction sites in cli/apply.rs + cli/failure_handler.rs**

- **Found during:** Task 1 (during initial build verification)
- **Issue:** The plan identified `src/adapter/registry.rs:175-177` as the only test mock needing updates, but `cargo build --lib` also surfaced 6 additional `ToolApplyResult { ... }` literal construction sites in `src/cli/apply.rs` (test fixture) and `src/cli/failure_handler.rs` (test fixture). Without adding `requires_new_shell` on those literals, the build fails in cli/ test modules, not just at the 12 adapter constructor sites. This contradicts the plan's stated acceptance criterion: "cargo build --lib fails ONLY with missing-field errors at the 12 adapter constructor sites".
- **Fix:** Added `requires_new_shell: false` to each of the 6 `ToolApplyResult` literals in the test fixtures. Two test fixtures under `set_theme_results(...)` in `failure_handler.rs` were varied to `true` for `bat` and `false` for `delta` so the fixture reflects the real D-C3 matrix (harmless but more informative).
- **Files modified:** `src/cli/apply.rs` (1 `#[test]` block), `src/cli/failure_handler.rs` (1 `#[test]` block)
- **Commit:** `0ae9e51` (rolled into Task 1 since the whole migration has to compile together)

### Deliberate Choices (not deviations)

**Use of `applied_no_shell()` / `applied_needs_new_shell()` constructors at every adapter site**

- The plan explicitly permits either inline struct literal or named constructor: "Executor may substitute `ApplyOutcome::applied_needs_new_shell()` or `ApplyOutcome::applied_no_shell()` ... where that reads cleaner — both produce the identical variant." All 12 adapter sites chose the named-constructor form for readability. The in-module `impl ApplyOutcome` block still contains the literal `requires_new_shell: true` and `requires_new_shell: false` tokens that the plan's grep-based acceptance checks look for.
- The single place that keeps an inline struct literal is the registry's `MockAdapter::apply_theme` — tests typically don't benefit from the constructor, and being explicit about the stubbed value is clearer at the point of use.

**Registry match arm shape**

- Plan suggested per-branch `ToolApplyResult` population. I collapsed it to a tuple-destructure `let (status, requires_new_shell) = match ...` so each branch returns `(status, bool)` uniformly and the `ToolApplyResult { .. }` construction happens exactly once. Functionally identical; fewer lines; clippy-friendly.

**Line-number drift**

- The plan lists pre-edit line numbers. After adding rationale comments above each constructor, the post-edit line numbers drift by +1 to +3. Documented in the table above for future archaeological convenience.

### No Rule 4 escalations

No architectural changes were needed. No new DB tables, no framework switches, no infrastructure shifts. This was a pure data-model migration with 14 touch points plus 2 blocking test-fixture fixes.

## Threat Flags

None. This plan touches only in-process enum shape and adapter constructors. No new network endpoints, file-system surfaces, auth paths, or schema changes at trust boundaries.

## Known Stubs

None. Every migrated site emits the new struct variant with a truthful `requires_new_shell` value per D-C3; no placeholders.

## Consumer Readiness for Downstream Plans

- **Plan 16-02 (LsColorsAdapter):** can construct `ApplyOutcome::applied_needs_new_shell()` directly — the constructor is `pub const fn` so usable anywhere.
- **Plan 16-04 (registry aggregator + shell-integration wiring):** can iterate `Vec<ToolApplyResult>` and consult `result.requires_new_shell` to build the `requires_new_shell(results: &[ToolApplyResult]) -> bool` helper + any `ToolApplyStatus::Applied` filter already in `apply.rs`.
- **UX-01 reminder copy:** gate on `requires_new_shell` aggregate + per-platform phrasing (macOS `[⌘N]`, Linux "open a new terminal") is now simply a `.iter().any(|r| r.requires_new_shell)` call away.

## Commits

- `0ae9e51` — feat(16-01): extend ApplyOutcome to struct variant { requires_new_shell }
- `8061ee2` — feat(16-01): declare requires_new_shell per adapter (D-C3 matrix)

## Self-Check: PASSED

- All 14 modified files exist on disk and are git-tracked at their expected paths.
- Both commits `0ae9e51` (Task 1) and `8061ee2` (Task 2) exist on the branch.
- Full `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --lib && cargo test` green at HEAD.
- Negative check: no `ApplyOutcome::Applied` unit-style construction anywhere in `src/`.
- Scope check: `ToolApplyStatus::Applied` remains a unit variant with 10 intact consumer sites in `cli/`.
