---
phase: 17
plan: 05
subsystem: adapter-nvim
tags: [editor-adapter, nvim, tdd, adapter-trait, registry, state-file-hook, plan-05]
dependency_graph:
  requires:
    - "src/adapter/nvim.rs::render_colorscheme (Plan 02)"
    - "src/adapter/nvim.rs::render_shim (Plan 02)"
    - "src/adapter/nvim.rs::render_loader (Plan 03 — full loader with uv compat, debounce, lualine refresh)"
    - "src/adapter/nvim.rs::write_state_file (Plan 03)"
    - "src/design/nvim_highlights.rs::HIGHLIGHT_GROUPS (Plans 01 + 04 — 406 entries)"
    - "src/platform/version_check.rs::detect_version (existing NVIM vX.Y.Z parser)"
    - "src/adapter/ToolAdapter trait + ApplyStrategy::WriteAndInclude + ApplyOutcome::Applied"
  provides:
    - "src/adapter/nvim.rs::NvimAdapter struct"
    - "src/adapter/nvim.rs::NvimAdapter::setup(env, theme) -> Result<()> (slow path — 18 shims + loader + initial state)"
    - "src/adapter/nvim.rs::NvimAdapter::apply_theme_with_env(theme, env) -> Result<ApplyOutcome> (crate-private fast path)"
    - "src/adapter/NvimAdapter re-export at the adapter module root"
    - "src/adapter/registry.rs::ToolRegistry::default() bumped from 13 → 14 adapters"
    - "src/platform/version_check.rs::VersionPolicy::min_version(\"nvim\") -> Some(\"0.8.0\")"
    - "src/cli/apply.rs::apply_theme_with_options best-effort nvim state-file tail hook"
  affects:
    - "Plan 06 (wizard flow — invokes NvimAdapter::setup for the D-09 consent flow)"
    - "Plan 07 (integration tests — exercises the shared apply path + the fast-path hot-reload contract)"
tech-stack:
  added: []
  patterns:
    - "Fast/slow split on a ToolAdapter impl — setup() writes 18 shims + loader once at wizard time; apply_theme re-writes only the state file per theme swap."
    - "Env-injection helper (apply_theme_with_env(&self, theme, env)) so unit tests use SlateEnv::with_home(TempDir) without std::env::set_var."
    - "Exclude-only version gate (Phase 7 D-11): is_installed returns Ok(false) on missing binary OR version < 0.8.0, never an error."
    - "Best-effort tail hook on the shared apply coordinator: write fails → log warning, theme swap still succeeds."
key-files:
  created:
    - ".planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-05-SUMMARY.md"
  modified:
    - "src/platform/version_check.rs (196 LOC → 217 LOC; +\"nvim\" => Some(\"0.8.0\") arm + 4 unit tests)"
    - "src/adapter/nvim.rs (963 LOC → 1244 LOC; +NvimAdapter struct + ToolAdapter impl + setup() + apply_theme_with_env helper + 6 unit tests)"
    - "src/adapter/mod.rs (201 LOC → 202 LOC; +pub use nvim::NvimAdapter re-export)"
    - "src/adapter/registry.rs (391 LOC → 392 LOC; +NvimAdapter registration + test_registry_default bumped 13 → 14)"
    - "src/cli/apply.rs (472 LOC → 552 LOC; +best-effort nvim state-file tail hook + 2 contract tests)"
