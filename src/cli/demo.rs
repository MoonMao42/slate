//! `slate demo` — single-screen palette showcase (Phase 15).
//!
//! Renders a curated read-only demo of the active palette (code snippet, file
//! tree, git-log excerpt, progress bar) in well under 1 second with no
//! network / external-tool calls. Also hosts the DEMO-02 session-local hint
//! emitter consumed from `slate setup` and `slate theme <id>`.

use crate::error::Result;
use crate::theme::Palette;
use std::sync::atomic::{AtomicBool, Ordering};

static HINT_EMITTED: AtomicBool = AtomicBool::new(false);

/// Top-level entry point for `slate demo`. Stub — Plan 03 (Wave 2) fills body.
pub fn handle() -> Result<()> {
    unimplemented!("slate demo renderer lands in Plan 03")
}

/// Pure render entry point — no stdout, no size gate. Used by unit tests and
/// the criterion bench to measure rendering cost without I/O. Stub — Plan 03
/// (Wave 2) fills body.
pub fn render_to_string(_palette: &Palette) -> String {
    String::new()
}

/// Emit the DEMO-02 hint once per process. Suppressed when auto || quiet.
/// Stub — Plan 03 (Wave 2) fills body.
pub fn emit_demo_hint_once(_auto: bool, _quiet: bool) {
    // STUB
    let _ = HINT_EMITTED.load(Ordering::SeqCst);
}

/// Mark the hint as already-emitted for this process, so downstream call sites
/// skip emission. Used by `slate set` to prevent demo-hint + deprecation-tip
/// co-occurrence (per D-C3). Plan 03 (Wave 2) may refine.
pub fn suppress_demo_hint_for_this_process() {
    HINT_EMITTED.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore] // Wave 0 stub — Plan 03 fills real test
    fn render_to_string_emits_ansi_escape() {
        // placeholder — real test in Plan 03 verifies ANSI 24-bit escapes present
    }

    #[test]
    #[ignore] // Wave 0 stub — Plan 03 fills real test
    fn demo_hint_dedup_atomicbool() {
        // placeholder — real test in Plan 03 verifies second call is a no-op
    }
}
