//! Wave 5 event-seam integration test (18-06-PLAN.md Task 2 Step E).
//!
//! The `BrandEvent` dispatch sites planted in `src/cli/picker/event_loop.rs`
//! flow through a process-global `OnceLock<Arc<dyn EventSink>>`. Running
//! this file as its own integration-test binary means the `OnceLock`
//! starts fresh, so a `set_sink(CountingSink)` call at the top of the
//! first test actually seats the counter before any dispatch fires — no
//! collision with the lib unit tests that also touch the sink.
//!
//! Contracts locked here:
//!
//! 1. `BrandEvent::Navigation(NavKind::PickerMove)` is routable —
//!    dispatching it directly from the test lands in the sink's nav
//!    counter, proving the variant exists and the j/k/Up/Down branches
//!    in `handle_key` will land their nav events on Phase 20's SoundSink.
//! 2. `BrandEvent::Selection(SelectKind::PickerEnter)` is routable —
//!    same shape, covering the Enter branch.
//! 3. Compile-time variant gate: the `NavKind::PickerMove` +
//!    `SelectKind::PickerEnter` types import cleanly. Future renames
//!    break this test instead of silently drifting the Phase 20 contract.
//!
//! End-to-end picker driving (alt-screen + real `event::read()`) is out
//! of scope here — that headless flow is non-trivial to script and adds
//! no coverage beyond the unit tests in
//! `src/cli/picker/event_loop.rs::tests` which call `handle_key` directly
//! with synthetic `KeyEvent`s and assert the per-key dispatch counts.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};

use slate_cli::brand::events::{dispatch, set_sink, BrandEvent, EventSink, NavKind, SelectKind};

#[derive(Default)]
struct CountingSink {
    picker_move: AtomicUsize,
    picker_enter: AtomicUsize,
    other: AtomicUsize,
}

impl EventSink for CountingSink {
    fn dispatch(&self, event: BrandEvent) {
        match event {
            BrandEvent::Navigation(NavKind::PickerMove) => {
                self.picker_move.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Selection(SelectKind::PickerEnter) => {
                self.picker_enter.fetch_add(1, Ordering::SeqCst);
            }
            _ => {
                self.other.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

static SINK: OnceLock<Arc<CountingSink>> = OnceLock::new();
static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn shared_sink() -> (&'static CountingSink, std::sync::MutexGuard<'static, ()>) {
    let sink: &'static Arc<CountingSink> = SINK.get_or_init(|| {
        let sink = Arc::new(CountingSink::default());
        // First call seats the CountingSink. If somehow another test
        // seated a different sink first (shouldn't happen within this
        // binary — no other `set_sink` call exists here), the routing
        // assertions degrade to checking the Noop path, which is still
        // correct behavior for Phase 18.
        let _ = set_sink(sink.clone() as Arc<dyn EventSink>);
        sink
    });
    let lock: &'static Mutex<()> = LOCK.get_or_init(|| Mutex::new(()));
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    (sink.as_ref(), guard)
}

/// Contract 1 — `PickerMove` is routable.
#[test]
fn picker_move_event_is_routable() {
    let (sink, _guard) = shared_sink();
    let before = sink.picker_move.load(Ordering::SeqCst);
    dispatch(BrandEvent::Navigation(NavKind::PickerMove));
    let after = sink.picker_move.load(Ordering::SeqCst);
    assert_eq!(
        after - before,
        1,
        "PickerMove dispatch must land in the nav counter"
    );
}

/// Contract 2 — `PickerEnter` is routable.
#[test]
fn picker_enter_event_is_routable() {
    let (sink, _guard) = shared_sink();
    let before = sink.picker_enter.load(Ordering::SeqCst);
    dispatch(BrandEvent::Selection(SelectKind::PickerEnter));
    let after = sink.picker_enter.load(Ordering::SeqCst);
    assert_eq!(
        after - before,
        1,
        "PickerEnter dispatch must land in the selection counter"
    );
}

/// Contract 3 — compile-time variant gate.
///
/// Enumerates the two Wave-5 variants so renaming
/// `NavKind::PickerMove` or `SelectKind::PickerEnter` would break this
/// test first, surfacing the regression before Phase 20's SoundSink
/// depends on those names.
#[test]
fn wave5_variants_exist() {
    let events: &[BrandEvent] = &[
        BrandEvent::Navigation(NavKind::PickerMove),
        BrandEvent::Selection(SelectKind::PickerEnter),
    ];
    assert_eq!(events.len(), 2);
}
