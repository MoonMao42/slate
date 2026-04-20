---
phase: 19
plan: "04"
subsystem: cli-picker-preview-compose
tags: [picker, preview, compose, fold-tier, wave-2, D-12, D-13, D-04, V-05, V-07]
dependency_graph:
  requires:
    - Plan 19-02 (picker::preview::blocks with render_code/tree/git_log/progress/palette_swatch pub fns)
    - Plan 19-03 (Wave 1 sibling — RollbackGuard + PickerState.preview_mode_full; no direct symbol use in this plan but the Wave-2 merge assumes both Wave-1 artifacts are in tree)
    - Phase 18 Roles API (Roles::heading → brand-lavender ◆ glyph)
    - Phase 18 brand::render_context::mock_theme / mock_context (byte-stable test fixtures)
    - Phase 15 SAMPLE_TOKENS + SemanticColor enum (src/cli/picker/preview_panel.rs)
  provides:
    - pub(crate) FoldTier enum (Minimum / Medium / Large)
    - pub(crate) fn decide_fold_tier(rows: u16) -> FoldTier (locked D-13 boundaries 31→Min, 32→Med, 39→Med, 40→Large)
    - pub(crate) fn compose_mini(palette, roles) -> String (D-12 3-line list-dominant strip)
    - pub(crate) fn compose_full(palette, tier, roles, prompt_line_override) -> String (D-13 stacked preview with ◆ Heading labels + optional Plan 19-06 fork injection)
    - pub fn preview_panel::self_draw_prompt_from_sample_tokens(palette) -> String (D-04 self-draw fallback — new SWATCH-RENDERER-marked prompt line renderer)
    - Plan 19-05 render mode dispatch unblocked — compose_mini / compose_full ready to call from render::render
    - Plan 19-06 starship fork injection seam — compose_full accepts Option<&str> so fork can slot in without signature churn
  affects:
    - src/cli/picker/preview/compose.rs (skeleton → 439 lines: composer + placeholders + 9 unit tests)
    - src/cli/picker/preview_panel.rs (306 → 357 lines: new self_draw_prompt_from_sample_tokens + SWATCH-RENDERER marker)
tech_stack:
  added: []
  patterns:
    - "Pure-fn composer (palette + rows + roles → String; no I/O, no state mutation)"
    - "Responsive fold tier decision (rows → enum variant; exact boundary unit tests)"
    - "Optional prompt-line override param — fork-agnostic composer + Plan 19-06 seam"
    - "heading_text-style Option<&Roles<'_>> parameter mirroring src/cli/list.rs:73-78"
    - "SWATCH-RENDERER allowlist marker on self_draw_prompt_from_sample_tokens (same idiom blocks.rs + preview_panel::render_preview use)"
    - "UTF-8-safe strip_ansi test helper (iterates chars, NOT bytes, so multi-byte glyphs like ◆/❯ survive round-trip)"
key_files:
  created: []
  modified:
    - src/cli/picker/preview/compose.rs (skeleton 11 → 439 lines)
    - src/cli/picker/preview_panel.rs (306 → 357 lines; +51 lines for self_draw_prompt_from_sample_tokens + SWATCH-RENDERER marker)
decisions:
  - "Both tasks implemented in their own commits per plan layout (Task 19-04-01 atomic commit with full impl + 4 tests; Task 19-04-02 atomic commit with 5 additional heading/lavender/override tests). Followed Plan 19-02 GREEN-first-one-shot precedent at each task boundary: green between commits, no intermediate RED-only commit of self-invented failing contracts."
  - "Placeholder diff/lazygit/nvim renderers use palette field names directly (palette.green / palette.red / palette.blue / palette.magenta) per V-07 preflight. Confirmed real Palette struct at src/theme/mod.rs L53-70 uses semantic names; no ansi_00..ansi_15 API exists."
  - "self_draw_prompt_from_sample_tokens lives in preview_panel.rs (not compose.rs) because SAMPLE_TOKENS lives there — keeps the SAMPLE_TOKENS consumer co-located with the data it consumes, same locality convention blocks.rs follows for SemanticColor + file_type_colors."
  - "strip_ansi test helper iterates chars, not bytes — initial byte-based version broke multi-byte UTF-8 glyphs (◆/❯) during strip, causing 3 test failures that surfaced during Task 19-04-01's first full run. Fixed inline as a Rule 1 bug before commit."
  - "Mini-preview separator: 1 blank line (not a styled help-text line). D-12 text-level contract is 3-line strip; help text like '↑↓ theme · Tab fullscreen' is render.rs's responsibility — keeps compose.rs pure (no chrome copy embedded)."
  - "Docstring on self_draw_prompt_from_sample_tokens scrubbed of literal \\x1b[ byte sequences so the Phase 18 aggregate invariant scanner doesn't double-count docstring text as raw ANSI. Marker-based allowlist strips the fn body; docstring sits above the marker so it's scanned."
  - "#[allow(dead_code)] on FoldTier / decide_fold_tier / compose_mini / compose_full — Plan 19-05 (Wave 2 sibling, runs in parallel worktree + modifies src/cli/picker/render.rs) will call these and drop the markers. Without the attribute, clippy -D warnings fails at merge time because the symbols aren't consumed yet."
