---
phase: 16
plan: 02
subsystem: adapter
tags: [ls_colors, eza_colors, adapter, truecolor, palette-projection, phase-16]
requires:
  - file_type_colors::classify (Phase 15 classifier — single source of truth)
  - file_type_colors::extension_map (Phase 15 — 48 entries confirmed on read)
  - file_type_colors::full_name_map (newly exposed accessor — 4 entries)
  - PaletteRenderer::hex_to_rgb / rgb_to_ansi_24bit (existing truecolor primitive)
  - ApplyOutcome::Applied { requires_new_shell } (Plan 16-01 foundation)
provides:
  - LsColorsAdapter struct implementing ToolAdapter (EnvironmentVariable strategy)
  - render_ls_colors(&Palette) -> String (pub(crate))
  - render_eza_colors(&Palette) -> String (pub(crate))
  - render_strings(&Palette) -> (String, String) (pub(crate), for Plan 16-04)
  - FILE_TYPE_KIND_KEYS static (8 file-type kind key → SemanticColor tuples)
  - full_name_map() accessor in src/design/file_type_colors.rs
affects:
  - src/adapter/mod.rs (module registration only — one new `pub mod ls_colors;`)
tech-stack:
  added: []
  patterns:
    - "Projection-only module — no file writes, consumed by downstream composer"
    - "Module-level `#![allow(dead_code)]` with rationale comment pointing to Plan 16-04"
    - "Rule-3 Rust accessor pattern: expose private static via pub fn, not pub const"
key-files:
  created:
    - src/adapter/ls_colors.rs
  modified:
    - src/adapter/mod.rs
    - src/design/file_type_colors.rs
decisions:
  - "D-A1 honoured: LsColorsAdapter implements ToolAdapter with EnvironmentVariable strategy"
  - "D-A3 honoured: exhaustive file-type kind keys + every extension_map() + every full_name_map()"
  - "D-A5 honoured: 24-bit truecolor only (0 occurrences of 38;5; in production code paths)"
  - "D-B5 honoured: is_installed() always Ok(true) — env var is a no-op on BSD ls, never gated"
  - "D-C3 honoured: apply_theme returns ApplyOutcome::Applied { requires_new_shell: true }"
  - "Pitfall 4 mitigated: `or`/`so`/`pi`/`bd`/`cd` intentional reuse documented in module-level doc comment + enforced by ls_colors_or_uses_file_symlink_role_by_intent test"
  - "Pitfall 2 (eza built-in extension DB drift) mitigated: render_eza_colors prepends reset: sentinel"
metrics:
  duration: "7m 27s"
  tests_added: 15
  tests_passing: 15
  lib_tests_total: 524
  files_changed: 3
  lines_added: 551
  completed: 2026-04-18T05:10:27Z
---

# Phase 16 Plan 02: LS_COLORS / EZA_COLORS Projection Adapter Summary

Shipped a pure projection adapter that turns the Phase-15 `file_type_colors` classifier into two truecolor environment-variable strings (`LS_COLORS`, `EZA_COLORS`), with zero drift from `classify()` and a `reset:` sentinel that stops eza's built-in extension map from leaking non-palette colours.

## What Landed

