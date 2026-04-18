---
phase: 17
plan: 00
subsystem: editor-adapter
tags: [scaffolding, ci, feature-flag, nvim, integration-test]
dependency_graph:
  requires: []
  provides:
    - has-nvim Cargo feature flag
    - src/adapter/nvim module surface
    - tests/nvim_integration.rs harness
    - tests/snapshots directory
    - notify dev-dep available for Plan 07
    - CI nvim install step
  affects:
    - "Cargo.toml [features] / [dev-dependencies]"
    - src/adapter/mod.rs (module table)
    - .github/workflows/ci.yml (test step)
tech_stack:
  added:
    - notify 6 (dev-dep, for Plan 07 fs-event assertions)
    - rhysd/action-setup-vim@v1 (CI action, installs Neovim stable)
  patterns:
    - feature-flag gating via #[cfg(feature = "has-nvim")]
    - empty module skeleton mirrors Phase 16 pattern (ls_colors.rs shape)
key_files:
  created:
    - src/adapter/nvim.rs
    - tests/nvim_integration.rs
    - tests/snapshots/.gitkeep
    - .planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-00-SUMMARY.md
  modified:
    - Cargo.toml
    - Cargo.lock
    - src/adapter/mod.rs
    - .github/workflows/ci.yml
decisions:
  - Sanity test body is empty (not assert!(true)) — clippy's
    assertions-on-constants lint blocks the literal form. Net behavior
    is identical: the test passes by virtue of compiling and running
    without panic.
  - Six #[ignore]d stubs (not seven) match the plan body's verbatim code
    listing. Plan acceptance criteria #1 / #3 mention "7 ignored" but
    the <action> section enumerates only six. Acceptance #2 (named-test
    --list count) and the <done> contract are satisfied by the six-stub
    layout. The 18-variant syntax-gate coverage is delivered by
    loader_lua_parses_via_luafile (covers all 18 spliced PALETTES
    sub-tables) plus nvim_headless_source_all_variants (proves each
    shim sources via :colorscheme).
  - Sorted [dev-dependencies] alphabetically while inserting notify so
    Plan 07's contract is visible at a glance.
metrics:
  duration_minutes: 4
  completed: 2026-04-19
  tasks_completed: 3
  files_created: 4
  files_modified: 4
---

# Phase 17 Plan 00: Wave 0 Scaffolding Summary

Lay Wave 0 scaffolding for Phase 17 (Neovim editor adapter): introduce the
`has-nvim` Cargo feature, register an empty `nvim` adapter module so the
crate compiles, stand up `tests/nvim_integration.rs` with `#[ignore]`d stubs
plus the snapshots directory, and wire the CI `rhysd/action-setup-vim@v1`
install step. No behavior yet — only the skeleton and test-discovery surface
that Waves 1–8 will fill in.

---

## What Shipped

### Cargo / build surface

- `Cargo.toml` gains a new `[features]` block with `default = []` and
  `has-nvim = []`. The flag gates integration tests that shell out to
  `nvim --headless`. CI enables it via `cargo test --features has-nvim`
  after the new install step lands the nvim binary on PATH.
- `Cargo.toml [dev-dependencies]` adds `notify = "6"` so Plan 07 Task 2's
  fs-event assertion (the mandatory direct-event-count test proving D-04's
  "single fs_event fire" contract) can compile against the watcher API
  used by `lua/slate/init.lua` consumers. Existing dev-deps were sorted
  alphabetically while at it (assert_cmd, criterion, notify, predicates,
  rstest).

### Adapter module surface

- `src/adapter/nvim.rs` (14 lines) — empty module skeleton with a
  module-level doc comment outlining the Wave 1–5 fill-in plan, plus a
  `#[cfg(test)] mod tests {}` placeholder. No public types yet; the
  `NvimAdapter` struct lands in Plan 05.
- `src/adapter/mod.rs` registers `pub mod nvim;` alphabetically between
  `marker_block` and `palette_renderer`. No `pub use` re-export yet
  (matches the "struct in Plan 05" timeline).

### Integration test harness

- `tests/nvim_integration.rs` (44 lines) declares one passing sanity test
  (`integration_harness_compiles`) plus six `#[ignore]`d stubs that Plan
  07 fills in:
  - `state_file_atomic_write_single_event` (no nvim binary needed)
  - `nvim_headless_source_all_variants` *(has-nvim)*
  - `watcher_debounces_multi_fire` *(has-nvim)*
  - `lualine_refresh_fires` *(has-nvim)*
  - `marker_block_lua_comment_regression` *(has-nvim)*
  - `loader_lua_parses_via_luafile` *(has-nvim)*
- `tests/snapshots/.gitkeep` (empty file) — landing pad for future
  `cargo insta` snapshots.

### CI

- `.github/workflows/ci.yml` inserts an `Install Neovim` step
  (`rhysd/action-setup-vim@v1`, `neovim: true`, `version: stable`)
  between `Swatinem/rust-cache@v2` and `Check formatting`.
- The `Test` step changes from `cargo test --locked --quiet` to
  `cargo test --locked --quiet --features has-nvim` so the gated
  integration tests execute on both macos-latest and ubuntu-latest
  runners once their bodies land.

---

## Verification Evidence

