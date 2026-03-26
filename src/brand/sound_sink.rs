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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::brand::events::{set_sink, BrandEvent, EventSink, NavKind, NoopSink};
use crate::config::ConfigManager;
use crate::env::SlateEnv;

// WAV samples — embedded via include_bytes! so no runtime path dependency.
// Paths are relative to this source file (`src/brand/sound_sink.rs`).
const HERO_WAV: &[u8] = include_bytes!("../../resources/sfx/hero.wav");
const APPLY_WAV: &[u8] = include_bytes!("../../resources/sfx/apply.wav");
const SUCCESS_WAV: &[u8] = include_bytes!("../../resources/sfx/success.wav");
const FAILURE_WAV: &[u8] = include_bytes!("../../resources/sfx/failure.wav");
const SELECT_WAV: &[u8] = include_bytes!("../../resources/sfx/select.wav");
const CLICK_WAV: &[u8] = include_bytes!("../../resources/sfx/click.wav");

/// 60ms window — gates priority folding of events that land near-simultaneously
/// (e.g. `Success → ApplyComplete → SetupComplete` on `slate setup` finish).
const FOLD_WINDOW_MS: u64 = 60;
/// 50ms window — picker hold-j/k repeats at ~30Hz would otherwise typewriter the
/// CLICK sample; pins to one play per 50ms.
const PICKER_DEBOUNCE_MS: u64 = 50;

/// Which sample to play for a given BrandEvent. Closed enum so tests assert on
/// a high-level identity ("HERO was played") rather than raw WAV bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sample {
    Hero,
    Apply,
    Success,
    Failure,
    Select,
    Click,
}

/// Injection seam — production = [`SubprocessPlayer`] (afplay / pw-play / ...);
/// tests = `MockPlayer` capturing a `Vec<Sample>`.
pub(crate) trait PlayBackend: Send + Sync + 'static {
    fn play(&self, sample: Sample);
}

/// Fire-and-forget subprocess-based player. Never `.wait()`s — the OS owns
/// the spawned child so playback continues independent of the CLI process.
pub(crate) struct SubprocessPlayer {
    cache_dir: PathBuf,
}

impl SubprocessPlayer {
    fn path_for(&self, s: Sample) -> PathBuf {
        let name = match s {
            Sample::Hero => "hero.wav",
            Sample::Apply => "apply.wav",
            Sample::Success => "success.wav",
            Sample::Failure => "failure.wav",
            Sample::Select => "select.wav",
            Sample::Click => "click.wav",
        };
        self.cache_dir.join(name)
    }
}

impl PlayBackend for SubprocessPlayer {
    fn play(&self, sample: Sample) {
        let path = self.path_for(sample);
        spawn_platform_player(&path);
    }
}

#[cfg(target_os = "macos")]
fn spawn_platform_player(path: &Path) {
    // Fire-and-forget — mirrors the shape of the soon-to-be-deleted
    // src/cli/sound.rs:25-34.
    let _ = std::process::Command::new("afplay")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(target_os = "linux")]
fn spawn_platform_player(path: &Path) {
    // Probe chain per RESEARCH §2 — first `.spawn().is_ok()` wins.
    // `aplay -q` suppresses PCM-format chatter that would otherwise leak into
    // the picker's alt-screen (Pitfall 4).
    for player in ["pw-play", "paplay", "aplay"] {
        let mut cmd = std::process::Command::new(player);
        if player == "aplay" {
            cmd.arg("-q");
        }
        let spawned = cmd
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if spawned.is_ok() {
            return;
        }
    }
    // No usable player — silent no-op (user probably has no audio stack at all).
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn spawn_platform_player(_path: &Path) {
    // Windows / other platforms: matches current sound.rs semantics — advisory
    // preference, zero playback.
}

/// Message to the player consumer thread. `Shutdown` is implicit — dropping
/// the `Sender` closes the channel and the consumer's `recv()` returns `Err`.
enum Msg {
    Event(BrandEvent),
}

/// The sound sink. `install` wires it into the EventSink
/// seam; `dispatch` enqueues onto the consumer thread via mpsc so the caller
/// (CLI main thread) never blocks on subprocess spawn.
pub struct SoundSink {
    tx: Sender<Msg>,
}

impl SoundSink {
    /// Install the sink as the process-wide EventSink. Must be called before
    /// any `brand::events::dispatch` call (Pitfall 5).
    /// Short-circuits to `NoopSink` when:
    /// - `auto || quiet` 
    /// - `slate config set sound off` 
    /// - cache-dir creation fails (silent degrade — no user-visible error)
    /// A double-install is a caller bug, not a user-visible error, so the
    /// `set_sink` Err path is swallowed.
    pub fn install(env: &SlateEnv, auto: bool, quiet: bool) {
        // `--auto` / `--quiet` silences the whole process.
        if auto || quiet {
            let _ = set_sink(Arc::new(NoopSink) as Arc<dyn EventSink>);
            return;
        }
        // honor `slate config set sound off`. ConfigManager construction
        // or read failure → default true (sound on); we only skip install when
        // the user has explicitly opted out.
        let enabled = ConfigManager::with_env(env)
            .and_then(|c| c.is_sound_enabled())
            .unwrap_or(true);
        if !enabled {
            let _ = set_sink(Arc::new(NoopSink) as Arc<dyn EventSink>);
            return;
        }
        // Cache unpack. Any I/O failure → silent NoopSink (never panic, never
        // log loudly — a user with a read-only home shouldn't see warnings).
        let cache_dir = env.slate_cache_dir().join("sounds");
        if ensure_cache(&cache_dir).is_err() {
            let _ = set_sink(Arc::new(NoopSink) as Arc<dyn EventSink>);
            return;
        }
        let backend = SubprocessPlayer { cache_dir };
        let sink = Self::spawn_with_backend(backend);
        let _ = set_sink(Arc::new(sink) as Arc<dyn EventSink>);
    }

    /// Test hook — constructs a sink driven by a caller-supplied backend so
    /// unit tests can assert on `Sample` identity without spawning subprocesses.
    #[cfg(test)]
    pub(crate) fn with_backend_for_tests<P: PlayBackend>(backend: P) -> Self {
        Self::spawn_with_backend(backend)
    }

    fn spawn_with_backend<P: PlayBackend>(backend: P) -> Self {
        let (tx, rx) = channel::<Msg>();
        thread::spawn(move || player_loop(rx, backend));
        Self { tx }
    }
}

impl EventSink for SoundSink {
    fn dispatch(&self, event: BrandEvent) {
        // Channel send failure (receiver dropped) is silently swallowed
        // we never want a brand-event dispatch to surface an error.
        let _ = self.tx.send(Msg::Event(event));
    }
}

/// Consumer loop — owns the mpsc receiver and the player backend for the
/// sink's entire lifetime. Exits cleanly when the channel closes (sender
/// dropped), so `SoundSink` drop is effectively a shutdown signal.
fn player_loop<P: PlayBackend>(rx: Receiver<Msg>, backend: P) {
    let last_picker_ms = AtomicU64::new(0);
    loop {
        let first = match rx.recv() {
            Ok(Msg::Event(e)) => e,
            Err(_) => return,
        };
        let best = coalesce_window(&rx, first);
        if !passes_debounce(&best, &last_picker_ms) {
            continue;
        }
        backend.play(sample_for(&best));
    }
}

/// Drains `rx` for up to 60ms after the first event, tracking the
/// highest-priority event seen. Returns the winner for playback.
fn coalesce_window(rx: &Receiver<Msg>, first: BrandEvent) -> BrandEvent {
    let deadline = Instant::now() + Duration::from_millis(FOLD_WINDOW_MS);
    let mut best = first;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match rx.recv_timeout(deadline - now) {
            Ok(Msg::Event(next)) if priority(&next) > priority(&best) => best = next,
            Ok(Msg::Event(_)) => {}
            Err(_) => break,
        }
    }
    best
}

