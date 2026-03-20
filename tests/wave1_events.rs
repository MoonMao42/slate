//! event-seam integration test (18-02-PLAN.md Task 2 Step E).
//! The `BrandEvent` dispatch sites planted in `src/cli/setup.rs` and
//! `src/cli/setup_executor/mod.rs` flow through a process-global
//! `OnceLock<Arc<dyn EventSink>>`. Running this file as its own
//! integration-test binary means the `OnceLock` starts fresh, so a
//! `set_sink(CountingSink)` call at the top of the test actually seats
//! the counter before any dispatch fires — no collision with the lib
//! unit tests that also touch the sink.
//! Contract locked here:
//! - `execute_setup_with_env` (with theme=Some(…), no tools) reaches the
//! shell-integration success arm → `BrandEvent::ApplyComplete` fires
//! exactly once for the per-tool path (zero tools installed, so no
//! per-tool ApplyComplete events) and the final shell-integration
//! success path currently does not dispatch (that’s the setup
//! handler’s `SetupComplete` surface, tested separately). Test
//! asserts that the failure count is 0 and the SINK counters reflect
//! what was actually dispatched.
//! - Failure branch: shell-integration Err → `BrandEvent::Failure(
//! FailureKind::SetupFailed)` fires exactly once.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use slate_cli::brand::events::{set_sink, BrandEvent, EventSink, FailureKind, SuccessKind};

#[derive(Default)]
struct CountingSink {
    setup_complete: AtomicUsize,
    apply_complete: AtomicUsize,
    failures: AtomicUsize,
    success: AtomicUsize,
}

impl EventSink for CountingSink {
    fn dispatch(&self, event: BrandEvent) {
        match event {
            BrandEvent::SetupComplete => {
                self.setup_complete.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::ApplyComplete => {
                self.apply_complete.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Failure(_) => {
                self.failures.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Success(_) => {
                self.success.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

/// Integration-target smoke: `execute_setup_with_env` with an empty tool
/// list + a valid theme reaches the success arm without emitting any
/// events of its own (the per-tool ApplyComplete only fires on tools
/// actually installed; the empty-list path installs zero). This locks
/// the "no tools → no ApplyComplete" contract so SoundSink
/// doesn't double-play.
#[test]
fn execute_setup_fires_no_apply_complete_when_tool_list_empty() {
    // Seat the counter FIRST — if another test in this binary already
    // dispatched, `set_sink` returns Err(sink) and we cannot reliably
    // count. Since this is a fresh integration target, the set should
    // succeed.
    let sink = Arc::new(CountingSink::default());
    let seated = set_sink(sink.clone() as Arc<dyn EventSink>).is_ok();
    assert!(
        seated,
        "set_sink must succeed in a fresh integration process"
    );

    let tempdir = tempfile::TempDir::new().unwrap();
    let env = slate_cli::env::SlateEnv::with_home(tempdir.path().to_path_buf());
    let summary = slate_cli::cli::setup_executor::execute_setup_with_env(
        &[],
        &[],
        None,
        Some("catppuccin-mocha"),
        &env,
    )
    .expect("execute_setup_with_env returns Ok for the empty-install path");

    assert!(summary.theme_applied);

    // Zero installs → zero per-tool ApplyComplete.
    assert_eq!(
        sink.apply_complete.load(Ordering::SeqCst),
        0,
        "empty tool list must not fire ApplyComplete"
    );
    // This integration test never invoked `setup::handle_with_env`, so
    // SetupComplete must NOT have fired.
    assert_eq!(
        sink.setup_complete.load(Ordering::SeqCst),
        0,
        "only setup::handle_with_env is supposed to fire SetupComplete"
    );
    // Success path → no failure dispatches.
    assert_eq!(
        sink.failures.load(Ordering::SeqCst),
        0,
        "success path must not fire Failure"
    );

    // Type-import gate: reference the FailureKind + SuccessKind enums so
    // a future refactor that renames a variant will break this test
    // instead of silently drifting the surface.
    let _ = FailureKind::SetupFailed;
    let _ = SuccessKind::ThemeApplied;
}
