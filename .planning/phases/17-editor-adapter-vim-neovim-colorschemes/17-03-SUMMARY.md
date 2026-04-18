---
phase: 17
plan: 03
subsystem: adapter-nvim
tags: [editor-adapter, nvim, tdd, lua-loader, fs-event, atomic-write, plan-03]
dependency_graph:
  requires:
    - "src/adapter/nvim.rs::render_colorscheme (Plan 02 — splice target)"
    - "src/design/nvim_highlights.rs::HIGHLIGHT_GROUPS (Plan 01)"
    - "src/theme::ThemeRegistry::all (stable TOML order — deterministic splice)"
    - "src/env.rs::SlateEnv::with_home + slate_cache_dir (pre-existing, XDG-aware)"
    - "atomic_write_file crate (already in Cargo.lock via marker_block.rs)"
  provides:
    - "src/adapter/nvim.rs::render_loader() -> String"
    - "src/adapter/nvim.rs::write_state_file(&SlateEnv, &str) -> Result<()>"
    - "src/adapter/nvim.rs::state_file_path(&SlateEnv) -> PathBuf (pub(crate))"
    - "src/adapter/nvim.rs::lua_string_literal(s) — defensive escape helper"
    - "src/adapter/nvim.rs::LOADER_TEMPLATE_HEAD / _MID / _TAIL consts"
  affects:
    - "Plan 05 (NvimAdapter::apply_theme + apply_setup — calls render_loader + write_state_file)"
    - "Plan 06 (slate clean — reads state_file_path for removal)"
    - "Plan 07 (integration tests — loader_lua_parses_via_luafile, single fs_event assertion)"
tech-stack:
  added: []
  patterns:
    - "head/mid/tail splice: static Lua template constants with per-variant render_colorscheme splice"
    - "pub(crate) shared path helper (state_file_path) for cross-plan reuse without duplication"
    - "defensive Lua string-literal escaping (quote/backslash/newline) for any future id scheme"
    - "atomic-write for watcher observability (AtomicWriteFile::commit = single fs_event fire)"
key-files:
  created:
    - ".planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-03-SUMMARY.md"
  modified:
    - "src/adapter/nvim.rs (432 LOC → 888 LOC; +456 LOC, +19 unit tests)"
    - "src/env.rs (253 LOC → 301 LOC; +2 tests in dedicated slate_cache_dir_tests module)"
key-decisions:
  - "Task 1 (SlateEnv::slate_cache_dir) was a verify-and-skip path — the accessor already existed with full XDG semantics (from_process honours XDG_CACHE_HOME; with_home derives <home>/.cache/slate). Added only a dedicated slate_cache_dir_tests module so Plan 03's acceptance gate resolves literally without retrofitting the existing test module."
  - "Strip the leading `-- slate-managed palette for <id>` comment from render_colorscheme output before splicing into PALETTES. The bare `{ ... }` is what the `['<id>'] =` assignment needs — keeping the comment would produce `['id'] = -- comment\\n{ ... }` which still parses but wastes ~50 bytes per variant and obscures the loader's own structural comments."
  - "Loader template split into HEAD / MID / TAIL constants rather than a single `format!` or template engine. Three benefits: (a) zero format-string escaping for the Lua `{` / `}` braces inside the tail's raw string, (b) Plan 04 can splice LUALINE_THEMES entries between MID and TAIL without touching either constant, (c) Plan 07's syntax gate parses the exact bytes — no runtime template resolution drift."
  - "Tight-loop atomicity test (25 writes, assert final content) substitutes for the RED phase's proposed 'partial-read-during-write' assertion. Observing mid-write state in pure Rust is impossible by construction (AtomicWriteFile uses `.tmp` + rename), so the behavioural proof is: N writes → content equals the N-th write, never a concatenation."
  - "Comment inside VimLeavePre autocmd block uses `-- prevents orphan libuv handles` (single-dash comment, no inner-em-dash) because Lua's long-comment form `--[[...]]` is only triggered by `[[` — the dash-comma-comma sequence in the original plan text was a drafting artifact, not a Lua syntax concern. Spot-checked via the passing `render_loader_registers_vim_leave_pre_cleanup` test which scans for literal substrings 'VimLeavePre' and 'watcher:close'."
