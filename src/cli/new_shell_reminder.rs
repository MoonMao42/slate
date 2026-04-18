//! Platform-aware reveal-framed "open a new shell" reminder emitted at the
//! tail of `slate setup` / `slate theme` / `slate font` / `slate config` when
//! any successful adapter declared `RequiresNewShell` (Phase 16 D-D1/D-D4/D-D5).
//!
//! Emitted at most once per process. Mirrors `src/cli/demo.rs::emit_demo_hint_once`
//! exactly: session-local `AtomicBool` flag, `--auto` / `--quiet` suppression
//! checked BEFORE the flag swap so suppressed calls don't burn the flag
//! (RESEARCH §Pitfall 1).
//!
//! The copy itself lives in `Language::new_shell_reminder()` (platform-aware;
//! macOS gets `⌘N`, Linux gets "terminal"). This module owns only dedup +
//! emission sequencing.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::brand::language::Language;
use crate::design::typography::Typography;

static REMINDER_EMITTED: AtomicBool = AtomicBool::new(false);

/// Emit the new-terminal reveal reminder at most once per process.
///
/// Order of checks (load-bearing per RESEARCH §Pitfall 1):
/// 1. Early-return on `auto || quiet` WITHOUT touching the flag — this is
///    what protects `slate theme --auto --quiet` (the Ghostty watcher path)
///    from emitting OR consuming the one-shot flag.
/// 2. Swap the flag; if it was already `true`, we've emitted this process.
/// 3. Print a blank line, then the `Typography::explanation`-wrapped reminder.
pub fn emit_new_shell_reminder_once(auto: bool, quiet: bool) {
    if auto || quiet {
        return;
    }
    if REMINDER_EMITTED.swap(true, Ordering::SeqCst) {
        return; // already emitted in this process
    }
    println!();
    println!(
        "{}",
        Typography::explanation(Language::new_shell_reminder())
    );
}

/// Test-only: reset the once-flag so ordered tests can verify suppression
/// paths without relying on process-start state. Not exposed in release
/// builds — the production flow never needs to reset.
#[cfg(test)]
pub(crate) fn reset_reminder_flag_for_tests() {
    REMINDER_EMITTED.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// All four tests touch the same process-wide `REMINDER_EMITTED` flag.
    /// Cargo runs tests in parallel by default, so without serialization the
    /// `reset → emit → assert` sequence in one test can race a `reset` in
    /// another and the "flag is true after emit" assertion flips red.
    ///
    /// `serial_test` is not a current dev-dependency; this hand-rolled mutex
    /// keeps the fix local and avoids adding a crate for four tests.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// We assert flag state directly rather than capturing stdout: stdout
    /// capture across threads is finicky, and the flag is the load-bearing
    /// state per §Pitfall 1 (did the early-return happen BEFORE the swap?).
    #[test]
    fn reminder_emits_once_per_process() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        // First call: flag flips to true.
        emit_new_shell_reminder_once(false, false);
        assert!(
            REMINDER_EMITTED.load(Ordering::SeqCst),
            "first non-suppressed call must set the flag"
        );
        // Second call is a no-op — the swap returns the previous value (true),
        // so the println! branch is skipped. Flag stays true.
        emit_new_shell_reminder_once(false, false);
        assert!(
            REMINDER_EMITTED.load(Ordering::SeqCst),
            "flag must remain set after the second call"
        );
    }

    #[test]
    fn reminder_suppressed_by_auto() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        emit_new_shell_reminder_once(true, false);
        // Critical §Pitfall 1 assertion: the early return MUST happen before
        // the swap, so `auto=true` leaves the flag in its reset state. If a
        // future refactor moves the swap above the `if auto || quiet` guard,
        // this test flips red.
        assert!(
            !REMINDER_EMITTED.load(Ordering::SeqCst),
            "auto=true must NOT touch the flag (early-return before swap)"
        );
    }

    #[test]
    fn reminder_suppressed_by_quiet() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        emit_new_shell_reminder_once(false, true);
        assert!(
            !REMINDER_EMITTED.load(Ordering::SeqCst),
            "quiet=true must NOT touch the flag (early-return before swap)"
        );
    }

    #[test]
    fn reminder_flag_state_after_successful_emit() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        assert!(
            !REMINDER_EMITTED.load(Ordering::SeqCst),
            "reset must leave the flag false"
        );
        emit_new_shell_reminder_once(false, false);
        assert!(
            REMINDER_EMITTED.load(Ordering::SeqCst),
            "successful emit must transition the flag to true"
        );
    }
}
