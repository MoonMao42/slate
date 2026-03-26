//! SFX sink. Implements `EventSink` to intercept `BrandEvent`
//! dispatches from the 40 Phase-18-planted sites and play subtle SFX.
//! Contract (see ``):
//! - 6 samples, 1-per-variant
//! - PickerMove 50ms debounce
//! - 60ms priority-fold window (SetupComplete > ApplyComplete > Failure > Success > Selection > Navigation)
//! - default ON, opt-out via `slate config set sound off`
//! - `auto || quiet` → NoopSink
//! - lives in `src/brand/` (not `src/cli/`)
//! Implementation pattern: dedicated consumer thread + `std::sync::mpsc`
//! channel + `recv_timeout(60ms)` coalesce loop (RESEARCH §8).
//! Fire-and-forget subprocess playback (RESEARCH §2).

use std::sync::mpsc::Sender;

use crate::brand::events::BrandEvent;
use crate::env::SlateEnv;

// WAV samples — populated in . Const refs kept here so the
// embedded sample paths are audited in a single file.
#[allow(dead_code)]
const HERO_WAV: &[u8] = include_bytes!("../../resources/sfx/hero.wav");
#[allow(dead_code)]
const APPLY_WAV: &[u8] = include_bytes!("../../resources/sfx/apply.wav");
#[allow(dead_code)]
const SUCCESS_WAV: &[u8] = include_bytes!("../../resources/sfx/success.wav");
#[allow(dead_code)]
const FAILURE_WAV: &[u8] = include_bytes!("../../resources/sfx/failure.wav");
#[allow(dead_code)]
const SELECT_WAV: &[u8] = include_bytes!("../../resources/sfx/select.wav");
#[allow(dead_code)]
const CLICK_WAV: &[u8] = include_bytes!("../../resources/sfx/click.wav");

/// Which sample to play for a given BrandEvent. Closed enum for tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Sample {
    Hero,
    Apply,
    Success,
    Failure,
    Select,
    Click,
}

/// Injection seam — production = SubprocessPlayer (afplay/pw-play/...);
/// tests = MockPlayer capturing a Vec<Sample>.
#[allow(dead_code)]
pub(crate) trait PlayBackend: Send + Sync {
    fn play(&self, sample: Sample);
}

/// Message to the player consumer thread. Single variant for now
/// may add `Shutdown`.
#[allow(dead_code)]
enum Msg {
    Event(BrandEvent),
}

/// The sound sink. `install` wires it into the 
/// EventSink seam; dispatches enqueue onto the consumer thread via mpsc.
pub struct SoundSink {
    // Populated in.
    #[allow(dead_code)]
    tx: Option<Sender<Msg>>,
}

impl SoundSink {
    /// Install the sink as the process-wide EventSink. Must be called
    /// before any `brand::events::dispatch` call (Pitfall 5).
    /// `auto || quiet` → register `NoopSink` (silent across the whole
    /// process). Otherwise — wires the consumer thread
    /// + cache extraction + real subprocess player.
    /// Any I/O failure (cache extraction, thread spawn) degrades to a
    /// silent `NoopSink` with no user-visible error.
    pub fn install(_env: &SlateEnv, _auto: bool, _quiet: bool) {
        // RED phase stub — GREEN phase replaces with real logic.
    }

    /// RED stub — returns a sink whose `dispatch` is a no-op so the
    /// routing / priority-fold / debounce tests fail (MockPlayer records
    /// nothing). GREEN phase wires the real consumer thread.
    #[cfg(test)]
    pub(crate) fn with_backend_for_tests<P: PlayBackend + 'static>(_backend: P) -> Self {
        Self { tx: None }
    }
}

impl crate::brand::events::EventSink for SoundSink {
    fn dispatch(&self, _event: BrandEvent) {
        // RED stub — GREEN phase forwards to the mpsc consumer thread.
    }
}