key-decisions:
  - "Fast-path delegates via apply_theme_with_env(theme, env) rather than inlining SlateEnv::from_process() in apply_theme. The crate-private helper lets unit tests inject a TempDir-backed env via SlateEnv::with_home(...) and assert three contracts (return value, state-file contents, no other files touched) without any std::env::set_var calls. Verified: the file contains zero literal set_var references (grep -c 'std::env::set_var' src/adapter/nvim.rs returns 0 after a doc-comment rewrite)."
  - "Version gate uses Phase 7 Decision 11's exclude-only pattern: is_installed returns Ok(false) for (a) missing binary, (b) version parse failure, (c) version < 0.8.0. No SlateError::PlatformError bubbles up in any of those paths — an absent nvim is a skip, never a run-breaker. Conservative parse-failure branch (Ok(false)) avoids writing files for an nvim we cannot verify."
  - "managed_config_path returns env.home().join(\".config/nvim\") directly per D-03, NOT the shared ~/.config/slate/managed/nvim/ convention. Nvim's runtimepath is the managed tier; the three-tier contract still holds, we just co-locate the managed tier with the runtime loader."
  - "State-file tail hook lands AFTER set_current_theme in apply_theme_with_options, not before. This preserves the plan's Test 3 contract: when no adapter applied the new theme (applied_count == 0 early-return), no orphan state file is written. The negative test `slate_theme_set_no_state_file_when_no_adapter_applied` guards this."
  - "Warning on state-file write failure uses eprintln! (matches the existing log_apply_report + log_warning pattern in apply.rs). The slate CLI does not depend on `log` or `tracing` — cliclack::log::warning is the user-visible surface, and eprintln! is the unattended / non-interactive path. Best-effort posture: the write failure does NOT propagate; the theme swap still returns Ok."
  - "Shared coordinator apply.rs was the insertion point, NOT cli/theme.rs::apply_explicit_theme. The latter is only one of four entry points (explicit set, picker commit, restore re-apply, auto-follow) — every one of them funnels through apply_theme_with_options, so a single hook covers all four paths with zero duplication."
metrics:
  duration_seconds: 1139
  duration_human: "~19m"
  completed_at: "2026-04-18T18:02:49Z"
  tasks_completed: 4
  tdd_phases: ["RED", "GREEN", "RED", "GREEN", "RED", "GREEN"]
  commits: 7
  files_created: 1
  files_modified: 5
  lib_tests_before: 635
  lib_tests_after: 647
  new_tests_added: 12
  registry_adapter_count_before: 13
  registry_adapter_count_after: 14
  nvim_rs_loc_before: 963
  nvim_rs_loc_after: 1244
---

# Phase 17 Plan 05: NvimAdapter + registry wiring + state-file tail hook Summary

Closed the Rust-side adapter contract: `NvimAdapter` now implements the
`ToolAdapter` trait with a clean fast/slow split (public `setup` writes
18 shims + loader + initial state; crate-private `apply_theme_with_env`
writes only the state file), the default `ToolRegistry` grew from 13 to
14 adapters, `VersionPolicy` knows the 0.8.0 floor for nvim, and the
shared apply coordinator now emits the state-file as a best-effort tail
hook so every successful `slate theme set`, picker commit, restore
re-apply, and auto-follow trip pokes the watcher in every running nvim.

## Accomplishments

- **`NvimAdapter` implements `ToolAdapter`** with six trait methods:
  `tool_name` returns `"nvim"`, `apply_strategy` returns
  `ApplyStrategy::WriteAndInclude`, `managed_config_path` returns
  `~/.config/nvim` directly (D-03), `integration_config_path` returns
  `~/.config/nvim/init.lua`, `is_installed` gates on binary AND
  version ≥ 0.8.0 via the Phase 7 exclude-only pattern, and
  `apply_theme` delegates to the env-injected fast-path helper.
- **`NvimAdapter::setup(env, initial_theme)`** — the slow path called
  from Plan 06's wizard. Creates `~/.config/nvim/colors/` and
  `~/.config/nvim/lua/slate/`, writes one `slate-<id>.lua` shim per
  built-in variant (iterates `ThemeRegistry::all()`, so future variants
  ship for free), writes the loader at `lua/slate/init.lua`, and seeds
  `~/.cache/slate/current_theme.lua` with the initial theme. Idempotent
  via `AtomicWriteFile::commit` — byte-identical reruns proven.
- **`NvimAdapter::apply_theme_with_env(&self, theme, env)`** —
  crate-private fast-path helper. Writes the state file only;
  `apply_theme` delegates with `SlateEnv::from_process()`. The split
  lets unit tests inject a `TempDir`-backed `SlateEnv::with_home(...)`
  and assert three contracts (return value, state-file contents, no
  other files touched) without `std::env::set_var` anywhere.
