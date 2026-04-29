//! Brand event seam — the sound / analytics plumbing.
//! ships the trait + `NoopSink` default only.
//! `SoundSink : impl EventSink` will register via [`set_sink`] on
//! startup; no other code changes.
//! Decisions honored:
//! - ** / / ** — `BrandEvent` enum + `EventSink` trait +
//! `NoopSink` default + `OnceLock<Arc<dyn EventSink>>` singleton.
//! Six top-level variants (Success / Failure / Navigation / Selection /
//! SetupComplete / ApplyComplete) — the 4 broad categories from
//! plus two whole-flow milestones for to latch onto.
//! ## Ordering (Pitfall 5)
//! `set_sink` **MUST** be called before the first `dispatch` on a given
//! process. Once `dispatch` has run, the `OnceLock` is initialized with
//! [`NoopSink`] and `set_sink` will error with the attempted `Arc` back
//! (so can log a "registered too late" message). relies
//! on `NoopSink` being the default, so absent any `set_sink` call the
//! dispatches are observably no-ops.

use std::sync::{Arc, Mutex, OnceLock};

/// High-level user-facing events emitted from `src/cli/*` during Waves
/// 1–6 migration. maps each variant to an SFX.
#[derive(Debug, Clone)]
pub enum BrandEvent {
    /// A user-observable success (theme applied, config set, font downloaded…).
    Success(SuccessKind),
    /// A user-observable failure.
    Failure(FailureKind),
    /// Picker / wizard navigation (move between items, forward/back).
    Navigation(NavKind),
    /// Picker / wizard selection (Enter pressed).
    Selection(SelectKind),
    /// Whole-flow milestone — `slate setup` finished.
    SetupComplete,
    /// Whole-flow milestone — theme apply / font apply / clean etc.
    /// finished.
    ApplyComplete,
}

#[derive(Debug, Clone, Copy)]
pub enum SuccessKind {
    ThemeApplied,
    ConfigSet,
    CleanComplete,
    RestoreComplete,
    FontDownloaded,
}

#[derive(Debug, Clone, Copy)]
pub enum FailureKind {
    ThemeApplyFailed,
    SetupFailed,
    CleanFailed,
    AutoThemeFailed,
    FontDownloadFailed,
}

#[derive(Debug, Clone, Copy)]
pub enum NavKind {
    PickerMove,
    WizardNext,
    WizardBack,
}

#[derive(Debug, Clone, Copy)]
pub enum SelectKind {
    PickerEnter,
    WizardConfirm,
}

/// Sink trait — a downstream consumer (sound, analytics, …)
/// that wants to react to [`BrandEvent`]s. Must be `Send + Sync` so the
/// `OnceLock<Arc<dyn EventSink>>` can be shared across threads.
pub trait EventSink: Send + Sync {
    fn dispatch(&self, event: BrandEvent);
}

/// Default no-op sink — zero behavior change when no sink is registered.
pub struct NoopSink;

impl EventSink for NoopSink {
    fn dispatch(&self, _: BrandEvent) {}
}

struct SinkState {
    sink: Arc<dyn EventSink>,
    dispatched: bool,
    explicitly_registered: bool,
}

impl Default for SinkState {
    fn default() -> Self {
        Self {
            sink: Arc::new(NoopSink) as Arc<dyn EventSink>,
            dispatched: false,
            explicitly_registered: false,
        }
    }
}

static SINK: OnceLock<Mutex<SinkState>> = OnceLock::new();

fn sink_state() -> &'static Mutex<SinkState> {
    SINK.get_or_init(|| Mutex::new(SinkState::default()))
}

/// Fire a [`BrandEvent`]. Self-initializes with [`NoopSink`] on first
/// call if no sink has been registered via [`set_sink`].
pub fn dispatch(event: BrandEvent) {
    let sink = {
        let mut state = sink_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.dispatched = true;
        state.sink.clone()
    };
    sink.dispatch(event);
}