metrics:
  duration_seconds: 398
  duration_human: "6m 38s"
  completed_at: "2026-04-18T17:31:23Z"
  tasks_completed: 3
  tdd_phases: ["RED", "GREEN", "RED", "GREEN"]
  commits: 5
  files_created: 1
  files_modified: 2
  lib_tests_before: 597
  lib_tests_after: 618
  new_tests_added: 21
  nvim_rs_loc_before: 432
  nvim_rs_loc_after: 888
  render_loader_bytes: 230477
  render_loader_lines: 4984
  catppuccin_mocha_subtable_bytes: 12688
---

# Phase 17 Plan 03: Runtime Lua loader + state-file plumbing Summary

**Closed D-04's hot-reload loop: `render_loader()` emits the complete `~/.config/nvim/lua/slate/init.lua` module (uv shim, 18-variant PALETTES splice, 100 ms debounce, watcher re-arm, VimLeavePre cleanup, lualine guard, ColorScheme autocmd), and `write_state_file()` atomically writes `~/.cache/slate/current_theme.lua` via `AtomicWriteFile::commit` so the Lua watcher observes exactly one `fs_event` per theme change.**

## Performance

- **Duration:** 6m 38s (398 seconds)
- **Completed:** 2026-04-18T17:31:23Z
- **Tasks:** 3 / 3 (all auto)
- **Commits:** 5 (2 RED + 2 GREEN + 1 verify-and-cover)
- **Files modified:** 2 (`src/adapter/nvim.rs`, `src/env.rs`)
- **LOC delta:** `src/adapter/nvim.rs` 432 → 888 (+456); `src/env.rs` 253 → 301 (+48 tests)

## Accomplishments

- **`render_loader()`** composes the full loader in three pieces: LOADER_TEMPLATE_HEAD (module prelude + uv compat shim + PALETTES open), per-variant sub-table splice keyed by `['<id>']`, LOADER_TEMPLATE_MID (close PALETTES / open LUALINE_THEMES), LOADER_TEMPLATE_TAIL (M.load / M.setup / fs_event watcher / 100 ms debounce / re-arm / VimLeavePre cleanup / return M).
- **All 18 built-in variants** appear in the PALETTES block (verified by iterating `ThemeRegistry::all()` in `render_loader_includes_palettes_for_all_builtin_variants`).
- **Six load-bearing Pitfalls** (per 17-RESEARCH §Pitfalls 1, 2, 5, 6 + VimLeavePre cleanup + ColorScheme autocmd) are inlined verbatim and each has a dedicated test.
- **`write_state_file()`** writes `<slate_cache_dir>/current_theme.lua` atomically via `AtomicWriteFile::open + write_all + commit` — single `fs_event` fire on the watcher side; parent directory is created on first run.
- **`lua_string_literal()`** defensively escapes `\\`, `"`, `\n` for any future variant-id scheme; direct unit coverage plus a round-trip proof through `write_state_file`.
- **`state_file_path()`** is `pub(crate)` so Plans 05 and 06 share the join without duplicating it.
- **`SlateEnv::slate_cache_dir`** verified and covered by a dedicated XDG-aware test module (`slate_cache_dir_tests`) — no global env mutation.

## Output shape (loader)

```
230,477 bytes / 4,984 lines / 18 variant entries
 ├── LOADER_TEMPLATE_HEAD          (   ~280 bytes)
 │   ├── "-- slate-managed: do not edit. Regenerate via `slate setup`."
 │   ├── "local M = {}"
 │   ├── "local uv = vim.uv or vim.loop  -- nvim 0.8 compat (Pitfall 1)"
 │   └── "local PALETTES = {"
 ├── 18 × "  ['<id>'] = { 270 groups },"   (~12,688 bytes per variant)
 ├── LOADER_TEMPLATE_MID           (   ~140 bytes)
 │   └── "local LUALINE_THEMES = {"        (empty in Plan 03; Plan 04 fills)
 └── LOADER_TEMPLATE_TAIL          (  ~1,850 bytes)
     ├── function M.load(variant) … nvim_set_hl … doautocmd ColorScheme
     ├── STATE_PATH + read_state + schedule_reload (100 ms debounce)
     ├── function M.setup(opts) … uv.new_fs_event … watcher re-arm
     ├── VimLeavePre cleanup autocmd (close watcher + debounce_timer)
     ├── M.setup()
     └── return M
```

