//! event-seam integration test (18-05-PLAN.md Task 1 Test 2 + Test 4).
//! The `BrandEvent` dispatch sites planted in `src/cli/clean.rs` and
//! `src/cli/restore.rs` flow through a process-global
//! `OnceLock<Arc<dyn EventSink>>`. Running this file as its own
//! integration-test binary means the `OnceLock` starts fresh, so a
//! `set_sink(CountingSink)` call at the top of the first test that runs
//! actually seats the counter before any dispatch fires — no collision
//! with the lib unit tests that also touch the sink.
//! Contracts locked here:
//! 1. `slate clean` (via `handle_clean`) on a pristine temp HOME reaches
//! the happy path and dispatches **exactly one** `CleanComplete` +
//! **exactly one** `ApplyComplete`. Zero `CleanFailed` events fire.
//! 2. `BrandEvent::Failure(FailureKind::CleanFailed)` is routable
//! dispatching it directly from the test lands in the sink's failure
//! counter, proving the variant exists and the wrapper in
//! `handle_clean` (which catches `Err` and re-dispatches) will land
//! its failure event on SoundSink.
//! 3. `BrandEvent::Success(SuccessKind::RestoreComplete)` is routable
//! same shape as (2), covering the event planted at the end of
//! `handle_restore_direct`. A full restore flow requires interactive
//! `confirm()` + a live `RestorePoint`, so direct invocation is out
//! of scope for this integration target; the variant-existence test
//! plus the unit-level `restore_summary_*` snapshots together lock
//! the surface.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};

use slate_cli::brand::events::{
    dispatch, set_sink, BrandEvent, EventSink, FailureKind, SuccessKind,
};

#[derive(Default)]
struct CountingSink {
    clean_complete: AtomicUsize,
    restore_complete: AtomicUsize,
    apply_complete: AtomicUsize,
    clean_failed: AtomicUsize,
    other_success: AtomicUsize,
    other_failure: AtomicUsize,
}

impl EventSink for CountingSink {
    fn dispatch(&self, event: BrandEvent) {
        match event {
            BrandEvent::Success(SuccessKind::CleanComplete) => {
                self.clean_complete.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Success(SuccessKind::RestoreComplete) => {
                self.restore_complete.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::ApplyComplete => {
                self.apply_complete.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Failure(FailureKind::CleanFailed) => {
                self.clean_failed.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Success(_) => {
                self.other_success.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Failure(_) => {
                self.other_failure.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

/// Seats the shared CountingSink exactly once for this integration
/// binary. Subsequent tests in this file re-use the same counters.
/// The `Mutex` guards the execution-order contract inside a single
/// integration binary: tests may run in parallel, but `slate clean`
/// mutates a tempdir + dispatches events, so serialising keeps the
/// per-test sink reads deterministic. Counters are `AtomicUsize` so
/// reads are cheap; the lock just forces sequential test execution.
static SINK: OnceLock<Arc<CountingSink>> = OnceLock::new();
static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn shared_sink() -> (&'static CountingSink, std::sync::MutexGuard<'static, ()>) {
    let sink: &'static Arc<CountingSink> = SINK.get_or_init(|| {
        let sink = Arc::new(CountingSink::default());
        // Best-effort seat. If another test in this binary already
        // seated a different sink we would degrade to that sink
        // but within this file we only ever seat this one, so the
        // set must succeed on first call.
        let _ = set_sink(sink.clone() as Arc<dyn EventSink>);
        sink
    });

    // Both statics are `'static`, so `OnceLock::get_or_init` hands back
    // a `&'static Mutex<>` whose `.lock()` yields a `MutexGuard<'static, >`
    // directly — no `unsafe` transmute needed.
    let lock: &'static Mutex<()> = LOCK.get_or_init(|| Mutex::new(()));
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    (sink.as_ref(), guard)
}

/// Contract 1 — a successful `slate clean` dispatches exactly one
/// `CleanComplete` + exactly one `ApplyComplete`, and zero
/// `CleanFailed`.
#[test]
fn clean_success_dispatches_clean_complete_and_apply_complete() {
    let (sink, _guard) = shared_sink();
    let before_clean_complete = sink.clean_complete.load(Ordering::SeqCst);
    let before_apply_complete = sink.apply_complete.load(Ordering::SeqCst);
    let before_clean_failed = sink.clean_failed.load(Ordering::SeqCst);

    // Run `handle_clean` in a pristine tempdir so no real config is
    // touched. `SLATE_HOME` points the env at the tempdir; the
    // clean handler walks that tree, finds nothing managed, and
    // exits via the happy path — which is exactly the contract we
    // want to lock here.
    let tempdir = tempfile::TempDir::new().unwrap();
    std::env::set_var("SLATE_HOME", tempdir.path());

    let result = slate_cli::cli::clean::handle_clean();
    assert!(
        result.is_ok(),
        "handle_clean on a pristine tempdir must succeed, got: {:?}",
        result
    );

    std::env::remove_var("SLATE_HOME");

    let clean_complete_delta = sink.clean_complete.load(Ordering::SeqCst) - before_clean_complete;
    let apply_complete_delta = sink.apply_complete.load(Ordering::SeqCst) - before_apply_complete;
    let clean_failed_delta = sink.clean_failed.load(Ordering::SeqCst) - before_clean_failed;

    assert_eq!(
        clean_complete_delta, 1,
        "exactly one CleanComplete must fire on success"
    );
    assert_eq!(
        apply_complete_delta, 1,
        "exactly one ApplyComplete must pair with CleanComplete on success"
    );
    assert_eq!(
        clean_failed_delta, 0,
        "success path must NOT dispatch CleanFailed"
    );
}

/// Contract 2 — `BrandEvent::Failure(CleanFailed)` is routable. The
/// wrapper in `handle_clean` catches an inner `Err` and re-dispatches
/// this exact variant; by dispatching it directly we prove the event
/// lands in the sink's failure counter. SoundSink will
/// receive this same event shape when `handle_clean` fails in
/// production.
#[test]
fn clean_failed_event_is_routable() {
    let (sink, _guard) = shared_sink();
    let before = sink.clean_failed.load(Ordering::SeqCst);

    dispatch(BrandEvent::Failure(FailureKind::CleanFailed));

    let delta = sink.clean_failed.load(Ordering::SeqCst) - before;
    assert_eq!(
        delta, 1,
        "dispatched CleanFailed must land in the failure counter exactly once"
    );
}

/// Contract 3 — `BrandEvent::Success(RestoreComplete)` is routable.
/// The full `handle_restore_direct` flow requires interactive
/// `confirm()` + a live `RestorePoint`, so we cover it with a
/// variant-routing test plus the unit-level `restore_summary_*`
/// snapshots in `src/cli/restore.rs`.
#[test]
fn restore_complete_event_is_routable() {
    let (sink, _guard) = shared_sink();
    let before = sink.restore_complete.load(Ordering::SeqCst);

    dispatch(BrandEvent::Success(SuccessKind::RestoreComplete));

    let delta = sink.restore_complete.load(Ordering::SeqCst) - before;
    assert_eq!(
        delta, 1,
        "dispatched RestoreComplete must land in the restore counter exactly once"
    );
}

/// Type-import gate — reference every variant this wave depends on
/// so a future refactor that renames one breaks this test instead of
/// silently drifting the event surface.
#[test]
fn wave4_variants_exist() {
    let _ = SuccessKind::CleanComplete;
    let _ = SuccessKind::RestoreComplete;
    let _ = FailureKind::CleanFailed;
}
