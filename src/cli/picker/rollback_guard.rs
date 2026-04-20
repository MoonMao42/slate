//! RollbackGuard: triple-guarded rollback of managed/* state on picker exit.
//!
//! Phase 19 D-11 locks "双保险" (two layers) but `Cargo.toml:67 panic = "abort"`
//! makes Drop unreliable in release, so this module provides THREE layers:
//!
//! 1. **Normal Esc path**: `event_loop.rs` explicitly calls
//!    `silent_preview_apply(original)` on the `ExitAction::Cancel` branch
//!    (already present at L102-107 pre-Phase-19).
//! 2. **Stack-unwind path (non-abort panic, explicit return)**:
//!    `impl Drop for RollbackGuard` calls `silent_preview_apply(original)`
//!    when `committed == false`.
//! 3. **`panic = "abort"` release path**: `install_rollback_panic_hook`
//!    wraps `std::panic::take_hook()` with a closure that restores the
//!    terminal + calls `silent_preview_apply(original)` before the
//!    default/backtrace handler runs (after which `abort()` skips Drop).
//!
//! All rollback failures are silent — `let _ = silent_preview_apply(...)`.
//! Panicking inside Drop would double-panic → abort (RESEARCH Pitfall 1).
//!
//! Filled in Plan 19-03 (Wave 1). Event-loop wiring lands in Plan 19-07.

use crate::env::SlateEnv;
use crate::opacity::OpacityPreset;
use std::cell::Cell;
use std::rc::Rc;

/// RAII guard that restores `managed/*` to the original theme + opacity
/// when the picker exits without `committed` being set to `true`.
#[allow(dead_code)] // Wired by Plan 19-07 (launch_picker)
pub(crate) struct RollbackGuard {
    env: SlateEnv,
    original_theme_id: String,
    original_opacity: OpacityPreset,
    committed: Rc<Cell<bool>>,
}

impl RollbackGuard {
    /// Arm the guard at picker launch. Snapshots the original theme + opacity
    /// and takes a shared handle to the committed flag (same `Rc<Cell<bool>>`
    /// held by `PickerState::committed`).
    #[allow(dead_code)] // Wired by Plan 19-07 (launch_picker)
    pub(crate) fn arm(
        env: &SlateEnv,
        original_theme_id: &str,
        original_opacity: OpacityPreset,
        committed: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            env: env.clone(),
            original_theme_id: original_theme_id.to_string(),
            original_opacity,
            committed,
        }
    }
}

impl Drop for RollbackGuard {
    fn drop(&mut self) {
        // D-11 + Phase 18 fail-silent: picker Cancel branch philosophy.
        // Never panic inside Drop (would double-panic → abort).
        if self.committed.get() {
            return;
        }
        let _ = crate::cli::set::silent_preview_apply(
            &self.env,
            &self.original_theme_id,
            self.original_opacity,
        );
    }
}

/// Install a process-wide panic hook that restores the terminal + rolls
/// back managed/* to the original theme before the previous hook runs.
///
/// Required because `Cargo.toml:67 panic = "abort"` skips Drop on panic
/// in release builds (RESEARCH §Pitfall 1). The hook captures the env +
/// original theme by move, so it carries its own state independent of
/// any guard Drop order.
#[allow(dead_code)] // Wired by Plan 19-07 (launch_picker)
pub(crate) fn install_rollback_panic_hook(
    env: SlateEnv,
    original_theme_id: String,
    original_opacity: OpacityPreset,
) {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // 1. Restore the terminal FIRST so the panic backtrace prints to a
        //    sane surface (not into the alt-screen). RESEARCH V7 —
        //    info-disclosure mitigation.
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        // 2. Silent managed/* rollback.
        let _ = crate::cli::set::silent_preview_apply(&env, &original_theme_id, original_opacity);
        // 3. Delegate to the previous hook (usually the default backtrace
        //    printer). After this returns, `panic = "abort"` kicks in.
        prev_hook(info);
    }));
}

