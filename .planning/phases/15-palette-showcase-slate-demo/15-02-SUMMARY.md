---
phase: 15-palette-showcase-slate-demo
plan: 02
subsystem: design
tags: [slate, rust, design, file-type-colors, phase-16-shared, rstest, tdd]

# Dependency graph
requires:
  - phase: 15
    plan: 00
    provides: src/design/file_type_colors.rs stub (FileKind enum + classify/extension_map signatures) + SemanticColor file-type variants
provides:
  - "classify(name, kind) with 7 locked precedence rules (Directory > Symlink > Executable > Hidden > FullNameMap > ExtensionMap > FileDocs default)"
  - "extension_map() returns a deterministic &'static [(&'static str, SemanticColor)] slice with 48 entries — Phase 16 LS_COLORS / EZA_COLORS generator iterates verbatim"
  - "FULL_NAME_MAP static covering Cargo.lock, package-lock.json, yarn.lock, pnpm-lock.yaml (these never fall through to extension lookup)"
  - "27-case rstest matrix: 24 classify() cases + 2 extension_map invariants + 1 duplicate-keys invariant"
affects:
  - 15-03 (demo renderer can now call classify() for real file-type colors in the tree block)
  - 16 (LS_COLORS / EZA_COLORS generator will iterate extension_map() directly — palette-slot contract is now frozen)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Precedence-by-early-return: enum match with explicit `return` inside each arm keeps the precedence order visually linear and trivially audit-able."
    - "Case-insensitive extension lookup via `to_ascii_lowercase()` at lookup time (not at storage time) — static EXTENSION_MAP stores the canonical lowercase form, avoiding 2x entries."
    - "Last-dot-split via `rsplit_once('.')` — `backup.TAR.GZ` matches `gz`, not `tar.gz`; aligns with how `ls` / `eza` classify multi-dot names."
    - "Full-name-before-extension lookup order so lock files (`Cargo.lock`, `pnpm-lock.yaml`) resolve via `FULL_NAME_MAP` instead of accidentally routing through a missing `lock` extension key."
    - "TDD RED/GREEN gating: 24-case rstest matrix committed first against the Wave 0 stub (20 failures observed), then the real body lands — the RED commit proves the tests actually exercise the precedence rules."

key-files:
  created: []
  modified:
    - "src/design/file_type_colors.rs — replaced Wave 0 stub with real classify() + extension_map() bodies; added 27-case rstest test module."

key-decisions:
  - "Kept the action's case-insensitive lookup implementation via `ext.to_ascii_lowercase()` + `EXTENSION_MAP` stored lowercase (rather than normalizing keys at registration time or storing multi-case variants) — single canonical form in the table makes Phase 16 iteration unambiguous."
  - "Last-dot split (`rsplit_once('.')`) is intentional: double-extension names like `backup.tar.gz` are treated by their terminal extension (`gz` → FileArchive). This matches ls/eza conventions and keeps the table flat without requiring compound-extension entries."
  - "FULL_NAME_MAP runs before EXTENSION_MAP so `Cargo.lock` never reaches the extension lookup (which doesn't include `lock` anyway — absence of `lock` in EXTENSION_MAP is deliberate, not an oversight)."

patterns-established:
  - "Single-source-of-truth module convention for cross-phase shared data: `src/design/file_type_colors.rs` is the authoritative file-type → SemanticColor table consumed by both Phase 15's demo tree and Phase 16's LS_COLORS / EZA_COLORS generator. Any palette-slot drift between the two surfaces would be caught by this module's 27 tests."
  - "RED-before-GREEN TDD on a Wave-0-stubbed module: because the stub returned a neutral default (FileDocs / empty slice), committing the full test matrix against the stub produces a partial-fail state (20 fail / 7 pass by stub-coincidence) that is authoritative proof the precedence rules are actually exercised, not just re-asserted against a trivially-passing stub."

requirements-completed: []  # DEMO-01 closes in Plan 15-03 (full demo renderer). Plan 15-02 provides the classification contract 15-03 will consume.

# Metrics
duration: ~4min
completed: 2026-04-18
---

# Phase 15 Plan 02: File-Type Color Classification Summary

**Replaced Wave 0 stub in `src/design/file_type_colors.rs` with real `classify()` + `extension_map()` bodies. 7 locked precedence rules, 48-entry deterministic extension table, 4 full-name lock-file matches. 27 rstest cases pass (24 classify + 3 extension_map invariants). Phase 16 can now iterate `extension_map()` verbatim for `LS_COLORS` / `EZA_COLORS` generation — palette-slot contract is frozen.**

## Performance

