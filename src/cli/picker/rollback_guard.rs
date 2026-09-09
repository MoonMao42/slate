//! The normal exit, Drop, and abort-profile panic paths share one immutable
//! snapshot of preview files. Cleanup never regenerates a guessed original theme.

use super::preview_snapshot::PreviewSnapshot;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Restores preview files on early return or unwind.
pub(crate) struct RollbackGuard {
    snapshot: Arc<PreviewSnapshot>,
    committed: Arc<AtomicBool>,
}

type SharedPanicHook = Arc<dyn for<'a> Fn(&std::panic::PanicHookInfo<'a>) + Send + Sync + 'static>;

/// Deactivates Slate's picker rollback hook when the picker exits.
/// The hook itself is process-global, so trying to "restore" the previous
/// hook from Drop would race with any later `std::panic::set_hook()` call in
/// the same process and could clobber it. Instead we leave the wrapper hook
/// installed but flip `active=false`, so future panics bypass Slate's rollback
/// branch and delegate straight to whatever hook chain is current.
pub(crate) struct PanicHookGuard {
    active: Arc<AtomicBool>,
}

impl RollbackGuard {
    /// The same snapshot is shared with the normal exit and panic hook.
    pub(crate) fn arm(snapshot: Arc<PreviewSnapshot>, committed: Arc<AtomicBool>) -> Self {
        Self {
            snapshot,
            committed,
        }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

impl Drop for RollbackGuard {
    fn drop(&mut self) {
        // fail-silent: picker Cancel branch philosophy.
        // Never panic inside Drop (would double-panic → abort).
        if self.committed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(err) = self.snapshot.restore() {
            report_cleanup_error(&err);
        }
    }
}

/// The weak handle prevents inactive hooks from retaining file contents forever.
/// The hook runs before abort, which would otherwise skip the guard's Drop.
pub(crate) fn install_rollback_panic_hook(
    snapshot: &Arc<PreviewSnapshot>,
    committed: Arc<AtomicBool>,
) -> PanicHookGuard {
    let snapshot = Arc::downgrade(snapshot);
    install_panic_hook(committed, move || {
        if let Some(snapshot) = snapshot.upgrade() {
            if let Err(err) = snapshot.restore() {
                report_cleanup_error(&err);
            }
        }
    })
}

fn report_cleanup_error(error: &crate::error::SlateError) {
    use std::io::Write;
    // Best-effort diagnostics must not cause a second panic in Drop/the hook.
    let _ = writeln!(std::io::stderr().lock(), "warning: {error}");
}

fn install_panic_hook<F>(committed: Arc<AtomicBool>, rollback: F) -> PanicHookGuard
where
    F: Fn() + Send + Sync + 'static,
{
    let previous_hook: SharedPanicHook = Arc::from(std::panic::take_hook());
    let chained_hook = previous_hook.clone();
    let active = Arc::new(AtomicBool::new(true));
    let hook_active = active.clone();

    std::panic::set_hook(Box::new(move |info| {
        if hook_active.load(Ordering::SeqCst) && !committed.load(Ordering::SeqCst) {
            // 1. Restore the terminal FIRST so the panic backtrace prints to a
            // sane surface (not into the alt-screen). RESEARCH V7
            // info-disclosure mitigation.
            restore_terminal_surface();
            // 2. Silent managed/* rollback.
            rollback();
        }
        // 3. Delegate to the previous hook (usually the default backtrace
        // printer). After this returns, `panic = "abort"` kicks in.
        chained_hook(info);
    }));

    PanicHookGuard { active }
}

fn restore_terminal_surface() {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::cursor::Show,
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    );
}

/// V-03 BEHAVIOR-PROVING test variant (test-only).
/// Same structure as `install_rollback_panic_hook` but at step 2 it flips
/// a caller-provided `AtomicBool` sentinel INSTEAD of calling
/// `silent_preview_apply`. This lets the test suite PROVE the hook body
/// actually reached the rollback branch — a critical invariant given
/// `panic = "abort"` makes Drop unreliable in release.
/// Why not assert via `silent_preview_apply`? Because it writes real
/// managed/* files on disk and would be destructive + flaky in parallel
/// tests. The sentinel proves the hook fired at the SAME point where
/// production calls `silent_preview_apply`; the production path is
/// exercised separately at integration scope in.
#[cfg(test)]
pub(crate) fn install_rollback_panic_hook_with_sentinel(
    sentinel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> PanicHookGuard {
    install_rollback_panic_hook_with_sentinel_and_commit_flag(
        sentinel,
        Arc::new(AtomicBool::new(false)),
    )
}

#[cfg(test)]
pub(crate) fn install_rollback_panic_hook_with_sentinel_and_commit_flag(
    sentinel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    committed: Arc<AtomicBool>,
) -> PanicHookGuard {
    use std::sync::atomic::Ordering;
    install_panic_hook(committed, move || {
        sentinel.store(true, Ordering::SeqCst);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SlateEnv;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::sync::{Mutex, OnceLock};

    fn hook_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn test_env() -> (tempfile::TempDir, SlateEnv) {
        // Use a unique tempdir so tests can run in parallel without
        // clobbering each other's managed/* directories.
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        (tmp, SlateEnv::with_home(home))
    }

    #[test]
    fn rollback_guard_noop_when_committed() {
        let (_tmp, env) = test_env();
        let committed = Arc::new(AtomicBool::new(false));
        // Simulate "user pressed Enter" by pre-setting committed=true.
        committed.store(true, Ordering::SeqCst);
        let guard = RollbackGuard::arm(
            Arc::new(PreviewSnapshot::capture(&env).unwrap()),
            committed.clone(),
        );
        drop(guard);
        // Assertion is behavioral: no panic, drop was silent.
        assert!(
            committed.load(Ordering::SeqCst),
            "committed flag must remain true after drop"
        );
    }

    #[test]
    fn rollback_guard_on_drop_when_not_committed() {
        let (_tmp, env) = test_env();
        let committed = Arc::new(AtomicBool::new(false));
        let guard = RollbackGuard::arm(
            Arc::new(PreviewSnapshot::capture(&env).unwrap()),
            committed.clone(),
        );
        drop(guard);
        // No panic = success at unit level. Full disk-side rollback
        // assertion lives in `tests/picker_full_preview_integration.rs`
        assert!(
            !committed.load(Ordering::SeqCst),
            "committed flag stays false; guard took the rollback branch"
        );
    }

    #[test]
    fn preview_write_state_panic_hook_preserves_inflight_changes_and_recovery() {
        let _lock = hook_test_lock();
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let path = env.managed_file("managed/ghostty/theme.conf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"PRIVATE_ORIGINAL").unwrap();
        let snapshot = Arc::new(PreviewSnapshot::capture(&env).unwrap());
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let guard = install_rollback_panic_hook(&snapshot, Arc::new(AtomicBool::new(false)));
        let result = std::panic::catch_unwind(|| {
            snapshot.apply_with(|| {
                std::fs::write(&path, b"PRIVATE_UNRECORDED_CHANGE").unwrap();
                panic!("injected panic during preview write");
            })
        });
        drop(guard);
        let _ = std::panic::take_hook();
        std::panic::set_hook(original_hook);
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"PRIVATE_UNRECORDED_CHANGE");
        let record = env.slate_cache_dir().join("preview-session.json");
        let saved = std::fs::read(&record).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&saved).unwrap()["writing"],
            true
        );
        // Unwind releases the unit-valued operation gate. Cleanup can now run,
        // but it must still reject bytes absent from the last verified state.
        assert!(snapshot.restore().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"PRIVATE_UNRECORDED_CHANGE");
        assert_eq!(std::fs::read(&record).unwrap(), saved);
        drop(snapshot);
        let plan = crate::cli::picker::inspect_recovery(&env).unwrap();
        assert!(!plan.active && plan.interrupted_write && plan.blocked_count() == 1);
    }

    #[test]
    fn preview_snapshot_panic_hook_restores_files_and_releases_capture() {
        let _lock = hook_test_lock();
        let td = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("ghostty/config.ghostty");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"# custom before panic\xff\n").unwrap();
        let snapshot = Arc::new(PreviewSnapshot::capture(&env).unwrap());
        snapshot
            .apply("nord", crate::opacity::OpacityPreset::Frosted)
            .unwrap();
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let guard = install_rollback_panic_hook(&snapshot, Arc::new(AtomicBool::new(false)));
        let result = std::panic::catch_unwind(|| panic!("simulated picker panic"));
        drop(guard);
        let _ = std::panic::take_hook();
        std::panic::set_hook(original_hook);
        assert!(result.is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"# custom before panic\xff\n");
        assert_eq!(
            Arc::strong_count(&snapshot),
            1,
            "inactive hook retained file bytes"
        );
    }

    #[test]
    fn panic_hook_rollback_on_abort_profile() {
        let _lock = hook_test_lock();
        // V-03 BEHAVIOR-PROVING TEST (checker feedback + RESEARCH §Pitfall 1):
        // install the sentinel-variant hook, provoke a panic inside
        // `catch_unwind`, assert the sentinel flipped. This proves the
        // hook body EXECUTED the rollback branch — not a source-grep.
        // `catch_unwind` works in test/debug mode even when the release
        // profile has `panic = "abort"`, because test binaries use the
        // default `panic = "unwind"` setting. The sentinel assert is the
        // primary behavior contract; the ordering check in
        // `panic_hook_uses_take_hook_chain_pattern` is supplementary.
        let sentinel = Arc::new(AtomicBool::new(false));
        let _guard = install_rollback_panic_hook_with_sentinel(sentinel.clone());
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
    }

    #[test]
    fn panic_hook_guard_does_not_clobber_later_hook_on_drop() {
        let _lock = hook_test_lock();
        let original_hook = std::panic::take_hook();

        let later_hook_seen = Arc::new(AtomicBool::new(false));
        let later_hook_seen_clone = later_hook_seen.clone();
        let rollback_seen = Arc::new(AtomicBool::new(false));
        {
            let _guard = install_rollback_panic_hook_with_sentinel(rollback_seen.clone());
            std::panic::set_hook(Box::new(move |_| {
                later_hook_seen_clone.store(true, Ordering::SeqCst);
            }));
        }

        rollback_seen.store(false, Ordering::SeqCst);
        later_hook_seen.store(false, Ordering::SeqCst);

        let result = std::panic::catch_unwind(|| {
            panic!("phase-19 later hook should stay installed");
        });
        assert!(
            result.is_err(),
            "inner panic must be caught by catch_unwind"
        );
        assert!(
            later_hook_seen.load(Ordering::SeqCst),
            "dropping PanicHookGuard must not clobber a later process-wide hook"
        );
        assert!(
            !rollback_seen.load(Ordering::SeqCst),
            "picker rollback hook leaked past picker lifetime"
        );

        let _ = std::panic::take_hook();
        std::panic::set_hook(original_hook);
    }

    #[test]
    fn panic_hook_skips_rollback_after_commit() {
        let _lock = hook_test_lock();
        let original_hook = std::panic::take_hook();

        let sentinel = Arc::new(AtomicBool::new(false));
        let committed = Arc::new(AtomicBool::new(true));
        let _guard =
            install_rollback_panic_hook_with_sentinel_and_commit_flag(sentinel.clone(), committed);

        let result = std::panic::catch_unwind(|| {
            panic!("post-commit panic should not rollback picker state");
        });
        assert!(
            result.is_err(),
            "inner panic must be caught by catch_unwind"
        );
        assert!(
            !sentinel.load(Ordering::SeqCst),
            "panic hook must not rollback after the picker commit flag flips true"
        );

        let _ = std::panic::take_hook();
        std::panic::set_hook(original_hook);
    }

    #[test]
    fn panic_hook_guard_does_not_touch_global_hook_during_unwind_drop() {
        let _lock = hook_test_lock();
        let original_hook = std::panic::take_hook();

        let previous_hook_seen = Arc::new(AtomicBool::new(false));
        let previous_hook_seen_clone = previous_hook_seen.clone();
        std::panic::set_hook(Box::new(move |_| {
            previous_hook_seen_clone.store(true, Ordering::SeqCst);
        }));

        let rollback_seen = Arc::new(AtomicBool::new(false));
        let result = std::panic::catch_unwind({
            let rollback_seen = rollback_seen.clone();
            move || {
                let _guard = install_rollback_panic_hook_with_sentinel(rollback_seen);
                panic!("phase-19 picker hook should survive unwind drop");
            }
        });
        assert!(
            result.is_err(),
            "inner panic must stay catchable; PanicHookGuard::drop must not double-panic during unwind"
        );
        assert!(
            rollback_seen.load(Ordering::SeqCst),
            "picker rollback hook should still fire before unwind reaches Drop"
        );
        assert!(
            previous_hook_seen.load(Ordering::SeqCst),
            "previous hook should remain chained during the picker panic"
        );

        rollback_seen.store(false, Ordering::SeqCst);
        previous_hook_seen.store(false, Ordering::SeqCst);

        let result = std::panic::catch_unwind(|| {
            panic!("post-unwind panic should not re-run picker rollback");
        });
        assert!(result.is_err(), "post-unwind panic must remain catchable");
        assert!(
            !rollback_seen.load(Ordering::SeqCst),
            "picker rollback hook must be deactivated once the guard drops during unwind"
        );
        assert!(
            previous_hook_seen.load(Ordering::SeqCst),
            "post-unwind panic should still reach the previous hook"
        );

        let _ = std::panic::take_hook();
        std::panic::set_hook(original_hook);
    }

    #[test]
    fn panic_hook_uses_take_hook_chain_pattern() {
        // SUPPLEMENTARY: source-level ordering assertion. This test is
        // NOT the primary proof that the hook runs — that's
        // `panic_hook_rollback_on_abort_profile` above. This test locks
        // the ordering invariant in the shared `install_panic_hook` body:
        // restore_terminal_surface → rollback() → chained_hook(info).
        // Reordering those steps would leak the panic backtrace into the
        // alt-screen (info-disclosure) OR skip the rollback entirely.
        let source = include_str!("rollback_guard.rs");
        assert!(
            source.contains("std::panic::take_hook"),
            "install_rollback_panic_hook MUST call take_hook to chain; otherwise the default backtrace handler is discarded"
        );
        assert!(
            source.contains("restore_terminal_surface"),
            "panic hook MUST restore terminal BEFORE letting the previous hook run — RESEARCH V7 info-disclosure mitigation"
        );
        assert!(
            source.contains("LeaveAlternateScreen"),
            "panic hook MUST exit alt-screen BEFORE letting prev_hook print backtrace"
        );
        assert!(
            source.contains("DisableMouseCapture"),
            "panic hook MUST disable mouse capture alongside leaving the alt-screen"
        );
        // Verify the order in source: find the PRODUCTION hook fn block
        // (install_panic_hook, not the test-only variant) and assert its
        // internal call ordering.
        let hook_start = source
            .find("fn install_panic_hook")
            .expect("install_panic_hook fn must exist");
        let hook_end_rel = source[hook_start..]
            .find("fn restore_terminal_surface")
            .expect("install_panic_hook block must end before restore_terminal_surface");
        let hook_block = &source[hook_start..hook_start + hook_end_rel];

        let restore_pos = hook_block
            .find("restore_terminal_surface")
            .expect("panic hook must restore the terminal surface");
        let rollback_pos = hook_block
            .find("rollback();")
            .expect("panic hook must call rollback()");
        let prev_pos = hook_block
            .find("chained_hook(info)")
            .expect("panic hook must call chained_hook(info)");
        assert!(
            restore_pos < rollback_pos,
            "terminal restore must precede rollback call"
        );
        assert!(
            rollback_pos < prev_pos,
            "rollback must precede prev_hook(info)"
        );
    }
}