metrics:
  duration: "~55min (2026-04-20, work spanning branch rebase + dual TDD tasks + one Rule 1 inline fix)"
  tasks_completed: 2
  files_modified: 2
  files_created: 0
  commits: 2
  completed_date: "2026-04-20"
requirements: [DEMO-03]
---

# Phase 19 Plan 04: Wave 2 responsive-fold composer Summary

Populated the Plan 19-01 skeleton at `src/cli/picker/preview/compose.rs` with the full D-13 responsive-fold composer — `FoldTier` enum + `decide_fold_tier` + `compose_mini` + `compose_full` + `push_heading` + three placeholder block renderers (diff / lazygit / nvim) — plus a new `pub fn self_draw_prompt_from_sample_tokens(palette) -> String` in `src/cli/picker/preview_panel.rs` that composer calls as the D-04 Hybrid self-draw fallback. The composer is pure: palette + rows + Roles → String, no I/O, no state mutation. Plan 19-06 `starship_fork` will inject its forked prompt via `compose_full`'s optional `prompt_line_override` param, keeping the composer fork-agnostic and unit-snapshot-testable. All 9 unit tests green including the Phase 18 lavender-byte lock on `Roles::heading` output.

## Commits

| Commit    | Subject                                                                                    | Task     |
| --------- | ------------------------------------------------------------------------------------------ | -------- |
| `a5c1c11` | feat(19-04): implement FoldTier composer + self_draw_prompt_from_sample_tokens             | 19-04-01 |
| `3594e15` | test(19-04): add compose_full block-count invariants + lavender byte lock                  | 19-04-02 |

## What Shipped

### `src/cli/picker/preview/compose.rs` (skeleton 11 → 439 lines)