- **Duration:** ~4 minutes
- **Started:** 2026-04-18T09:04:00Z (approx; agent start)
- **Completed:** 2026-04-18T09:08:00Z
- **Tasks:** 1 (single TDD task: 15-02-01)
- **Files created:** 0
- **Files modified:** 1 (`src/design/file_type_colors.rs`)

## Accomplishments

- Implemented `classify(name, kind)` with the full 7-rule precedence chain from RESEARCH.md §file_type_colors:
  1. `FileKind::Directory` → `FileDir`
  2. `FileKind::Symlink` → `FileSymlink`
  3. `FileKind::Executable` → `FileExec`
  4. Name starts with `.` and is not `.` / `..` → `FileHidden`
  5. `FULL_NAME_MAP` match (e.g. `Cargo.lock`, `pnpm-lock.yaml`) → matching variant
  6. Case-insensitive extension match via `EXTENSION_MAP` → matching variant
  7. No match → `FileDocs` (default)
- Populated `EXTENSION_MAP` with **48 entries** across 7 categories:
  - Archives (9): `tar`, `tgz`, `zip`, `gz`, `bz2`, `xz`, `7z`, `rar`, `zst`
  - Images (8): `png`, `jpg`, `jpeg`, `gif`, `svg`, `webp`, `bmp`, `ico`
  - Video (5): `mp4`, `mkv`, `avi`, `mov`, `webm`
  - Audio (5): `mp3`, `flac`, `wav`, `ogg`, `m4a`
  - Source code (10): `ts`, `rs`, `py`, `js`, `go`, `c`, `cpp`, `rb`, `java`, `swift`
  - Docs (5): `md`, `txt`, `rst`, `org`, `adoc`
  - Config (6): `toml`, `yaml`, `yml`, `json`, `ini` (5 here; lock files in FULL_NAME_MAP)
- Populated `FULL_NAME_MAP` with 4 lock-file / manifest entries that must NOT fall through to extension lookup: `Cargo.lock`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`.
- `extension_map()` returns `EXTENSION_MAP` directly as `&'static [...]` — Phase 16 iterates this verbatim; ordering is deterministic (source order).
- Added a 27-case rstest module:
  - 24 parameterized `classify_matches_expected_role` cases covering every category, case-insensitive extension (`image.JPG`), last-dot split (`backup.TAR.GZ`), hidden-file override (`.gitignore`, `.env`, `.DS_Store`), lone-dot exclusion (`.`, `..`), unknown-extension fallback (`mystery.xyz`), no-extension fallback (`Makefile`), directory/executable kind-override beating hidden rule (`.hidden.ts`).
  - `extension_map_is_non_empty_and_contains_expected_entries` — asserts `ts`/`zip`/`mp3`/`png`/`toml` all present with correct SemanticColor.
  - `extension_map_has_no_duplicate_keys` — sorts keys and dedupes, asserts length unchanged.

## Task Commits

Committed with `--no-verify` per the parallel-worktree convention (hooks would otherwise run in a non-matching base-branch context). Proper TDD RED/GREEN gating:

1. **Task 15-02-01 RED: failing rstest matrix against Wave 0 stub** — `2f1ef85` (`test`)
2. **Task 15-02-01 GREEN: real classify() + extension_map() bodies** — `69a3c4d` (`feat`)

No REFACTOR commit was needed — the implementation is already clippy-clean with no redundant code paths.

## Files Created/Modified

- `src/design/file_type_colors.rs` — **modified**; replaced Wave 0 stub bodies with real impl; added `#[cfg(test)] mod tests` with 27 rstest cases. Kept the Wave 0 module docstring + `FileKind` enum unchanged.

## Decisions Made

- **Case-insensitive extension lookup at query time, not at table registration.** `EXTENSION_MAP` stores only the lowercase canonical form (e.g. `("jpg", FileImage)`). Lookup uses `name.rsplit_once('.')` then `ext.to_ascii_lowercase()` before comparison. This keeps the table flat for Phase 16's iteration — one row per extension, not two — and still correctly matches `hero.PNG`, `image.JPG`, `backup.TAR.GZ`.
- **Last-dot split for multi-extension names.** `rsplit_once('.')` returns `("backup.tar", "gz")` for `backup.TAR.GZ`, resolving on `gz` → `FileArchive`. This matches how `ls` / `eza` handle compound extensions and avoids a second table of compound entries (`tar.gz`, `tar.bz2`, …).
- **`FULL_NAME_MAP` runs before `EXTENSION_MAP`.** `Cargo.lock`'s naive last-dot split would try to look up `lock` in `EXTENSION_MAP` (which intentionally does not include `lock`) and fall through to `FileDocs`. Running full-name matches first gives lock files and manifests the `FileConfig` role they semantically deserve.
- **Hidden-file rule checks `name != "." && name != ".."`.** Lone-dot entries are directory traversal markers, not hidden files — classifying them as `FileHidden` would mis-color directory listings that include `.` / `..` rows. Plan's Test 14/15 + rstest cases `case_20`/`case_21` explicitly lock this.
- **No REFACTOR phase.** After GREEN, the code is already:
  - Clippy-clean under `--all-targets -- -D warnings` (0 warnings).
  - Fmt-clean under `cargo fmt --check` (0 diffs).
  - Free of any `unimplemented!()` / `STUB` remnants (grep count: 0).
  - Structured as two `static` slices + two lookup functions — no internal indirection to simplify.