| Check | Command | Result |
|-------|---------|--------|
| Crate compiles (default features) | `cargo check` | clean |
| Crate compiles (all features) | `cargo check --all-features` | clean |
| Crate builds (has-nvim) | `cargo build --features has-nvim` | clean |
| Default test run | `cargo test --test nvim_integration` | 1 passed, 1 ignored |
| Featured test discovery | `cargo test --test nvim_integration --features has-nvim` | 1 passed, 6 ignored |
| Forced ignored execution | `cargo test --test nvim_integration --features has-nvim -- --ignored` | 6 passed |
| Lint gate | `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| Format gate | `cargo fmt --all -- --check` | clean |

CI YAML invariants verified locally:

- `rhysd/action-setup-vim@v1` line count: 1
- `neovim: true` line count: 1
- `--features has-nvim` line count: 1
- `Install Neovim` step appears strictly before any `cargo test`
  invocation (verified via `awk` ordering check).

---

## Per-Task Commits

| Task | Name                                                    | Commit    | Files                                                                  |
| ---- | ------------------------------------------------------- | --------- | ---------------------------------------------------------------------- |
| 1    | has-nvim feature + notify dev-dep + nvim module         | `9981ade` | Cargo.toml, Cargo.lock, src/adapter/mod.rs, src/adapter/nvim.rs        |
| 2    | nvim integration harness + snapshots directory          | `b37c34a` | tests/nvim_integration.rs, tests/snapshots/.gitkeep                    |
| 3    | CI Neovim install + has-nvim feature flag in test step  | `e7de8d8` | .github/workflows/ci.yml                                               |

---

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Replaced `assert!(true)` with empty test body in `integration_harness_compiles`**

- **Found during:** Task 2 (clippy gate before commit)
- **Issue:** Plan body specified `assert!(true);` inside the sanity test.
  Repository policy (user preference) requires
  `cargo clippy --all-targets --all-features -- -D warnings` to pass
  before every commit; clippy's `assertions-on-constants` lint
  (default-warn, promoted to error by `-D warnings`) blocks the literal
  form.
- **Fix:** Removed the assertion; left the function body empty with an
  inline comment explaining the lint context. The test still passes —
  cargo's harness considers a test successful when the function returns
  without panic. Net behavior is identical to `assert!(true)`.
- **Files modified:** `tests/nvim_integration.rs`
- **Commit:** `b37c34a`

### Plan Inconsistencies Noted (no code change)

- **Plan acceptance #1 / #3** mention "7 ignored" stubs, but the plan's
  `<action>` section enumerates exactly six `#[ignore]` blocks (the
  18-variant syntax-gate test was intentionally dropped per the plan's
  own annotation in acceptance #2). The implementation matches the
  verbatim `<action>` listing — six `#[ignore]`d stubs plus one passing
  sanity test = seven total tests. Acceptance #2 (the named-test
  `--list` count) and `<done>` (six-named stubs in `:colorscheme` /
  `luafile` coverage) are satisfied. No code change made; this summary
  records the plan-internal inconsistency for the verifier and for
  Plan 07's authors.

### Out of Scope (deferred for later phase)

- The CI `Clippy` step (`.github/workflows/ci.yml` line 36) still runs
  `cargo clippy --all-targets -- -D warnings` without `--all-features`,
  meaning `has-nvim`-gated code paths are not lint-checked on CI. This
  is harmless today (the gated module is a stub) but should be revisited
  once Plan 05 adds real `NvimAdapter` code. Not modified in this plan
  to keep the diff scoped to the three documented tasks.

---

## Authentication Gates

None.

---

## Known Stubs

The whole plan is a scaffolding plan — by design every meaningful test
body is `#[ignore]`d and every adapter function is absent. This is the
plan's contract, documented in the Wave 0 README:

| Stub | File | Resolves in |
|------|------|-------------|
| Empty `mod tests {}` block | `src/adapter/nvim.rs` | Waves 2+ (per file header) |
| 6 `#[ignore]`d test stubs | `tests/nvim_integration.rs` | Plan 17-07 (per `#[ignore = "Plan 07 — …"]` annotations) |
| Empty `tests/snapshots/.gitkeep` | `tests/snapshots/` | Filled by `cargo insta` runs in Wave 2+ |
| `has-nvim` feature with no consumers | `Cargo.toml [features]` | Waves 1+ (via `#[cfg(feature = "has-nvim")]` on tests + future build.rs probing per RESEARCH §Environment Availability) |

All four are *intentional* per the plan's `<objective>`: "No behavior yet
— only the skeleton and the test-discovery surface Waves 1-8 will fill
in." None blocks the plan goal.

---

## Self-Check: PASSED

**Created files exist:**
- `src/adapter/nvim.rs` — FOUND
- `tests/nvim_integration.rs` — FOUND
- `tests/snapshots/.gitkeep` — FOUND
- `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-00-SUMMARY.md` — FOUND (this file)

**Modified files reflect changes:**
- `Cargo.toml` — `[features] has-nvim = []` present, `notify = "6"` present
- `src/adapter/mod.rs` — `pub mod nvim;` present
- `.github/workflows/ci.yml` — `rhysd/action-setup-vim@v1` and `--features has-nvim` present

**Commits exist:**
- `9981ade` — `feat(17-00): add has-nvim feature flag and nvim adapter skeleton`
- `b37c34a` — `test(17-00): scaffold nvim integration harness with ignored stubs`
- `e7de8d8` — `ci(17-00): install Neovim and enable has-nvim feature in CI`

All git log entries verified via `git log --oneline -5`.
