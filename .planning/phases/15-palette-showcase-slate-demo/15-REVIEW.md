---
phase: 15-palette-showcase-slate-demo
reviewed: 2026-04-18T00:00:00Z
depth: standard
files_reviewed: 13
files_reviewed_list:
  - src/cli/picker/preview_panel.rs
  - src/theme/mod.rs
  - src/design/mod.rs
  - src/design/file_type_colors.rs
  - src/cli/demo.rs
  - src/cli/mod.rs
  - src/brand/language.rs
  - src/main.rs
  - src/cli/setup.rs
  - src/cli/theme.rs
  - src/cli/set.rs
  - tests/integration_tests.rs
  - benches/performance.rs
findings:
  critical: 0
  warning: 2
  info: 6
  total: 8
status: issues_found
---

# Phase 15: Code Review Report

**Reviewed:** 2026-04-18T00:00:00Z
**Depth:** standard
**Files Reviewed:** 13
**Status:** issues_found

## Summary

Phase 15 adds `slate demo` — a one-screen palette showcase — and hooks a session-local hint emitter into `slate setup` / `slate theme <id>`. The core design concerns called out in the review brief are met: the 14 new `SemanticColor` variants are exhaustively covered by `Palette::resolve` (no `_ =>` catch-all — line 147 uses a full `match`), `demo.rs` has no hex literals (all colors flow through `palette.resolve(…)` or direct named field access such as `palette.bright_red`), `file_type_colors::classify` is deterministic and order-locked by static slice, and `emit_demo_hint_once` uses `AtomicBool::swap(true, SeqCst)` for once-only emission with auto/quiet suppression wired at all call sites.

The findings below are minor: a text typo in a comment, a subtle extras-wrap edge case, two test-scope hygiene concerns around process-global `AtomicBool` sharing, and a few maintainability notes. No critical or security issues.

## Warnings

### WR-01: Process-global `HINT_EMITTED` creates test-order coupling

**File:** `src/cli/demo.rs:20, 528-548`
**Issue:** `HINT_EMITTED` is a process-global `AtomicBool` shared by every test in `mod tests`. The comment at line 531 and line 544 acknowledges this, but the test module still calls `emit_demo_hint_once` and `suppress_demo_hint_for_this_process` without resetting the flag. Specifically:

- `emit_demo_hint_once_auto_is_silent` (528) and `emit_demo_hint_once_quiet_is_silent` (537) pass because the `auto || quiet` early-return runs before the swap, so the flag isn't touched.
- `suppress_demo_hint_marks_emitted_flag` (542) sets the flag to `true`.
- If `cargo test` reorders and runs `suppress_demo_hint_marks_emitted_flag` first, nothing observable changes (the two `auto/quiet` tests still pass). But any future test that wants to assert the **first** `emit_demo_hint_once(false, false)` call actually prints will fail non-deterministically depending on test ordering.

This is a future-hazard, not a current bug. The cleanest fix is either (a) serialize these tests with a `Mutex` + explicit reset, or (b) refactor the emitter to take an injected `&AtomicBool` so tests can pass a fresh one.

**Fix:**
```rust
// Option (b) — allow test injection without touching the public API:
pub fn emit_demo_hint_once(auto: bool, quiet: bool) {
    emit_hint_once_with_flag(auto, quiet, &HINT_EMITTED)
}

fn emit_hint_once_with_flag(auto: bool, quiet: bool, flag: &AtomicBool) {
    if auto || quiet { return; }
    if flag.swap(true, Ordering::SeqCst) { return; }
    println!();
    println!("{}", Typography::explanation(Language::DEMO_HINT));
}

#[cfg(test)]
mod tests {
    // Each test constructs its own AtomicBool and calls the inner fn directly.
}
```

### WR-02: Extras matrix can emit a trailing empty indented line

**File:** `src/cli/picker/preview_panel.rs:211-227`
**Issue:** The wrap logic at lines 222-224 pushes `"\n        "` after every 8 extras. If the number of extras is an exact multiple of 8 (8, 16, 24, …), this inserts the indent **after** the last entry, and then line 227's unconditional `output.push('\n')` produces a final line that contains only whitespace. Visually harmless in most terminals but will show as a ragged empty row in status-bar-style tight layouts and breaks the "each line fits 80 cols" invariant the demo block observes elsewhere.

Additionally, line 211 has a cosmetic typo in the comment — `"present conditional)"` should be `"(conditional)"` or similar.

**Fix:**
```rust
// Render extras matrix (conditional)
if !palette.extras.is_empty() {
    output.push_str("Extras: ");
    let mut sorted_extras: Vec<_> = palette.extras.iter().collect();
    sorted_extras.sort_by_key(|(name, _)| *name);
    for (i, (name, color)) in sorted_extras.iter().enumerate() {
        if i > 0 && i % 8 == 0 {
            output.push_str("\n        ");
        }
        output.push_str(&bg(color));
        output.push_str(&format!(" {} ", name));
        output.push_str(RESET);
        output.push(' ');
    }
    output.push('\n');
}
```