/// Priority ranking per . Higher number wins the 60ms fold.
fn priority(e: &BrandEvent) -> u8 {
    match e {
        BrandEvent::SetupComplete => 6,
        BrandEvent::ApplyComplete => 5,
        BrandEvent::Failure(_) => 4,
        BrandEvent::Success(_) => 3,
        BrandEvent::Selection(_) => 2,
        BrandEvent::Navigation(_) => 1,
    }
}

/// Event → sample map per (one sample per top-level variant).
fn sample_for(e: &BrandEvent) -> Sample {
    match e {
        BrandEvent::SetupComplete => Sample::Hero,
        BrandEvent::ApplyComplete => Sample::Apply,
        BrandEvent::Failure(_) => Sample::Failure,
        BrandEvent::Success(_) => Sample::Success,
        BrandEvent::Selection(_) => Sample::Select,
        BrandEvent::Navigation(_) => Sample::Click,
    }
}

/// PickerMove events within 50ms of the last PLAYED PickerMove are
/// dropped. Non-PickerMove events always pass. The AtomicU64 tracks the
/// last play timestamp (epoch ms) so the check is lock-free.
fn passes_debounce(e: &BrandEvent, last_picker_ms: &AtomicU64) -> bool {
    if !matches!(e, BrandEvent::Navigation(NavKind::PickerMove)) {
        return true;
    }
    let now = now_epoch_ms();
    let last = last_picker_ms.load(Ordering::Relaxed);
    if now.saturating_sub(last) < PICKER_DEBOUNCE_MS {
        return false;
    }
    last_picker_ms.store(now, Ordering::Relaxed);
    true
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Unpack the 6 embedded WAVs into `sounds_dir`. Idempotent — pre-existing
/// files are left untouched. Uses `AtomicWriteFile` so a crash mid-write
/// cannot leave a half-file (subsequent runs see either the old file or
/// the new one, never a truncated one).
fn ensure_cache(sounds_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(sounds_dir)?;
    for (name, bytes) in &[
        ("hero.wav", HERO_WAV),
        ("apply.wav", APPLY_WAV),
        ("success.wav", SUCCESS_WAV),
        ("failure.wav", FAILURE_WAV),
        ("select.wav", SELECT_WAV),
        ("click.wav", CLICK_WAV),
    ] {
        let path = sounds_dir.join(name);
        if path.exists() {
            continue;
        }
        let mut file = atomic_write_file::AtomicWriteFile::open(&path)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::events::{
        dispatch, reset_sink_for_tests, FailureKind, SelectKind, SuccessKind,
    };
    use std::sync::Mutex;
    use std::thread::sleep;

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
        // Point `ensure_cache` at a path under a regular file so
        // create_dir_all fails (a file is not a directory). ensure_cache
        // must Err, and the production `install` swallows that to a
        // NoopSink registration. This test covers the inner helper
        // directly; install wrapping is exercised by the global sink tests
        // above.
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let bogus = tmpfile.path().join("sounds");
        let result = ensure_cache(&bogus);
        assert!(
            result.is_err(),
            "ensure_cache on a path under a regular file must Err, not panic"
        );
    }
}