## Per-task Commits

| Task | Phase | Commit    | Message                                                                       |
| ---- | ----- | --------- | ----------------------------------------------------------------------------- |
| 1    | TEST  | `2b29897` | `test(17-03): cover SlateEnv::slate_cache_dir with XDG-aware tests`           |
| 2    | RED   | `954bec1` | `test(17-03): add failing tests for write_state_file + lua_string_literal`    |
| 2    | GREEN | `ce8afef` | `feat(17-03): implement write_state_file with AtomicWriteFile`                |
| 3    | RED   | `eceae61` | `test(17-03): add failing tests for render_loader template`                   |
| 3    | GREEN | `ce545a2` | `feat(17-03): implement render_loader with loader template + PALETTES splice` |

REFACTOR steps were intentionally skipped: each GREEN implementation was already idiomatic (static constants, pure functions, no duplication). No behaviour-preserving cleanup warranted a third commit per task. Task 1 did not need a GREEN pair because the production code already existed and followed XDG semantics — the commit adds only the dedicated test module to satisfy Plan 03's acceptance grep.

## Tests Added

| Test                                                      | Task | Guards                                                               |
| --------------------------------------------------------- | ---- | -------------------------------------------------------------------- |
| `slate_cache_dir_honors_injected_home`                    | 1    | `with_home(h).slate_cache_dir()` sits under `h/.cache/slate`         |
| `slate_cache_dir_is_stable_across_calls`                  | 1    | Determinism; no side effects                                         |
| `write_state_file_writes_exact_content`                   | 2    | Body is `return "<id>"\n` byte-for-byte                              |
| `write_state_file_creates_parent_directory`               | 2    | First-run: missing cache dir is created                              |
| `write_state_file_is_overwrite_not_append`                | 2    | Two writes → content equals last write (no append)                   |
| `lua_string_literal_escapes_metachars`                    | 2    | `"`, `\`, plain strings all round-trip correctly                     |
| `write_state_file_escapes_quote_metachar`                 | 2    | Quote in variant id flows through escape                             |
| `write_state_file_escapes_backslash_metachar`             | 2    | Backslash in variant id flows through escape                         |
| `write_state_file_loop_yields_final_variant_content`      | 2    | 25-write loop → content = 25th write (atomicity proxy)               |
| `render_loader_includes_uv_compat_shim`                   | 3    | Pitfall 1: `local uv = vim.uv or vim.loop`                           |
| `render_loader_includes_100ms_debounce`                   | 3    | Pitfall 2: `start(100, 0,`                                           |
| `render_loader_registers_vim_leave_pre_cleanup`           | 3    | `VimLeavePre` + `watcher:close` inline                               |
| `render_loader_guards_lualine_package_load`               | 3    | Pitfall 5: `package.loaded['lualine']` / `[\"lualine\"]`             |
| `render_loader_fires_colorscheme_autocmd`                 | 3    | `doautocmd ColorScheme`                                              |
| `render_loader_includes_palettes_for_all_builtin_variants`| 3    | `['<id>']` key per variant in `ThemeRegistry::all()`                 |
| `render_loader_declares_lualine_themes_table`             | 3    | `local LUALINE_THEMES = {` present (Plan 04 will fill)               |
| `render_loader_ends_with_return_m`                        | 3    | Trimmed output ends with `return M`                                  |
| `render_loader_is_deterministic`                          | 3    | Two calls return byte-identical strings                              |
| `render_loader_uses_lf_line_endings`                      | 3    | No `\r` anywhere                                                     |
| `render_loader_size_is_bounded`                           | 3    | 2.5 KB ≤ len ≤ 512 KB (actual: 230 KB)                               |
| `render_loader_calls_nvim_set_hl`                         | 3    | D-05: `vim.api.nvim_set_hl` in M.load body                           |

21 tests added, sub-second runtime, all green under `cargo test --lib`.

## Files Created/Modified

- **`src/adapter/nvim.rs`** (432 → 888 LOC) — adds `write_state_file`, `state_file_path`, `lua_string_literal`, `LOADER_TEMPLATE_HEAD` / `_MID` / `_TAIL`, `render_loader`; 19 new unit tests in the existing `mod tests` block.
- **`src/env.rs`** (253 → 301 LOC) — adds a dedicated `slate_cache_dir_tests` module with 2 pure-function tests (no global env mutation).
- **`.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-03-SUMMARY.md`** (this file).

## Decisions Made

1. **Task 1 was a verify-and-skip.** `SlateEnv::slate_cache_dir` already existed with correct XDG semantics (pre-dated this phase). The production code needed no change. Instead I added the dedicated `slate_cache_dir_tests` module specified by Plan 03's acceptance criteria (`grep -c "pub fn slate_cache_dir"` and `cargo test --lib slate_cache_dir_tests` gates). This follows the plan's explicit instruction: "If present: verify it follows XDG semantics… Skip this task." The "skip" referred to skipping the implementation; the test coverage was still warranted to make the acceptance gate unambiguous.

2. **Strip render_colorscheme comment before splicing.** `render_colorscheme` prepends `-- slate-managed palette for <id>\n` to its output. Inside the loader's `PALETTES` block we need a bare `{ ... }` on the RHS of `['<id>'] = …`. Keeping the comment would produce `['id'] = -- comment\n{ ... }` (valid Lua, but cluttered and redundant with the loader's own section headers). Using `split_once('\n')` to strip the first line is cheap and Plan 02 already documented this shape.

3. **Head/mid/tail constant split.** Rather than one monolithic `format!` or a templating engine, three `&'static str` consts. This gives Plan 04 a clean insertion point (between MID and TAIL) without editing the surrounding template, and lets Plan 07's `luafile` gate parse the exact compiled bytes.

4. **Tight-loop atomicity proxy.** The plan suggested observing mid-write partial content, which is impossible in pure Rust (AtomicWriteFile's `.tmp + rename` guarantees no observer ever sees the half-written file at the target path). The practical proof is 25 writes → final content matches the 25th, which would fail if any write appended or left stale bytes. Not a weakening of the contract — it's the structural invariant restated as a runtime assertion.

5. **Keep the existing `test_cache_dir` test unchanged.** It asserts `env.slate_cache_dir().ends_with(".cache/slate")` as a surface check; the new `slate_cache_dir_tests` module adds stronger invariants (injected-home containment, determinism) in its own namespace. Two independent tests of the same accessor is fine — they guard different angles and the module name satisfies Plan 03's grep verbatim.

## Deviations from Plan

None — plan executed as written with three minor clarifications documented above under **Decisions Made** (verify-and-skip path for Task 1, comment-stripping before splice, tight-loop atomicity proxy). No auto-fix rules triggered; no scope creep; no architectural decisions punted.

## Issues Encountered

**Worktree base mismatch at agent startup.** The initial `git merge-base HEAD <required-base>` returned `e34f10c` (v0.1.2 release commit, ancestor of the required `8d1d252`). Per the `<worktree_branch_check>` protocol, hard-reset the worktree to `8d1d252` (which includes the merged 17-00 / 17-01 / 17-02 outputs). After reset, `src/adapter/nvim.rs` and `src/design/nvim_highlights.rs` became available and the build was green. No code changes; just the prescribed worktree-base correction.

**`cargo fmt --check` caught a long-line wrap in the Task 3 GREEN commit.** The `expect(...)` message on `ThemeRegistry::new()` inside `render_loader` exceeded the default `max_width = 100`. Ran `cargo fmt --all`, re-verified clippy + tests, amended to a single wrapped line, re-gated green. Not a deviation — standard pre-commit hygiene.

## Verification

| Gate                                                              | Result                      |
| ----------------------------------------------------------------- | --------------------------- |
| `cargo test --lib adapter::nvim::tests`                           | 30 / 30 pass                |
| `cargo test --lib env::slate_cache_dir_tests`                     | 2 / 2 pass                  |
| `cargo test --lib env::tests` (existing)                          | 9 / 9 pass (no regression)  |
| `cargo test --lib`                                                | 618 / 618 pass (+21 new)    |
| `cargo clippy --all-targets --all-features -- -D warnings`        | 0 warnings                  |
| `cargo fmt --all -- --check`                                      | no diff                     |
| `grep -c "pub fn render_loader" src/adapter/nvim.rs`              | 1                           |
| `grep -c "LOADER_TEMPLATE_" src/adapter/nvim.rs`                  | 10 (3 decls + 7 refs ≥ 3)   |
| `grep -c "pub fn write_state_file" src/adapter/nvim.rs`           | 1                           |
| `grep -c "pub(crate) fn state_file_path" src/adapter/nvim.rs`     | 1                           |
| `grep -c "pub fn slate_cache_dir" src/env.rs`                     | 1                           |
| `wc -l src/adapter/nvim.rs`                                       | 888 (≥ 400 floor)           |
| `render_loader()` size                                            | 230,477 bytes (in envelope) |

## Architecture Notes for Plan 05

- **`render_loader()` is pure.** Calls `ThemeRegistry::new()` internally (panics on init failure — validated at phase load by prior plans). No I/O, no env lookup. Plan 05's `apply_setup` will call this + `render_shim` (Plan 02) + write them through `ConfigManager`.
- **`write_state_file(env, id)` is the fast path.** `NvimAdapter::apply_theme` should call only `write_state_file` (not the shim/loader writer) — the file watcher already present in running nvim instances consumes the change and re-runs `M.load(variant)` in-process. This is the D-04 contract.
- **`state_file_path(env)` is `pub(crate)`.** Plan 06's `remove_nvim_managed_references` should use it to locate the cache file for deletion, not reconstruct the path.
- **LUALINE_THEMES is intentionally empty.** Plan 04 inserts per-variant theme tables between `LOADER_TEMPLATE_MID` and `LOADER_TEMPLATE_TAIL`. The splice insertion point is `out.push_str(LOADER_TEMPLATE_MID); /* Plan 04 splices here */ out.push_str(LOADER_TEMPLATE_TAIL);` in `render_loader`.

## Known Stubs

None specific to this plan. The `LUALINE_THEMES = {}` empty table is intentional and documented — Plan 04 fills it — and the loader correctly guards on `LUALINE_THEMES[variant]` being non-nil inside `M.load`, so the stub is inert at runtime.

## Self-Check: PASSED

**Created files exist:**

- `.planning/phases/17-editor-adapter-vim-neovim-colorschemes/17-03-SUMMARY.md` — being written now

**Modified files reflect changes:**

- `src/adapter/nvim.rs` — 888 LOC, `render_loader` / `write_state_file` / `state_file_path` / `lua_string_literal` all present
- `src/env.rs` — 301 LOC, `slate_cache_dir_tests` module present with 2 tests

**Commits on branch (verified via `git log --oneline 8d1d252..HEAD`):**

- `2b29897` — `test(17-03): cover SlateEnv::slate_cache_dir with XDG-aware tests` — FOUND
- `954bec1` — `test(17-03): add failing tests for write_state_file + lua_string_literal` — FOUND
- `ce8afef` — `feat(17-03): implement write_state_file with AtomicWriteFile` — FOUND
- `eceae61` — `test(17-03): add failing tests for render_loader template` — FOUND
- `ce545a2` — `feat(17-03): implement render_loader with loader template + PALETTES splice` — FOUND

All five hashes verified. clippy / fmt / test gates green. Plan 17-03 is complete.

---

*Phase: 17-editor-adapter-vim-neovim-colorschemes*
*Plan: 03 (Wave 3 — runtime Lua loader + state-file plumbing)*
*Completed: 2026-04-18*