- **`src/adapter/ls_colors.rs`** (new, 542 lines including tests and docs)
  - Module-level doc comment documenting the 8-key `FILE_TYPE_KIND_KEYS` mapping, the orphan/socket/pipe/device intentional reuse rationale, and the eza `reset:` sentinel invariant.
  - `FILE_TYPE_KIND_KEYS` static: 8 kind → `SemanticColor` tuples (`di→FileDir`, `ln→FileSymlink`, `ex→FileExec`, `or→FileSymlink`, `so→FileExec`, `pi→FileConfig`, `bd→FileConfig`, `cd→FileConfig`).
  - `ansi_code(palette, role) -> String` — graceful-degrade helper (returns `"0"` on hex parse error).
  - `render_ls_colors(&Palette) -> String` — iterates kind keys → `extension_map()` → `full_name_map()`, emits `rs=0:no=0:…` wire format with 24-bit truecolor only.
  - `render_eza_colors(&Palette) -> String` — prepends `reset` sentinel, reuses the LS body, appends eza identity keys (`uu`/`gu`=`Text`, `un`/`gn`/`da`=`Muted`).
  - `render_strings(&Palette) -> (String, String)` — convenience tuple wrapper for Plan 16-04's `SharedShellModel::new`.
  - `LsColorsAdapter` struct + `ToolAdapter` impl: `tool_name()="ls_colors"`, `is_installed()=Ok(true)` (D-B5), `apply_strategy()=EnvironmentVariable`, `apply_theme(_)` returns `ApplyOutcome::Applied { requires_new_shell: true }` (D-C3).
  - 15 unit tests covering every acceptance-criteria case (see "Tests Landed" below).

- **`src/adapter/mod.rs`** (1-line module registration)
  - Added `pub mod ls_colors;` in alphabetical position between `lazygit` and `marker_block`.

- **`src/design/file_type_colors.rs`** (new 3-line accessor)
  - Added `pub fn full_name_map() -> &'static [(&'static str, SemanticColor)]`, mirroring the existing `extension_map()` accessor. Rule-3 auto-fix (see Deviations).

## FILE_TYPE_KIND_KEYS Mapping (with Intentional-Reuse Rationale)

| GNU key | Meaning                   | Phase-15 role reused | Rationale                                                                                             |
|---------|---------------------------|----------------------|-------------------------------------------------------------------------------------------------------|
| `di`    | Directory                 | `FileDir`            | Direct classifier equivalence (`FileKind::Directory → FileDir`).                                     |
| `ln`    | Symbolic link             | `FileSymlink`        | Direct classifier equivalence (`FileKind::Symlink → FileSymlink`).                                   |
| `ex`    | Executable file           | `FileExec`           | Direct classifier equivalence (`FileKind::Executable → FileExec`).                                   |
| `or`    | Orphan symlink (broken)   | `FileSymlink` (reuse)| Phase-15 has no `Orphan` variant; `FileSymlink` is closest semantic neighbour.                       |
| `so`    | Unix socket               | `FileExec` (reuse)   | Phase-15 has no `Socket` variant; sockets are active/executable-adjacent.                            |
| `pi`    | Named pipe / FIFO         | `FileConfig` (reuse) | Phase-15 has no `Pipe` variant; pipes are plumbing-like, grouped with metadata keys.                 |
| `bd`    | Block device              | `FileConfig` (reuse) | Phase-15 has no `BlockDevice` variant; devices are infrastructure-adjacent.                          |
| `cd`    | Character device          | `FileConfig` (reuse) | Same as `bd`.                                                                                          |

Reuse mappings are enforced by the dedicated test `ls_colors_or_uses_file_symlink_role_by_intent` and documented in the module's doc comment. Per RESEARCH §Pitfall 4, this is recommendation (a) — "accept the gap and document it explicitly". Extending `FileKind` with `Pipe`/`Socket`/`BlockDevice`/`CharDevice` variants remains deferred (Phase 15 deferred list).

## EZA_COLORS `reset:` Sentinel Rationale

Per `eza_colors(5)`, eza merges `EZA_COLORS` over a **built-in extension → colour map**. Setting only `EZA_COLORS="*.rs=…"` does not wipe the built-ins; it overlays. To preserve slate's "one palette across the stack" invariant (any extension not in `file_type_colors::extension_map()` must fall through to default foreground via `no=0`, not to eza's built-in colour for that extension), `render_eza_colors` prepends `reset:`. The `reset:` keyword resets eza's internal style state, then our entries apply on top of a clean slate.

The test `eza_colors_starts_with_reset_sentinel` pins this contract.

## Tests Landed (15 total, all green)