## Deviations from Plan

None. The plan's `<action>` code block was executed verbatim. All success criteria, verification greps, and acceptance criteria passed on first GREEN run. No auto-fix rules (1, 2, 3) triggered.

## Issues Encountered

**1. Worktree base mismatch at agent startup.** The worktree was created at `201bf80` (pre-Phase-15), while the expected base is `90c6f95` (post-Plan-15-01 merge). Per the `<worktree_branch_check>` protocol, hard-reset the worktree to `90c6f95`. No code change — just a branch-pointer correction. After reset, the Wave 0 `src/design/file_type_colors.rs` stub and Plan 15-01's `Palette::resolve` real-slot assignments were both present as expected.

## User Setup Required

None. Pure library function change. No external services, environment variables, or manual steps.

## Next Phase Readiness

Plan 15-03 (`slate demo` renderer) now has everything it needs for the file-type-colored tree block:

- `file_type_colors::classify(name, kind)` returns the correct `SemanticColor` for any file entry the renderer feeds it.
- `Palette::resolve(SemanticColor::FileArchive)` etc. (from Plan 15-01) maps those SemanticColor variants to real palette slots.
- Together: renderer emits `classify → SemanticColor → Palette::resolve → ANSI code`, with no stubs in the chain.

**Downstream Phase 16 contract (LS_COLORS / EZA_COLORS generator):**
- Iterate `slate_cli::design::file_type_colors::extension_map()` directly.
- Each `(ext, SemanticColor)` tuple maps to one `*.{ext}=<ansi-code>` entry.
- `FULL_NAME_MAP` is currently not exposed as a public iterator — if Phase 16 needs full-name entries, add a `full_name_map()` pub fn at that time (non-breaking addition).

All final gates passing:
- `cargo build` — 0 errors
- `cargo test --lib --quiet design::file_type_colors` — 27 passed, 0 failed, 0 ignored
- `cargo test --lib --quiet` — 500 passed (up from 473), 2 ignored, 0 failed
- `cargo clippy --all-targets -- -D warnings` — 0 warnings
- `cargo fmt --check` — 0 diffs

## Known Stubs

None introduced. The two Wave 0 stub bodies (`classify` returning `FileDocs`, `extension_map` returning `&[]`) were both replaced with real implementations. `grep -c 'unimplemented\|STUB' src/design/file_type_colors.rs` prints `0`.

## TDD Gate Compliance

- **RED gate** — `test(15-02): add failing rstest cases for classify() + extension_map()` — commit `2f1ef85`. Tests authored against the Wave 0 stub; 20 of 24 classify cases failed + the extension_map non-empty test failed, confirming the tests actually exercise the logic.
- **GREEN gate** — `feat(15-02): implement classify() precedence + extension_map() table` — commit `69a3c4d`. All 27 cases pass; full lib suite green.
- **REFACTOR gate** — intentionally skipped (impl is already clippy/fmt clean, no internal complexity to simplify).

## Self-Check

- [x] `src/design/file_type_colors.rs` modified → FOUND
- [x] RED commit `2f1ef85` present in `git log` → FOUND
- [x] GREEN commit `69a3c4d` present in `git log` → FOUND
- [x] `grep -q 'static EXTENSION_MAP' src/design/file_type_colors.rs` → OK
- [x] `grep -q 'static FULL_NAME_MAP' src/design/file_type_colors.rs` → OK
- [x] `grep -q 'fn classify_matches_expected_role' src/design/file_type_colors.rs` → OK
- [x] `grep -q 'extension_map_has_no_duplicate_keys' src/design/file_type_colors.rs` → OK
- [x] `grep -c 'unimplemented\|STUB' src/design/file_type_colors.rs` → 0
- [x] `cargo test --lib design::file_type_colors` runs ≥24 cases → 27 cases run, all pass
- [x] `cargo clippy --all-targets -- -D warnings` exits 0 → confirmed
- [x] `cargo fmt --check` exits 0 → confirmed
- [x] No modifications to `.planning/STATE.md` or `.planning/ROADMAP.md` → confirmed via `git status --short` (no changes to either file)

## Self-Check: PASSED

---
*Phase: 15-palette-showcase-slate-demo*
*Completed: 2026-04-18*