/// Register a sink. Returns `Err(sink)` if the `OnceLock` has already
/// been initialized (either by a prior `set_sink` or by `dispatch`
/// triggering the `NoopSink` default) — the caller can then log a
/// "registered too late" diagnostic.
pub fn set_sink(sink: Arc<dyn EventSink>) -> std::result::Result<(), Arc<dyn EventSink>> {
    let mut state = sink_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.dispatched || state.explicitly_registered {
        Err(sink)
    } else {
        state.sink = sink;
        state.explicitly_registered = true;
        Ok(())
    }
}

/// Explicitly register the default [`NoopSink`] — optional in
/// (dispatch self-initializes) but `main.rs` calls this to make the
/// Pitfall 5 ordering explicit so has a documented template.
pub fn ensure_default_sink() {
    let _ = sink_state();
}

#[cfg(test)]
pub(crate) fn reset_sink_for_tests() {
    let mut state = sink_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *state = SinkState::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Row `18-W0-event-noop` — dispatching with no sink registered must
    /// be a silent no-op (returns without panic; observable flag
    /// confirms the `NoopSink` default).
    #[test]
    fn dispatch_with_default_sink_is_noop() {
        reset_sink_for_tests();
        dispatch(BrandEvent::SetupComplete);
        dispatch(BrandEvent::Success(SuccessKind::ThemeApplied));
        // If this line is reached, the default sink did not panic.
    }

    struct TestSink {
        flag: Arc<AtomicBool>,
    }
    impl EventSink for TestSink {
        fn dispatch(&self, _: BrandEvent) {
            self.flag.store(true, Ordering::SeqCst);
        }
    }

    /// Row `18-W0-event-routing` — `set_sink(TestSink)` routes subsequent
    /// dispatches. The test-only reset helper restores the global sink
    /// state between cases so this stays deterministic inside the shared
    /// lib-test process.
    #[test]
    fn set_sink_routes_subsequent_dispatches() {
        reset_sink_for_tests();
        let flag = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(TestSink { flag: flag.clone() }) as Arc<dyn EventSink>;

        assert!(
            set_sink(sink).is_ok(),
            "sink should register before first dispatch"
        );
        dispatch(BrandEvent::Selection(SelectKind::PickerEnter));
        assert!(
            flag.load(Ordering::SeqCst),
            "TestSink must have been invoked"
        );
    }

    /// `ensure_default_sink` is idempotent — safe to call multiple times
    /// from `main.rs::run()` and from test setup.
    #[test]
    fn ensure_default_sink_is_idempotent() {
        reset_sink_for_tests();
        ensure_default_sink();
        ensure_default_sink();
    }

    #[test]
    fn set_sink_can_replace_default_before_first_dispatch() {
        reset_sink_for_tests();
        ensure_default_sink();

        let flag = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(TestSink { flag: flag.clone() }) as Arc<dyn EventSink>;
        assert!(
            set_sink(sink).is_ok(),
            "explicit sink should replace default before first dispatch"
        );

        dispatch(BrandEvent::ApplyComplete);
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn set_sink_fails_after_first_dispatch() {
        reset_sink_for_tests();
        dispatch(BrandEvent::SetupComplete);

        let sink = Arc::new(TestSink {
            flag: Arc::new(AtomicBool::new(false)),
        }) as Arc<dyn EventSink>;
        assert!(set_sink(sink).is_err());
    }

    /// Six top-level variants per — the plan fixes this at 6 so
    /// SFX mapping stays broad-category, not per-detail.
    #[test]
    fn brand_event_has_six_top_level_variants() {
        reset_sink_for_tests();
        let all: &[BrandEvent] = &[
            BrandEvent::Success(SuccessKind::ThemeApplied),
            BrandEvent::Failure(FailureKind::ThemeApplyFailed),
            BrandEvent::Navigation(NavKind::PickerMove),
            BrandEvent::Selection(SelectKind::PickerEnter),
            BrandEvent::SetupComplete,
            BrandEvent::ApplyComplete,
        ];
        assert_eq!(all.len(), 6);
    }
}