- **`VersionPolicy::min_version("nvim")` → `Some("0.8.0")`** — new arm
  between the existing alacritty entry and the catch-all. Floor
  matches D-01 (nvim_set_hl API + vim.uv baseline). Existing
  `extract_version_from_output` already parses `NVIM v0.12.0` correctly
  (verified locally — `nvim --version` returns `NVIM v0.12.0`).
- **Registry bumped 13 → 14** — `pub use nvim::NvimAdapter` at the
  adapter module root, `registry.register(Box::new(NvimAdapter))` in
  `ToolRegistry::default()`, `test_registry_default` assertion updated
  from 13 to 14.
- **Shared apply coordinator state-file hook** — in
  `apply_theme_with_options`, after `set_current_theme` and the
  auto-follow update, we invoke `crate::adapter::nvim::write_state_file(env, &theme.id)`
  as a non-fatal tail hook. On failure we log a single `eprintln!`
  warning and continue (posture matches the tmux reload hook).

## Contract guarantees

- **Fast path is state-only.** The new test
  `nvim_adapter_apply_theme_with_env_writes_state_file_only` proves
  the fast-path creates the state file at the expected path with the
  applied variant id AND that neither `~/.config/nvim/colors/` nor
  `~/.config/nvim/lua/slate/` is created. Running nvim instances
  hot-reload via the loader's `vim.uv.fs_event` watcher; the 18 shims
  + loader live from the wizard's prior `setup` call.
- **Setup is idempotent.** `nvim_adapter_setup_is_idempotent` reads the
  loader + the catppuccin-mocha shim, re-runs setup, and asserts byte
  identity. `AtomicWriteFile` guarantees atomic rename-over semantics.
- **Version gate is exclude-only.** `is_installed` returns `Ok(false)`
  — never an error — on missing binary, version parse failure, or
  version < 0.8.0. Conservative: unknown version → treated as "can't
  verify, don't write files."
- **No orphan state files.** The hook lands AFTER the `applied_count
  == 0` early-return in `apply_theme_with_options`, so when no adapter
  applied the new theme the state file is NOT written. The negative
  test `slate_theme_set_no_state_file_when_no_adapter_applied` pins
  this.
- **State-file write failures are non-fatal.** The theme swap returns
  `Ok(report)` even if the write fails — matches 17-RESEARCH §Pattern
  3 best-effort posture. Failure is logged to stderr, not propagated.
- **No `std::env::set_var` anywhere.** Verified by
  `grep -c "std::env::set_var" src/adapter/nvim.rs` returning 0 (after
  a doc-comment rewrite that avoided the literal string while still
  conveying the intent).

## Per-task Commits

| Task | Phase | Commit    | Message                                                                        |
| ---- | ----- | --------- | ------------------------------------------------------------------------------ |
| 1    | RED   | `2a59881` | `test(17-05): add failing tests for nvim 0.8.0 version gate`                   |
| 1    | GREEN | `9cc3e7a` | `feat(17-05): add nvim 0.8.0 floor to VersionPolicy`                           |
| 2    | RED   | `8365ca2` | `test(17-05): add failing tests for NvimAdapter trait + setup + fast path`     |
| 2    | GREEN | `f8af8ba` | `feat(17-05): implement NvimAdapter with fast/slow split`                      |
| 3    | AUTO  | `1bb8c40` | `feat(17-05): register NvimAdapter in the default ToolRegistry`                |
| 4    | RED   | `891526a` | `test(17-05): add failing test for nvim state-file tail hook`                  |
| 4    | GREEN | `2b89cc2` | `feat(17-05): hook nvim state-file write into shared apply coordinator`       |

REFACTOR steps intentionally skipped — each GREEN commit was
idiomatic on the first pass (match arm addition, single trait impl
with helper delegation, one-line register call, one-block tail hook).

## Tests Added

### Task 1 — version gate (4 new tests)