### LS_COLORS (7 tests)
1. `ls_colors_starts_with_rs_and_no_sentinels` — string begins with `rs=0:no=0`.
2. `ls_colors_contains_all_file_type_kind_keys` — all 8 kind keys present with truecolor codes.
3. `ls_colors_contains_every_extension_map_entry` — every `extension_map()` entry emitted as `:*.{ext}=38;2;…`.
4. `ls_colors_contains_every_full_name_entry` — every `full_name_map()` entry emitted as `:{name}=38;2;…`.
5. `ls_colors_round_trips_through_classifier` — for 8 curated cases (including `main.rs`, `Cargo.lock`, `src` directory, etc.), the `classify()`-derived ANSI code equals the code emitted in `LS_COLORS`.
6. `ls_colors_uses_only_truecolor_codes` — every value is `38;2;R;G;B` or literal `0`; no `38;5;`, no `\x1b[` wrappers.
7. `ls_colors_or_uses_file_symlink_role_by_intent` — pins the `or` → `FileSymlink` intentional reuse.

### EZA_COLORS (4 tests)
8. `eza_colors_starts_with_reset_sentinel` — string begins with literal `reset:`, no leading colon.
9. `eza_colors_body_equals_ls_colors_body_for_shared_keys` — every kind / extension / full-name entry in LS appears verbatim in the EZA body.
10. `eza_colors_has_identity_keys` — `uu`, `gu`, `un`, `gn`, `da` all present with truecolor codes.
11. `eza_colors_identity_keys_map_to_text_and_muted` — `uu`/`gu` carry `Text`, `un`/`gn`/`da` carry `Muted`.

### Adapter contract (4 tests)
12. `ls_colors_adapter_declares_env_var_strategy` — `tool_name="ls_colors"`, strategy is `EnvironmentVariable`.
13. `ls_colors_adapter_apply_theme_requires_new_shell` — `apply_theme` returns `ApplyOutcome::Applied { requires_new_shell: true }` (D-C3 contract).
14. `ls_colors_adapter_is_installed_is_always_true` — always `Ok(true)` per D-B5.
15. `render_strings_returns_both_env_vars` — tuple wrapper matches individual calls.

All LS / EZA tests are parameterised across Catppuccin Mocha (macOS-family dark) and Gruvbox Dark (Linux-family).

## Confirmation: `requires_new_shell: true` per D-C3

`LsColorsAdapter::apply_theme` declares `Ok(ApplyOutcome::Applied { requires_new_shell: true })` at `src/adapter/ls_colors.rs:213-215`. The dedicated test `ls_colors_adapter_apply_theme_requires_new_shell` enforces this at the contract level. This is the **struct variant** from Plan 16-01 (Wave 0) — not the pre-phase unit variant.

## Pointer to Plan 16-04 (Wave 2)

`render_strings()` (line 155) is the entry point that Plan 16-04 will call from `SharedShellModel::new` to materialise the two env vars into `managed/shell/env.{zsh,bash,fish}`. Until Plan 16-04 lands, the `pub(crate)` render functions have no non-test call sites — the module-level `#![allow(dead_code)]` (line 51) handles this transitional state and will be dropped when Plan 16-04 wires the call site.

## Discoveries During Implementation