/// RED stub — GREEN phase replaces with the real cache-unpack helper.
#[cfg(test)]
fn ensure_cache(_sounds_dir: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::events::{
        dispatch, reset_sink_for_tests, set_sink, EventSink, FailureKind, NavKind, NoopSink,
        SelectKind, SuccessKind,
    };
    use crate::config::ConfigManager;
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;

    /// Serialize tests that mutate the global sink singleton. Prevents
    /// cross-test races in the shared lib-test process without relying on
    /// `--test-threads=1` or an extra `serial_test` dependency.
    static SINK_LOCK: Mutex<()> = Mutex::new(());

    /// Test-only capture: pushes every `Sample` the sink plays into a
    /// shared `Vec` so assertions can inspect order + debounce + fold.
    pub(crate) struct MockPlayer {
        pub played: Arc<Mutex<Vec<Sample>>>,
    }

    impl MockPlayer {
        pub fn new() -> (Self, Arc<Mutex<Vec<Sample>>>) {
            let played = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    played: played.clone(),
                },
                played,
            )
        }
    }

    impl PlayBackend for MockPlayer {
        fn play(&self, s: Sample) {
            self.played.lock().unwrap().push(s);
        }
    }

    #[test]
    fn routing_maps_each_brand_event_to_correct_sample() {
        let (mock, played) = MockPlayer::new();
        let sink = SoundSink::with_backend_for_tests(mock);
        // Space events past the 60ms fold window so each one plays
        // independently:
        sink.dispatch(BrandEvent::Success(SuccessKind::ThemeApplied));
        sleep(Duration::from_millis(90));
        sink.dispatch(BrandEvent::Failure(FailureKind::ThemeApplyFailed));
        sleep(Duration::from_millis(90));
        sink.dispatch(BrandEvent::Selection(SelectKind::PickerEnter));
        sleep(Duration::from_millis(90));
        sink.dispatch(BrandEvent::SetupComplete);
        sleep(Duration::from_millis(90));
        sink.dispatch(BrandEvent::ApplyComplete);
        sleep(Duration::from_millis(150));
        // PickerMove verified separately (picker_move_debounce test) so
        // we skip it here — this spacing interleaves with the 50ms
        // debounce and would make the assertion flaky.
        let p = played.lock().unwrap().clone();
        assert_eq!(
            p,
            vec![
                Sample::Success,
                Sample::Failure,
                Sample::Select,
                Sample::Hero,
                Sample::Apply,
            ],
            "6 BrandEvent variants map 1-to-1 to the 6 Sample variants"
        );
    }

    #[test]
    fn priority_fold_picks_highest_in_60ms_window() {
        let (mock, played) = MockPlayer::new();
        let sink = SoundSink::with_backend_for_tests(mock);
        sink.dispatch(BrandEvent::Success(SuccessKind::ThemeApplied));
        sink.dispatch(BrandEvent::ApplyComplete);
        sink.dispatch(BrandEvent::SetupComplete);
        sleep(Duration::from_millis(200));
        let p = played.lock().unwrap().clone();
        assert_eq!(
            p,
            vec![Sample::Hero],
            "priority fold: SetupComplete wins inside 60ms window"
        );
    }

    #[test]
    fn picker_move_debounce_50ms_drops_second_within_window() {
        let (mock, played) = MockPlayer::new();
        let sink = SoundSink::with_backend_for_tests(mock);
        // First PickerMove: plays after 60ms fold window.
        sink.dispatch(BrandEvent::Navigation(NavKind::PickerMove));
        sleep(Duration::from_millis(80));
        // Second PickerMove: within 50ms of first's play — debounced.
        sink.dispatch(BrandEvent::Navigation(NavKind::PickerMove));
        sleep(Duration::from_millis(20));
        // Third PickerMove: still within 50ms — debounced.
        sink.dispatch(BrandEvent::Navigation(NavKind::PickerMove));
        sleep(Duration::from_millis(200));
        let p = played.lock().unwrap().clone();
        assert!(
            p.iter().all(|s| *s == Sample::Click),
            "all plays must be click samples: {:?}",
            p
        );
        assert!(!p.is_empty(), "at least one PickerMove plays");
        assert!(
            p.len() <= 2,
            "50ms debounce should drop at least one of the 3 rapid PickerMoves: {:?}",
            p
        );
    }

    #[test]
    fn install_with_auto_or_quiet_registers_noop_sink() {
        // either flag alone must short-circuit install to register
        // a NoopSink. Sequenced (not parametric) to keep the assertions in
        // a single test — acceptance criterion calls for 7 tests total.
        let _g = SINK_LOCK.lock().unwrap();

        // Case 1: auto=true.
        reset_sink_for_tests();
        let tmp1 = tempfile::TempDir::new().unwrap();
        let env1 = SlateEnv::with_home(tmp1.path().to_path_buf());
        SoundSink::install(&env1, /*auto=*/ true, /*quiet=*/ false);
        let dummy = Arc::new(NoopSink) as Arc<dyn EventSink>;
        assert!(
            set_sink(dummy).is_err(),
            "install(auto=true) must latch a sink slot"
        );

        // Case 2: quiet=true (reset the sink state between cases).
        reset_sink_for_tests();
        let tmp2 = tempfile::TempDir::new().unwrap();
        let env2 = SlateEnv::with_home(tmp2.path().to_path_buf());
        SoundSink::install(&env2, false, true);
        let dummy2 = Arc::new(NoopSink) as Arc<dyn EventSink>;
        assert!(
            set_sink(dummy2).is_err(),
            "install(quiet=true) must latch a sink slot"
        );
    }

    #[test]
    fn setup_cascade_folds_to_setup_complete_hero() {
        let (mock, played) = MockPlayer::new();
        let sink = SoundSink::with_backend_for_tests(mock);
        sink.dispatch(BrandEvent::Success(SuccessKind::ThemeApplied));
        sleep(Duration::from_millis(5));
        sink.dispatch(BrandEvent::ApplyComplete);
        sleep(Duration::from_millis(5));
        sink.dispatch(BrandEvent::SetupComplete);
        sleep(Duration::from_millis(200));
        let p = played.lock().unwrap().clone();
        assert_eq!(
            p,
            vec![Sample::Hero],
            "realistic setup cascade folds to the single HERO"
        );
    }

    #[test]
    fn disabled_config_silent() {
        // End-to-end write sound=off under an injected SlateEnv, then
        // install — the sink slot must be latched with a silent NoopSink,
        // and subsequent dispatches must not panic.
        // No std::env::set_var — tests stay pure per user rule
        // `feedback_no_tech_debt` (no global env var mutation in tests).
        let _g = SINK_LOCK.lock().unwrap();
        reset_sink_for_tests();
        let tmp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tmp.path().to_path_buf());
        let cm = ConfigManager::with_env(&env).unwrap();
        cm.set_sound_enabled(false).unwrap();

        SoundSink::install(&env, false, false);
        // Slot latched — a follow-up set_sink Errs regardless of which
        // sink `install` chose (NoopSink, SoundSink, …).
        let dummy = Arc::new(NoopSink) as Arc<dyn EventSink>;
        assert!(
            set_sink(dummy).is_err(),
            "install(sound=off) must register a sink"
        );
        // No panic on dispatch.
        dispatch(BrandEvent::ApplyComplete);
    }

    #[test]
    fn cache_unpack_failure_degrades_to_silent_noop() {
        // Point `ensure_cache` at a *file* path — create_dir_all must Err,
        // ensure_cache returns Err, and the production `install` swallows
        // that to a NoopSink registration. This test covers the inner
        // helper directly (install wrapping is exercised by the global
        // sink tests above).
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let bogus = tmpfile.path().join("sounds");
        // tmpfile is a file; bogus is <file>/sounds — create_dir_all on
        // a path whose parent is a file returns Err.
        let result = ensure_cache(&bogus);
        assert!(
            result.is_err(),
            "ensure_cache on a path under a regular file must Err, not panic"
        );
    }
}