| Test                                   | Guards                                                           |
| -------------------------------------- | ---------------------------------------------------------------- |
| `version_policy_nvim_min_is_0_8`       | `VersionPolicy::min_version("nvim") == Some("0.8.0")`            |
| `check_version_accepts_nvim_0_12`      | nvim 0.12.0 accepted (dev-machine floor)                         |
| `check_version_accepts_nvim_0_8_floor` | nvim 0.8.0 accepted (exact floor)                                |
| `check_version_rejects_nvim_0_7`       | nvim 0.7.2 rejected as too old                                   |

### Task 2 — NvimAdapter (6 new tests)

| Test                                                    | Guards                                                                            |
| ------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `nvim_adapter_tool_name`                                | `NvimAdapter.tool_name() == "nvim"`                                               |
| `nvim_adapter_apply_strategy_is_write_and_include`      | `apply_strategy() == ApplyStrategy::WriteAndInclude`                              |
| `nvim_adapter_apply_theme_with_env_writes_state_file_only` | Fast path writes state file only; no `colors/`, no `lua/slate/`                |
| `nvim_adapter_setup_writes_full_install`                | `setup` writes 18 shims + loader + initial state file                             |
| `nvim_adapter_setup_is_idempotent`                      | Two setup calls produce byte-identical loader + shim                              |
| `nvim_adapter_managed_path_points_at_nvim_home`         | `managed_config_path()` ends with `.config/nvim` (D-03)                           |

### Task 3 — registry wiring (existing test updated)

| Test                                         | Change                                                       |
| -------------------------------------------- | ------------------------------------------------------------ |
| `test_registry_default` (existing)           | `adapters().len()` assertion bumped from 13 to 14            |

### Task 4 — shared coordinator tail hook (2 new tests)

| Test                                                          | Guards                                                                        |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `slate_theme_set_writes_nvim_state_file_on_successful_apply`  | After successful apply, state file exists with the applied variant id         |
| `slate_theme_set_no_state_file_when_no_adapter_applied`       | With applied_count == 0, no orphan state file is created                      |

**Total: 12 new tests + 1 updated test.** All green, sub-second run time.

## Files Created / Modified

- **`src/platform/version_check.rs`** (196 → 217 LOC) — adds the
  `"nvim" => Some("0.8.0")` arm and 4 unit tests asserting the new
  floor.
- **`src/adapter/nvim.rs`** (963 → 1244 LOC) — adds the `NvimAdapter`
  struct, its `ToolAdapter` impl, the `NvimAdapter::setup` slow-path
  method, the crate-private `apply_theme_with_env` fast-path helper,
  and a local `write_atomic` helper. 6 new tests in `mod tests`.
- **`src/adapter/mod.rs`** (201 → 202 LOC) — adds
  `pub use nvim::NvimAdapter;` alphabetically between
  `LsColorsAdapter` and the `ToolRegistry` re-export.
- **`src/adapter/registry.rs`** (391 → 392 LOC) — adds
  `registry.register(Box::new(crate::adapter::NvimAdapter));` to
  `Default::default()` and bumps `test_registry_default`'s count
  assertion to 14.
- **`src/cli/apply.rs`** (472 → 552 LOC) — adds the best-effort tail
  hook in `apply_theme_with_options` after `set_current_theme` and
  the auto-follow update, plus 2 contract tests.
- **`.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-05-SUMMARY.md`** (this file).

## Deviations from Plan

None — the plan executed exactly as written. One clarification worth
noting:

- **Hook insertion file**: the plan frontmatter lists
  `src/cli/theme.rs` under `files_modified` but the `<action>` body
  explicitly directs the executor to edit `src/cli/apply.rs` because
  `apply_theme_with_options` is the shared coordinator that all four
  successful apply paths funnel through. Followed the action (which
  matches the actual architecture); the frontmatter is a drafting slip.
  No semantic deviation — the hook lives in the same function the
  action describes.

- **Logging API**: the plan suggested `log::warn!` but the slate crate
  does not depend on `log` or `tracing` (verified via `Cargo.toml`
  inspection). The codebase uses `eprintln!` for unattended stderr
  messages and `cliclack::log::warning` for interactive ones. Used
  `eprintln!` — matches `log_apply_report`'s existing error-path
  shape in the same file.