- **`extension_map()` has 48 entries, not 38.** RESEARCH §Pattern 1 claimed 38 (archives 9 + images 8 + video 5 + audio 5 + source 10 + docs 5 + config 5 = 47; 47 ≠ 38). Reading `src/design/file_type_colors.rs` directly confirmed 48 entries (9+8+5+5+10+5+5 = 47 + 1 `bmp` nested in images block = 48). RESEARCH's count was stale; the tests iterate the live slice so no brittleness was introduced. No impact on correctness — all entries are exhaustively emitted.
- **`full_name_map()` was private.** `FULL_NAME_MAP` was a `static` with module-private visibility. The plan's `<action>` item 2 instructed the executor to import it, but it was not exported. Added a public `full_name_map()` accessor (mirrors `extension_map()`) in the same file — minimal, additive, no API surface removed. See "Deviations from Plan" below.
- **`Palette::resolve` returns a hex string including the `#` prefix** (e.g. `"#89b4fa"`). `PaletteRenderer::hex_to_rgb` already strips the `#`, so the pipeline composes cleanly.
- **`rstest::rstest` is already a dev-dependency** (Cargo.toml:72). Tests use it for palette-parameterisation in harmony with existing eza adapter tests.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Expose `full_name_map()` accessor in `src/design/file_type_colors.rs`**
- **Found during:** Task 1, imports setup
- **Issue:** Plan's `<action>` item 2 instructs the executor to import `FULL_NAME_MAP` from `crate::design::file_type_colors`, but the static was module-private. Hard-coding the 4 entries inside `ls_colors.rs` would violate the Phase-15 invariant ("`file_type_colors` is the single source of truth; Phase 16 is a pure projection with zero drift permitted", CONTEXT §D-A3).
- **Fix:** Added a 3-line public accessor `pub fn full_name_map() -> &'static [(&'static str, SemanticColor)]` that returns the existing private static. Mirrors `extension_map()` exactly — same shape, same pattern, zero net API surface expansion beyond what's needed.
- **Files modified:** `src/design/file_type_colors.rs` (+7 lines including doc comment)
- **Commit:** `34f05d3` (bundled into the RED test commit since the stubs also depended on it)
- **Scope check:** The plan's `files_modified` list names only `src/adapter/ls_colors.rs` and `src/adapter/mod.rs`. `src/design/file_type_colors.rs` is strictly additive and backwards-compatible; peer 16-03 does not touch this file (per parallel_execution context).

### Non-fixed Observations (flagged for future polish)

- **RESEARCH §Pattern 1 extension count** claimed 38; actual is 48. Cosmetic documentation staleness in the research file — no code impact. Not updating the research file from this plan (out of scope).

## Known Stubs

None. Every function, struct, and test has a real implementation. The `#![allow(dead_code)]` attribute is a transitional scope marker (pointing to Plan 16-04), not a stub.

## Threat Flags

None. No new network endpoints, auth surfaces, or file-access patterns at trust boundaries. The adapter reads immutable palette data and returns a `String`; no I/O, no PATH resolution beyond what existing adapters already perform.

## Self-Check: PASSED

All claims verified on disk:

- `src/adapter/ls_colors.rs` exists (542 lines)
- `src/adapter/mod.rs` line 14: `pub mod ls_colors;`
- `src/design/file_type_colors.rs`: `pub fn full_name_map()` accessor present
- Commit `34f05d3` (RED) — test: add failing tests for LS_COLORS/EZA_COLORS adapter
- Commit `d2f4c43` (GREEN) — feat: implement LS_COLORS/EZA_COLORS projection
- `cargo fmt --check`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --lib -- ls_colors`: 15 passed, 0 failed
- `cargo test` (full suite): 524 lib tests + 129 integration tests all green
- No touch to `src/config/shell_integration.rs` or `src/adapter/registry.rs` (Plan 16-04 territory)
- No touch to peer 16-03's files (language.rs, new_shell_reminder.rs, cli/mod.rs, tracked_state.rs, detection.rs)

## TDD Gate Compliance

Plan-level TDD gate sequence satisfied:

1. **RED** — `34f05d3` `test(16-02): add failing tests for LS_COLORS/EZA_COLORS adapter` — 15 tests compiled but 13 failed (stubs returned empty strings / wrong signals). Fail-fast rule honoured: the 2 tests that passed (`ls_colors_adapter_declares_env_var_strategy` and `render_strings_returns_both_env_vars`) pass trivially because of stub symmetry, not because the feature already exists.
2. **GREEN** — `d2f4c43` `feat(16-02): implement LS_COLORS/EZA_COLORS projection` — all 15 tests pass with real implementation.
3. **REFACTOR** — not required; the implementation landed clean against rustfmt + clippy on first green pass.
