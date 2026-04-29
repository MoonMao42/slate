//! event-seam integration test (18-07-PLAN.md Task 2 Test F).
//! The Wave-6 dispatch sites (`AutoThemeFailed` across `src/cli/auto_theme.rs`;
//! `ConfigSet` across `src/cli/config.rs` + `src/cli/share.rs` + the
//! auto-theme configure flow) flow through a process-global
//! `OnceLock<Arc<dyn EventSink>>`. Running this file as its own
//! integration-test binary means the `OnceLock` starts fresh, so the
//! `set_sink(CountingSink)` call at the top of the first test actually
//! seats the counter before any dispatch fires — no collision with the
//! lib unit tests that also touch the sink (mirror of `tests/wave4_events.rs`
//! execution-serialiser pattern).
//! Contracts locked here:
//! 1. `BrandEvent::Failure(FailureKind::AutoThemeFailed)` is routable
//! dispatching it directly from the test lands in the sink's failure
//! counter, proving the variant exists and the wrapper in
//! `resolve_auto_theme` / `configure_auto_theme` (which catch `Err`
//! and re-dispatch) will land their failure event on
//! SoundSink.
//! 2. `BrandEvent::Success(SuccessKind::ConfigSet)` is routable — same
//! shape as (1), covering the 9 dispatch sites planted across
//! `slate config set` (8 sub-commands) + `slate share import` + the
//! auto-theme configure success path.
//! 3. `slate config set sound on` (the safest-to-drive sub-command in a
//! tempdir context — no watcher spawn, no shell-integration refresh
//! side-effects) drives the real handler end-to-end and asserts
//! exactly one ConfigSet fires and zero AutoThemeFailed leaks.
//! 4. Compile-time variant gate enumerates every Wave-6 event variant
//! so future renames break this test instead of silently drifting.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};

use slate_cli::brand::events::{
    dispatch, set_sink, BrandEvent, EventSink, FailureKind, SuccessKind,
};

#[derive(Default)]
struct CountingSink {
    config_set: AtomicUsize,
    auto_theme_failed: AtomicUsize,
    other_success: AtomicUsize,
    other_failure: AtomicUsize,
}

impl EventSink for CountingSink {
    fn dispatch(&self, event: BrandEvent) {
        match event {
            BrandEvent::Success(SuccessKind::ConfigSet) => {
                self.config_set.fetch_add(1, Ordering::SeqCst);
            }
            BrandEvent::Failure(FailureKind::AutoThemeFailed) => {
                self.auto_theme_failed.fetch_add(1, Ordering::SeqCst);
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

static SINK: OnceLock<Arc<CountingSink>> = OnceLock::new();
static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn shared_sink() -> (&'static CountingSink, std::sync::MutexGuard<'static, ()>) {
    let sink: &'static Arc<CountingSink> = SINK.get_or_init(|| {
        let sink = Arc::new(CountingSink::default());
        let _ = set_sink(sink.clone() as Arc<dyn EventSink>);
        sink
    });
    let lock: &'static Mutex<()> = LOCK.get_or_init(|| Mutex::new(()));
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    (sink.as_ref(), guard)
}

/// Contract 1 — `BrandEvent::Failure(AutoThemeFailed)` is routable.
#[test]
fn auto_theme_failed_event_is_routable() {
    let (sink, _guard) = shared_sink();
    let before = sink.auto_theme_failed.load(Ordering::SeqCst);
    dispatch(BrandEvent::Failure(FailureKind::AutoThemeFailed));
    let delta = sink.auto_theme_failed.load(Ordering::SeqCst) - before;
    assert_eq!(
        delta, 1,
        "dispatched AutoThemeFailed must land in the failure counter exactly once"
    );
}

/// Contract 2 — `BrandEvent::Success(ConfigSet)` is routable.
#[test]
fn config_set_event_is_routable() {
    let (sink, _guard) = shared_sink();
    let before = sink.config_set.load(Ordering::SeqCst);
    dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
    let delta = sink.config_set.load(Ordering::SeqCst) - before;
    assert_eq!(
        delta, 1,
        "dispatched ConfigSet must land in the success counter exactly once"
    );
}

/// Contract 3 — driving the real `slate config set sound on` handler
/// fires exactly one ConfigSet and zero AutoThemeFailed events. `sound`
/// is chosen because it has no shell-integration / watcher side
/// effects (only a managed-state file write), so a tempdir HOME can
/// run end-to-end without leaking processes.
#[test]
fn config_set_sound_handler_dispatches_one_config_set() {
    let (sink, _guard) = shared_sink();
    let before_config_set = sink.config_set.load(Ordering::SeqCst);
    let before_auto_failed = sink.auto_theme_failed.load(Ordering::SeqCst);

    let tempdir = tempfile::TempDir::new().unwrap();
    std::env::set_var("SLATE_HOME", tempdir.path());

    let result = slate_cli::cli::config::handle_config_set("sound", "on");
    assert!(
        result.is_ok(),
        "handle_config_set sound on must succeed in a tempdir HOME, got: {:?}",
        result
    );

    std::env::remove_var("SLATE_HOME");

    let config_set_delta = sink.config_set.load(Ordering::SeqCst) - before_config_set;
    let auto_failed_delta = sink.auto_theme_failed.load(Ordering::SeqCst) - before_auto_failed;

    assert_eq!(
        config_set_delta, 1,
        "exactly one ConfigSet must fire on `slate config set sound on` success"
    );
    assert_eq!(
        auto_failed_delta, 0,
        "success path must NOT dispatch AutoThemeFailed"
    );
}

/// Contract 4 — type-import gate. Reference every Wave-6 variant so a
/// future refactor that renames one breaks this test instead of
/// silently drifting the event surface.
#[test]
fn wave6_variants_exist() {
    let _ = SuccessKind::ConfigSet;
    let _ = FailureKind::AutoThemeFailed;
}