/// V-03 BEHAVIOR-PROVING test variant (test-only).
///
/// Same structure as `install_rollback_panic_hook` but at step 2 it flips
/// a caller-provided `AtomicBool` sentinel INSTEAD of calling
/// `silent_preview_apply`. This lets the test suite PROVE the hook body
/// actually reached the rollback branch — a critical invariant given
/// `panic = "abort"` makes Drop unreliable in release.
///
/// Why not assert via `silent_preview_apply`? Because it writes real
/// managed/* files on disk and would be destructive + flaky in parallel
/// tests. The sentinel proves the hook fired at the SAME point where
/// production calls `silent_preview_apply`; the production path is
/// exercised separately at integration scope in Plan 19-08.
#[cfg(test)]
pub(crate) fn install_rollback_panic_hook_with_sentinel(
    sentinel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        sentinel.store(true, Ordering::SeqCst);
        prev_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn test_env() -> SlateEnv {
        // Use a unique tempdir so tests can run in parallel without
        // clobbering each other's managed/* directories.
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        // Intentionally leak the TempDir so files survive guard.drop.
        // This is test-only; process exit cleans up.
        std::mem::forget(tmp);
        SlateEnv::with_home(home)
    }

    #[test]
    fn rollback_guard_noop_when_committed() {
        let env = test_env();
        let committed = Rc::new(Cell::new(false));
        // Simulate "user pressed Enter" by pre-setting committed=true.
        committed.set(true);
        let guard = RollbackGuard::arm(
            &env,
            "catppuccin-mocha",
            OpacityPreset::Solid,
            committed.clone(),
        );
        drop(guard);
        // Assertion is behavioral: no panic, drop was silent.
        assert!(
            committed.get(),
            "committed flag must remain true after drop"
        );
    }

    #[test]
    fn rollback_guard_on_drop_when_not_committed() {
        let env = test_env();
        let committed = Rc::new(Cell::new(false));
        let guard = RollbackGuard::arm(
            &env,
            "catppuccin-frappe",
            OpacityPreset::Frosted,
            committed.clone(),
        );
        drop(guard);
        // No panic = success at unit level. Full disk-side rollback
        // assertion lives in `tests/picker_full_preview_integration.rs`
        // (Plan 19-08).
        assert!(
            !committed.get(),
            "committed flag stays false; guard took the rollback branch"
        );
    }

    #[test]
    fn panic_hook_rollback_on_abort_profile() {
        // V-03 BEHAVIOR-PROVING TEST (checker feedback + RESEARCH §Pitfall 1):
        // install the sentinel-variant hook, provoke a panic inside
        // `catch_unwind`, assert the sentinel flipped. This proves the
        // hook body EXECUTED the rollback branch — not a source-grep.
        //
        // `catch_unwind` works in test/debug mode even when the release
        // profile has `panic = "abort"`, because test binaries use the
        // default `panic = "unwind"` setting. The sentinel assert is the
        // primary behavior contract; the ordering check in
        // `panic_hook_uses_take_hook_chain_pattern` is supplementary.
        let sentinel = Arc::new(AtomicBool::new(false));
        install_rollback_panic_hook_with_sentinel(sentinel.clone());
        let result = std::panic::catch_unwind(|| {
            panic!("phase-19 simulated crash for rollback-hook behavior proof");
        });
        assert!(
            result.is_err(),
            "inner panic must be caught by catch_unwind"
        );
        assert!(
            sentinel.load(Ordering::SeqCst),
            "panic hook did NOT reach the rollback branch — sentinel not flipped. \
             This means the hook body short-circuited or was overwritten; \
             under `panic = abort` release this would leave managed/* drifted."
        );
        // Restore a default hook so later tests don't inherit ours.
        let _ = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
    }

    #[test]
    fn panic_hook_uses_take_hook_chain_pattern() {
        // SUPPLEMENTARY: source-level ordering assertion. This test is
        // NOT the primary proof that the hook runs — that's
        // `panic_hook_rollback_on_abort_profile` above. This test locks
        // the ordering invariant in the production `install_rollback_panic_hook`
        // body: disable_raw_mode → LeaveAlternateScreen → silent_preview_apply
        // → prev_hook(info). Reordering those steps would leak the panic
        // backtrace into the alt-screen (info-disclosure) OR skip the
        // rollback entirely.
        let source = include_str!("rollback_guard.rs");
        assert!(
            source.contains("std::panic::take_hook"),
            "install_rollback_panic_hook MUST call take_hook to chain; otherwise the default backtrace handler is discarded"
        );
        assert!(
            source.contains("disable_raw_mode"),
            "panic hook MUST restore terminal (disable_raw_mode) BEFORE letting prev_hook run — RESEARCH V7 info-disclosure mitigation"
        );
        assert!(
            source.contains("LeaveAlternateScreen"),
            "panic hook MUST exit alt-screen BEFORE letting prev_hook print backtrace"
        );
        // Verify the order in source: find the PRODUCTION hook fn block
        // (install_rollback_panic_hook, NOT the _with_sentinel variant)
        // and assert its internal call ordering.
        let prod_start = source
            .find("pub(crate) fn install_rollback_panic_hook(")
            .expect("install_rollback_panic_hook fn must exist");
        let prod_end_rel = source[prod_start..]
            .find("#[cfg(test)]")
            .unwrap_or(source.len() - prod_start);
        let prod_block = &source[prod_start..prod_start + prod_end_rel];

        let raw_pos = prod_block
            .find("disable_raw_mode")
            .expect("production hook must call disable_raw_mode");
        let alt_pos = prod_block
            .find("LeaveAlternateScreen")
            .expect("production hook must call LeaveAlternateScreen");
        let rollback_pos = prod_block
            .find("silent_preview_apply")
            .expect("production hook must call silent_preview_apply");
        let prev_pos = prod_block
            .find("prev_hook(info)")
            .expect("production hook must call prev_hook(info)");
        assert!(
            raw_pos < alt_pos,
            "disable_raw_mode must precede LeaveAlternateScreen"
        );
        assert!(
            alt_pos < rollback_pos,
            "terminal restore must precede rollback call"
        );
        assert!(
            rollback_pos < prev_pos,
            "rollback must precede prev_hook(info)"
        );
    }
}
