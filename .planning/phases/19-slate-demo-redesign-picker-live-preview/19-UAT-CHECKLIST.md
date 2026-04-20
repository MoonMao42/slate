# Phase 19 · Manual UAT Checklist

**Use:** `/gsd-verify-work 19` runs these after every automated Wave-6 test
passes. Each row is a self-contained scenario with a concrete action, an
expected-outcome line, and a pass/fail slot.

**Environment prerequisites:**
- macOS + Ghostty terminal (primary target per Phase 19 CONTEXT)
- `starship --version` ≥ 1.0 on PATH (for UAT-2)
- Slate registry includes `catppuccin-mocha`, `catppuccin-frappe`,
  `tokyo-night-storm` (at minimum — the 18-theme default bundle covers this)
- A release binary built from the current commit: `cargo build --release`

---

## UAT-1 · Ghostty real-reload smoothness (D-01)

**Goal:** Verify Ghostty background switches without flicker as the user
holds ↓ through the picker list.

**Steps:**
1. Open a Ghostty terminal window (any committed theme).
2. Run `./target/release/slate theme` (no args → launches the picker).
3. Hold ↓ for ~5 seconds so the cursor sweeps through every variant in
   the list.
4. Observe the Ghostty window background while the list updates.

**Expected:**
- Background color transitions smoothly at each row.
- No flicker, no visible "lag then jump" pattern.
- Latency feels ≤ 50 ms per variant (Phase 19 D-01 budget).

**Pass / Fail:** ⬜

**Notes:**

---

## UAT-2 · Tab full-mode starship fork matches the real prompt (D-04 Hybrid)

**Goal:** Verify the Tab full-screen preview's `◆ Prompt` row matches
what the user actually sees in a fresh shell session.

**Steps:**
1. Confirm `starship --version` returns ≥ 1.0 and the binary is on PATH.
2. Run `./target/release/slate theme`.
3. Navigate with ↓/↑ until `catppuccin-mocha` is highlighted.
4. Press **Tab** to enter full-preview mode.
5. Locate the `◆ Prompt` block row inside the preview.
6. Open a fresh Ghostty shell (⌘N) and compare its starship prompt to
   the one inside the full-preview.

**Expected:**
- The prompt row in full-preview matches the fresh-shell prompt:
  directory segment, git segment (if applicable), prompt character,
  language icons.
- If starship is absent from PATH, preview falls back to the self-drawn
  style — still coherent, just simpler.

**Pass / Fail:** ⬜

**Notes:**

---

## UAT-3 · Esc rollback visually reverts Ghostty bg (D-11 layer 1)

**Goal:** Verify the Esc rollback path (active, non-panic) visually
returns Ghostty's background to the pre-picker color.

**Steps:**
1. Note the current Ghostty background color before launching the picker.
2. Run `./target/release/slate theme`.
3. Press ↓ five times so the bg drifts to a new theme.
4. Press **Esc**.

**Expected:**
- Ghostty bg returns to the pre-picker color within ~50 ms.
- No stale `managed/ghostty/*.conf` content leaves the window in the
  drifted theme.

**Pass / Fail:** ⬜

**Notes:**

---

## UAT-4 · Ctrl+C mid-nav → managed/* rolled back (D-11 layer 2)

**Goal:** Verify SIGINT triggers the Drop-based rollback (release build
uses the panic hook path).

**Steps:**
1. Build a release binary: `cargo build --release`.
2. Run `./target/release/slate theme`.
3. Press ↓ three times so managed/* drifts.
4. Press **Ctrl+C** while the picker is still open.
5. Run `cat ~/.config/slate/managed/ghostty/theme.conf`.

**Expected:**
- The file's palette block matches the ORIGINAL theme (pre-picker), not
  the drifted variant.
- If the palette matches the drifted variant, the triple-guard rollback
  layer 2 failed — check the panic hook is installed and the `panic =
  "abort"` profile isn't stripping it.

**Pass / Fail:** ⬜

**Notes:**

---

## UAT-5 · `slate demo` command no longer exists (D-05)

**Goal:** Verify the CLI surface rejects the retired `slate demo`
subcommand.

**Steps:**
1. Run `./target/release/slate demo`.

**Expected:**
- stderr prints `error: unrecognized subcommand 'demo'` (clap default).
- Exit code is non-zero.
- No renderer runs; no deprecation hint surfaces.

**Pass / Fail:** ⬜

**Notes:**

---

## Sign-off

- Tester: ____________
- Date: ____________
- Build (`git rev-parse --short HEAD`): ____________
- Overall: ⬜ pass / ⬜ fail

If any row fails, either open an issue or append a "blockers" section at
the bottom of this file pointing at the failing Plan / Task in Phase 19.
Failures do NOT auto-trigger `/gsd-plan-phase --gaps`; the user decides
whether to file gap-closure work.