**Composer API (all `pub(crate)`):**
- `FoldTier { Minimum, Medium, Large }` — responsive-fold tier enum per D-13.
- `decide_fold_tier(rows: u16) -> FoldTier` — locked boundaries `0..=31→Minimum`, `32..=39→Medium`, `40..=∞→Large`. Tests enumerate 0/23/24/31/32/33/39/40/50/100.
- `compose_mini(palette, roles) -> String` — 3-line D-12 list-dominant strip: swatch row + self-drawn prompt + blank separator. `roles` accepted for signature symmetry with `compose_full` (currently unused; kept so Plan 19-05 doesn't have to branch on preview mode when materializing Roles).
- `compose_full(palette, tier, roles, prompt_line_override) -> String` — responsive-stacked preview. Every block prefixed with `◆ Heading` via `Roles::heading`. `Some(fork)` replaces the self-drawn prompt verbatim; `None` falls back to `self_draw_prompt_from_sample_tokens(palette)`.

**Internals:**
- `push_heading(out, roles, title)` — private helper mirroring `src/cli/list.rs:73-78::heading_text`. Routes through `Roles::heading` when `Some`, falls back to plain `◆ {title}` otherwise.
- `render_diff_placeholder(palette) -> String` — 2-line `+/-` diff shape in green/red (SWATCH-RENDERER marker).
- `render_lazygit_placeholder(palette) -> String` — 3-line unstaged-changes summary in blue (SWATCH-RENDERER marker).
- `render_nvim_placeholder(palette) -> String` — 1-line nvim-style `fn main() { println!("…"); }` in magenta + green (SWATCH-RENDERER marker).
- `rgb_fg(hex) -> String` — 24-bit FG escape builder with (128,128,128) gray fallback on malformed palette hex.

**Test module (9 tests, all green):**

| # | Test | Gate |
| - | ---- | ---- |
| 1 | `fold_thresholds_24_32_40` | VALIDATION row 10 — exact boundaries 0/23/24/31→Min, 32/33/39→Med, 40/50/100→Large |
| 2 | `mini_is_exactly_three_lines` | D-12 — `compose_mini` emits exactly 3 `\n` |
| 3 | `mini_contains_swatch_and_prompt` | D-12 — bg swatch bytes + fg prompt bytes + visible `❯` sigil all present |
| 4 | `self_draw_prompt_uses_sample_tokens` | D-04 — fg 24-bit bytes + at least one SAMPLE_TOKENS identifier; NO trailing newline |
| 5 | `compose_full_minimum_has_four_heading_labels` | D-13 Minimum — exactly 4 `◆ ` + label identities |
| 6 | `compose_full_medium_has_six_heading_labels` | D-13 Medium — exactly 6 `◆ ` (+ Git/Diff) |
| 7 | `compose_full_large_has_eight_heading_labels` | D-13 Large — exactly 8 `◆ ` (+ Lazygit/Nvim) |
| 8 | `heading_uses_roles_lavender_when_roles_some` | Phase 18 D-01 — `Roles::heading` output carries `38;2;114;135;253` (lavender `#7287fd`) |
| 9 | `prompt_override_replaces_self_draw` | Plan 19-06 seam — `Some(fork)` replaces self-draw verbatim; `❯` sigil absent |

Test helper `strip_ansi` iterates **chars** (not bytes) so multi-byte UTF-8 glyphs (`◆` / `❯`) survive CSI-sequence scrubbing — see Deviations §Rule 1 for the bug that prompted this.

### `src/cli/picker/preview_panel.rs` (306 → 357 lines)

Added one new public function:

- `pub fn self_draw_prompt_from_sample_tokens(palette: &Palette) -> String` — iterates the Phase 15 `SAMPLE_TOKENS` prompt prefix (stops at first `"\n"` marker), emits 24-bit fg ANSI per `palette.resolve(role)`, then appends a conventional `❯` sigil via `SemanticColor::Prompt`. NO trailing newline (caller owns newline policy). Marked `// SWATCH-RENDERER:` so the Phase 18 aggregate invariant scanner skips the fn body.

No existing `preview_panel.rs` surface touched (`SemanticColor` / `SAMPLE_TOKENS` / `PreviewSpan` / `render_preview` unchanged, 3 pre-existing tests still green).

## Verification Results

| Gate | Result | Details |
| ---- | ------ | ------- |
| `cargo test --lib picker::preview::compose` | GREEN | 9/9 (4 from Task 19-04-01, 5 from Task 19-04-02) |
| `cargo test --lib` | GREEN | 788 passed / 0 failed / 0 ignored (Wave 1 baseline 776 → +9 compose; +3 delta is from other files' test discovery, unrelated) |
| `cargo test --test theme_tests` | GREEN | 12 passed — includes Plan 19-01 retirement invariant |
| `cargo test --test integration_tests` | GREEN | 67 passed |
| `cargo build --release` | GREEN | ~20s incremental |
| `cargo clippy --all-targets -- -D warnings` | GREEN | Zero warnings |
| `rustfmt --check --edition 2021 src/cli/picker/preview/compose.rs src/cli/picker/preview_panel.rs` | GREEN | Our two touched files are fmt-clean |
| `cargo fmt --check` (whole workspace) | PRE-EXISTING DRIFT | 4 diff locations in `src/brand/{render_context,roles}.rs`. Confirmed identical to Wave 0 / Wave 1 SUMMARY reports (base commit 437da1e). Not introduced by this plan. Out of scope per SCOPE BOUNDARY. |
| Phase 18 aggregate: `no_raw_styling_ansi_anywhere_in_user_surfaces` | GREEN | `// SWATCH-RENDERER:` marker on `self_draw_prompt_from_sample_tokens` keeps scanner clean. Docstring was scrubbed of literal `\x1b[` byte-sequences after initial run flagged them as raw ANSI (Rule 1 inline fix). |
| Wave 5 gate: `no_raw_ansi_in_wave_5_files` | GREEN | Same marker allowlist. |

## Deviations from Plan

### Rule 1 — Bug: strip_ansi helper corrupted multi-byte UTF-8 glyphs

- **Found during:** Task 19-04-01 first full test run (9 tests executed at once before commit split).
- **Issue:** Initial `strip_ansi` iterated `s.as_bytes()` and did `out.push(bytes[i] as char)`, which treats each byte as a Latin-1 codepoint. Multi-byte UTF-8 glyphs like `◆` (U+25C6, 3 bytes `E2 97 86`) and `❯` (U+276F, 3 bytes `E2 9D AF`) survived CSI-sequence skipping intact but then got mangled during the character emission — `◆` became `â\u{97}\u{86}`. Four tests failed (`mini_contains_swatch_and_prompt` + 3 `compose_full_*_heading_labels`).
- **Why Rule 1:** Bug in test infrastructure written in this plan. Fixing prevents false-negative silencing of real contracts (the heading labels DID survive; only the test's own stripper was broken).
- **Fix:** Rewrote `strip_ansi` to iterate `s.chars().peekable()` and skip CSI sequences by char-classification (`(0x40..=0x7e).contains(&code)`) instead of byte indexing. All 9 tests green after fix.
- **Files modified:** `src/cli/picker/preview/compose.rs` (strip_ansi helper inside `mod tests`).
- **Commit:** Fix was bundled into `a5c1c11` (Task 19-04-01) — the broken-stripper version never left the working tree.

### Rule 1 — Bug: docstring literal `\x1b[` sequences tripped the Phase 18 aggregate invariant

- **Found during:** full `cargo test --lib` run after Task 19-04-01 commit (not caught by targeted compose tests).
- **Issue:** `self_draw_prompt_from_sample_tokens` docstring included the literal text `"emit 24-bit foreground ANSI (\x1b[38;2;R;G;B m)"`. The Phase 18 aggregate invariant's `count_style_ansi_in` scanner counts `\x1b[` string occurrences in filtered (non-SWATCH-RENDERER) regions — docstrings above the marker are scanned, so the literal byte-sequence inside the docstring showed up as 1 styling residue. Both `no_raw_ansi_in_wave_5_files` and `no_raw_styling_ansi_anywhere_in_user_surfaces` failed on `src/cli/picker/preview_panel.rs`.
- **Why Rule 1:** Pure bug in documentation — the scanner's behavior is correct (docstring text CAN contain real ANSI at build-time if someone uses `concat!` or includes), so scrubbing the docstring is the right fix, not relaxing the scanner.
- **Fix:** Rewrote the docstring paragraph to describe the behavior prose-style ("emits 24-bit foreground SGR bytes") without including any `\x1b[` byte sequences. Scanner re-runs green.
- **Files modified:** `src/cli/picker/preview_panel.rs` (docstring only; fn body unchanged).
- **Commit:** Bundled into `a5c1c11` (Task 19-04-01) — discovered during the same session before the Task 01 git add, so never shipped as a red commit.

### Signature pragmatism: `compose_mini` accepts `_roles` it doesn't use (yet)

- **Plan intent:** Plan 19-04 `must_haves` says `compose_mini(palette, roles)`.
- **Implementation:** `compose_mini(palette: &Palette, _roles: Option<&Roles<'_>>) -> String`. The underscore prefix silences the unused-param warning while keeping the signature the plan specified.
- **Rationale:** mini-preview has no `◆ Heading` labels so there's nothing to route through Roles today. But Plan 19-05 will call `compose_mini` from the same render-mode dispatcher that calls `compose_full` — keeping the signatures parallel means render.rs doesn't branch on `preview_mode` when materializing a Roles handle, and future D-12 chrome lines can pick up roles with zero signature churn.

## V-05 / V-07 Compliance

- **V-05 (files_modified frontmatter):** Plan 19-04 `files_modified` already lists `src/cli/picker/preview/compose.rs` AND `src/cli/picker/preview_panel.rs`. Wave 2 sibling plan 19-05 modifies `src/cli/picker/render.rs` only — no file overlap, parallel worktree execution stays clean. Verified via the prompt's non-overlap note.
- **V-07 (real palette field names):** All placeholder renderers use `palette.green` / `palette.red` / `palette.blue` / `palette.magenta` — confirmed against `src/theme/mod.rs:53-70`. No `ansi_00..ansi_15` API exists; the plan's action block correctly warned about this as a preflight check.

## Known Stubs

- `src/cli/picker/preview/compose.rs` `render_diff_placeholder` / `render_lazygit_placeholder` / `render_nvim_placeholder` — intentional 2-3 line placeholder bodies. Not real diff / lazygit / nvim renderers. If UAT shows these are too sparse, richer analogs (diff hunk parsing, lazygit TUI mock, nvim syntax snippet) can land in a follow-up. Documented in the module docstring.
- Sibling skeletons still awaiting their fill plans:
  - `src/cli/picker/preview/starship_fork.rs` — Plan 19-06 Wave 3.
  - `src/cli/picker/rollback_guard.rs` — populated by Plan 19-03 (Wave 1 sibling, already landed in main).

## Call-Sites to Wire in Plan 19-05

Plan 19-05 (render mode dispatch) will:

1. At the top of `src/cli/picker/render.rs::render(...)`, read `state.preview_mode_full` (field added by Plan 19-03) and branch:
   ```rust
   if state.preview_mode_full {
       // Clear alt-screen, call compose_full
       let tier = super::preview::compose::decide_fold_tier(rows);
       let out = super::preview::compose::compose_full(
           palette, tier, roles.as_ref(), /*prompt_line_override=*/ None,
       );
       print!("{out}");
   } else {
       // Existing list rendering + mini-preview footer strip
       let mini = super::preview::compose::compose_mini(palette, roles.as_ref());
       print!("{mini}");
       // Plus the "↑↓ theme · Tab fullscreen" help line
   }
   ```
2. Remove the four `#[allow(dead_code)]` attributes on `FoldTier` / `decide_fold_tier` / `compose_mini` / `compose_full` once the wiring lands.

Plan 19-06 (starship fork) will:

1. Inject a forked prompt into `compose_full(..., Some(fork_line))` after a successful `fork_starship_prompt(...)` call — no signature churn needed.

## Deferred Items

| Item | Reason | Owner |
| ---- | ------ | ----- |
| Richer diff / lazygit / nvim renderers | Placeholder bodies documented in compose.rs module docstring; replace if UAT shows sparse-ness | Phase 20 or dedicated follow-up after UAT |
| `cargo fmt --check` drift in `src/brand/{render_context,roles}.rs` | Pre-existing on base commit 437da1e (documented in Wave 0 / Wave 1 SUMMARY); SCOPE BOUNDARY | Whoever touches those files next |
| `#[allow(dead_code)]` removal on FoldTier / decide_fold_tier / compose_mini / compose_full | Attributes drop when Plan 19-05 wires render.rs dispatch | Plan 19-05 |
| `#[allow(dead_code)]` on `RollbackGuard` (from Plan 19-03) | Sibling Wave-1 artifact; drops in Plan 19-07 launch_picker wiring | Plan 19-07 |

## Threat Flags

None. The composer is pure (no I/O, no subprocess, no file system, no network). Placeholder renderers consume palette hex fields only; all field accesses have graceful fallback paths (`PaletteRenderer::hex_to_rgb` returns `Err` on malformed → `rgb_fg` substitutes gray `(128,128,128)`). Plan 19-04's `<threat_model>` listed one T-19-04-01 (palette → ANSI trust boundary, accepted) — still accepted; mitigation (gray fallback) implemented.

## Self-Check: PASSED

- FOUND: `src/cli/picker/preview/compose.rs` (439 lines, `pub(crate) FoldTier` + `decide_fold_tier` + `compose_mini` + `compose_full` + `push_heading` + 3 placeholder renderers + `rgb_fg` + 9 tests)
- FOUND: `src/cli/picker/preview_panel.rs` (357 lines; added `pub fn self_draw_prompt_from_sample_tokens`)
- FOUND: commit `a5c1c11` (Task 19-04-01)
- FOUND: commit `3594e15` (Task 19-04-02)
- CONFIRMED: `cargo test --lib picker::preview::compose` 9 passed
- CONFIRMED: `cargo test --lib` 788 passed / 0 failed
- CONFIRMED: `cargo test --test theme_tests` 12 passed
- CONFIRMED: `cargo test --test integration_tests` 67 passed
- CONFIRMED: `cargo build --release` green
- CONFIRMED: `cargo clippy --all-targets -- -D warnings` green
- CONFIRMED: `rustfmt --check` on our two touched files clean
- CONFIRMED: Phase 18 aggregate invariant `no_raw_styling_ansi_anywhere_in_user_surfaces` + Wave-5 per-file gate `no_raw_ansi_in_wave_5_files` both green after SWATCH-RENDERER marker + docstring scrub
- CONFIRMED: Plan 19-01 retirement invariant `slate_demo_surface_stays_retired_post_phase_19` still green (no accidental re-introduction of removed symbols)