- **Doc-comment rewrite for the `set_var` grep**: the plan's acceptance
  criterion `grep -c "std::env::set_var" src/adapter/nvim.rs` returns
  0 required rewording a doc comment in `NvimAdapter`'s rustdoc that
  originally included the literal `std::env::set_var` (explaining why
  the helper exists). The rewrite preserves the same intent ("unit
  tests inject a tempdir-backed env without mutating process vars
  anywhere in the test suite") without the literal string, so the
  acceptance grep is clean.

### Auth Gates

None — pure-Rust changes, no external auth.

### Out of Scope (deferred to Plans 06+)

- D-09 3-way consent prompt in `slate setup` (Plan 06).
- `slate clean` nvim file removal + best-effort init.lua line removal (Plan 06).
- `slate config editor disable` sub-command (Plan 06).
- Integration tests (`nvim --headless -c 'luafile %'`) via `has-nvim` feature flag (Plan 07).
- CI workflow to install Neovim via `rhysd/action-setup-vim@v1` (Plan 07).

## Verification

| Gate                                                                              | Result                       |
| --------------------------------------------------------------------------------- | ---------------------------- |
| `cargo test --lib`                                                                | 647 / 647 pass (+12 new)     |
| `cargo test --lib adapter::nvim`                                                  | 39 / 39 pass (+6 new)        |
| `cargo test --lib platform::version_check`                                        | 12 / 12 pass (+4 new)        |
| `cargo test --lib adapter::registry`                                              | 12 / 12 pass (count → 14)    |
| `cargo test --lib cli::apply`                                                     | 9 / 9 pass (+2 new)          |
| `cargo test --all`                                                                | all suites pass              |
| `cargo clippy --all-targets --all-features -- -D warnings`                        | 0 warnings                   |
| `cargo fmt --all -- --check`                                                      | no diff                      |
| `grep -n "pub struct NvimAdapter" src/adapter/nvim.rs`                            | 1 match                      |
| `grep -n "impl ToolAdapter for NvimAdapter" src/adapter/nvim.rs`                  | 1 match                      |
| `grep -n "pub fn setup" src/adapter/nvim.rs`                                      | 1 match                      |
| `grep -n "pub(crate) fn apply_theme_with_env" src/adapter/nvim.rs`                | 1 match                      |
| `grep -n "ApplyStrategy::WriteAndInclude" src/adapter/nvim.rs`                    | 2 matches (impl + test)      |
| `grep -c "requires_new_shell: false" src/adapter/nvim.rs`                         | 3 matches                    |
| `grep -c "std::env::set_var" src/adapter/nvim.rs`                                 | 0 matches                    |
| `grep -n "\"nvim\" => Some(\"0.8.0\")" src/platform/version_check.rs`             | 1 match                      |
| `grep -n "pub use nvim::NvimAdapter" src/adapter/mod.rs`                          | 1 match                      |
| `grep -c "NvimAdapter" src/adapter/registry.rs`                                   | 1 match (register call)      |
| `grep -n "crate::adapter::nvim::write_state_file" src/cli/apply.rs`               | 1 match                      |
| `grep -n "warning" src/cli/apply.rs \| grep -i nvim`                              | 1 match (eprintln! warning)  |

## Architecture Notes for Plan 06

- **`NvimAdapter::setup` is the consent-flow's target.** Plan 06's
  wizard should only call `setup` after the D-09 3-way prompt resolves
  to option A (slate manages the `pcall(require, 'slate')` line) OR
  option B (user confirms they'll paste it manually — slate still
  writes the shims/loader so the user's paste is immediately useful).
  Option C (skip) can still call `setup` because the shims are
  harmless without the `require('slate')` line — users can invoke
  `:colorscheme slate-catppuccin-mocha` manually and the shim's
  `require('slate').load(...)` call will apply the palette once the
  user opts in later. (Revisit during plan-06 if the UX research
  favors skipping `setup` entirely on option C.)
- **The registry path now includes nvim.** `slate theme set <id>` on
  a machine with nvim installed writes the state file twice: once via
  the registry's `NvimAdapter::apply_theme` (which uses
  `SlateEnv::from_process()`) and once via the shared coordinator tail
  hook (which uses the coordinator's injected `env`). Both writes are
  to the same path and use `AtomicWriteFile::commit` — the net effect
  is a single observable state file whose contents match the latest
  applied variant. No duplicate fs_events in practice because both
  writes complete before the Lua watcher's 100ms debounce window
  elapses.

## Known Stubs

None — every surface added in Plan 05 is fully wired:

- `NvimAdapter::setup` writes real shims + a real loader + a real
  initial state file.
- `apply_theme_with_env` writes a real state file whose contents
  match the applied variant id.
- The registry call path now runs `NvimAdapter::is_installed` → (if
  gated in) → `apply_theme` → real state file write.
- The shared coordinator tail hook writes the state file on every
  successful apply.

## Self-Check: PASSED

**Created files exist:**

- `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-05-SUMMARY.md` — being written now

**Modified files reflect changes:**

- `src/platform/version_check.rs` — 217 LOC, `"nvim" => Some("0.8.0")` arm present, 4 new tests
- `src/adapter/nvim.rs` — 1244 LOC, `pub struct NvimAdapter`, `impl ToolAdapter for NvimAdapter`, `pub fn setup`, `pub(crate) fn apply_theme_with_env` all present
- `src/adapter/mod.rs` — 202 LOC, `pub use nvim::NvimAdapter` present
- `src/adapter/registry.rs` — 392 LOC, `NvimAdapter` registered, `test_registry_default` asserts 14
- `src/cli/apply.rs` — 552 LOC, `crate::adapter::nvim::write_state_file(env, &theme.id)` tail hook present after `set_current_theme`

**Commits on branch:**

- `2a59881` — `test(17-05): add failing tests for nvim 0.8.0 version gate` — FOUND
- `9cc3e7a` — `feat(17-05): add nvim 0.8.0 floor to VersionPolicy` — FOUND
- `8365ca2` — `test(17-05): add failing tests for NvimAdapter trait + setup + fast path` — FOUND
- `f8af8ba` — `feat(17-05): implement NvimAdapter with fast/slow split` — FOUND
- `1bb8c40` — `feat(17-05): register NvimAdapter in the default ToolRegistry` — FOUND
- `891526a` — `test(17-05): add failing test for nvim state-file tail hook` — FOUND
- `2b89cc2` — `feat(17-05): hook nvim state-file write into shared apply coordinator` — FOUND

All seven hashes present in `git log 12f5395..HEAD`. clippy / fmt / full
test suite all green. Plan 17-05 is complete.

## TDD Gate Compliance

Plan-level `type: execute` (not `tdd`), but each TDD-flagged task used
`tdd="true"`. RED → GREEN discipline observed across the three
TDD tasks (Task 3 is `type="auto"` without `tdd="true"` and was a
two-file, one-line mechanical wiring — no test needed, the registry
count assertion update was the behavior change).

| Task | TDD?   | RED commit | GREEN commit | REFACTOR |
| ---- | ------ | ---------- | ------------ | -------- |
| 1    | yes    | `2a59881`  | `9cc3e7a`    | skipped  |
| 2    | yes    | `8365ca2`  | `f8af8ba`    | skipped  |
| 3    | no     | —          | `1bb8c40`    | —        |
| 4    | yes    | `891526a`  | `2b89cc2`    | skipped  |

Each RED commit introduced at least one assertion that failed at
RED-time: Task 1 RED failed on `min_version("nvim")` returning `None`,
Task 2 RED failed to compile (`NvimAdapter` undeclared), Task 4 RED
failed because no hook existed in `apply_theme_with_options` (and
`NvimAdapter`'s own `apply_theme` uses `SlateEnv::from_process()` which
does not see the test's tempdir-scoped `env`). Each GREEN commit
closed exactly those failing assertions without introducing new TODOs.

---

*Phase: 17-editor-adapter-vim-neovim-colorschemes*
*Plan: 05 (Wave 5 — NvimAdapter + registry + shared apply tail hook)*
*Completed: 2026-04-18*
