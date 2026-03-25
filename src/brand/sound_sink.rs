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
        // body goes here. For , silent no-op so
        // callers (main.rs) can be wired unconditionally later.
    }
}

#[cfg(test)]
mod tests {
    // 6 ignored stubs — fleshes them out.

    #[test]
    #[ignore = " — filled with SoundSink wiring"]
    fn priority_fold_picks_highest_in_60ms_window() {}

    #[test]
    #[ignore = " "]
    fn picker_move_debounce_50ms_drops_second_within_window() {}

    #[test]
    #[ignore = " "]
    fn install_with_auto_or_quiet_registers_noop_sink() {}

    #[test]
    #[ignore = " "]
    fn sound_off_config_yields_silent_dispatch() {}

    #[test]
    #[ignore = " "]
    fn setup_cascade_folds_to_setup_complete_hero() {}

    #[test]
    #[ignore = " "]
    fn cache_unpack_failure_degrades_to_silent_noop() {}
}