## Info

### IN-01: Typo in source comment

**File:** `src/cli/picker/preview_panel.rs:211`
**Issue:** `// Render extras matrix if presentconditional)` — stray `)` and missing space/word. Not a bug but it's in committed code.
**Fix:** Replace with `// Render extras matrix (conditional on palette having extras)`.

### IN-02: `fg()` swallows hex-parse errors silently

**File:** `src/cli/demo.rs:30-35`
**Issue:** The doc comment ("Returns an empty string on malformed input — which would be a palette / theme-file bug, not a user-facing error — so the demo degrades to uncolored text rather than crashing") justifies the silent fallback, but the function doesn't debug-assert or log. Because every theme is validated at registry load (`Palette::validate()`), any `Err` from `hex_to_rgb` here is a logic bug somewhere upstream and would ship silently. Consider `debug_assert!(matches!(PaletteRenderer::hex_to_rgb(hex), Ok(_)), ...)` so the dev build surfaces the bug loudly while the release build stays graceful.
**Fix:**
```rust
fn fg(hex: &str) -> String {
    match PaletteRenderer::hex_to_rgb(hex) {
        Ok((r, g, b)) => format!("\x1b[38;2;{r};{g};{b}m"),
        Err(e) => {
            debug_assert!(false, "demo: invalid palette hex '{hex}': {e}");
            String::new()
        }
    }
}
```

### IN-03: `Palette::resolve` clones hex strings on every call

**File:** `src/theme/mod.rs:144-194`
**Issue:** Each arm of the match returns `self.foo.clone()`. For the demo renderer this runs 7+ times per block × 4 blocks = ~30+ allocations per render. The bench at `benches/performance.rs:29` currently measures total render; current numbers satisfy the <1s budget, so this is not a bug. A zero-cost alternative would be returning `&str`:

```rust
pub fn resolve(&self, role: SemanticColor) -> &str {
    match role { … => &self.blue, … }
}
```

Callers in `demo.rs` (e.g. `let kw = palette.resolve(SemanticColor::Keyword);`) would need to take `&kw` where `&String` is accepted. Deferring — flagged only because `Palette::resolve` is the single hottest function on the demo path and future cross-file consumers (Phase 16 `LS_COLORS` generator) will call it many more times.

**Fix:** Change signature to return `&str` in a future pass; keep the current contract for this phase.

### IN-04: `FULL_NAME_MAP` lookup is O(n) per call — fine today, document the limit

**File:** `src/design/file_type_colors.rs:44-48, 75-80`
**Issue:** `FULL_NAME_MAP` is a 4-entry static slice scanned linearly. That's cheaper than a HashMap for n ≤ 4 and is the right choice now, but Phase 16 will re-use this classifier for `LS_COLORS` / `EZA_COLORS` generation where list sizes can balloon (gitignore-style patterns, framework-specific manifest files, etc). Add a comment marking the threshold where the data structure should flip to a HashMap, so the next author doesn't have to re-derive the tradeoff.
**Fix:** Add a doc comment on `FULL_NAME_MAP` noting "linear scan; switch to `&'static phf::Map` or `HashMap` if this grows past ~16 entries."

### IN-05: `suppress_demo_hint_for_this_process` has asymmetric guards vs `emit_demo_hint_once`

**File:** `src/cli/demo.rs:365-381`
**Issue:** `emit_demo_hint_once` has a public `auto: bool, quiet: bool` contract, but `suppress_demo_hint_for_this_process` takes no arguments — it's called unconditionally from `src/cli/set.rs:18`. That's the intended semantics per D-C3, but it means a future caller could call `suppress_…` and accidentally mask a legitimate hint. There's no test guarding against "does `slate set` also silence a later `slate theme` call in the same process?" — though since these are CLI invocations with a fresh process each, the risk is only in test scaffolding that invokes two handlers back-to-back.
**Fix:** No code change; add an integration test that invokes `slate set <theme>` (which calls `suppress_demo_hint_for_this_process`) in a separate `assert_cmd::Command` from a subsequent `slate theme <theme>` call, asserting the second call still emits the hint (baseline sanity).

### IN-06: `render_code_block` inlines a literal `42` "retries" default

**File:** `src/cli/demo.rs:137`
**Issue:** `"42"` is a magic number in the rendered string. Acceptable for a hand-curated demo (it's deliberately chosen to land on the Number slot), but worth a comment pointing at the D-B4 sample-data spec so the next reader understands it's a contract value, not an accident.
**Fix:** Add a trailing `// 42 chosen per D-B4 sample data — lights up the Number (yellow) slot` above line 137, or reference the plan file in the existing block-level doc comment.

---

_Reviewed: 2026-04-18T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
