//! Event loop and rendering for the interactive crossterm picker.
//! Built on crossterm for live preview support.

use crate::brand::events::{dispatch, BrandEvent, NavKind, SelectKind};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use std::io::{self, Write as _};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

use super::actions::{quick_resume_auto, quick_save_auto};
use super::render::{
    get_effective_opacity_for_rendering, is_ghostty, render, render_afterglow_receipt,
    should_guard_light_theme_opacity,
};
use super::state::PickerState;

/// Flash message shown at the bottom of the picker for ~900ms.
struct Flash {
    text: String,
    until: Instant,
}

/// Terminal state cleanup guard — restores screen on drop even if we panic.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()
            .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)
            .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Launch the interactive 2D picker for theme + opacity selection.
pub fn launch_picker(env: &SlateEnv) -> Result<()> {
    let config = crate::config::ConfigManager::with_env(env)?;
    let starting_theme_id = config
        .get_current_theme()?
        .unwrap_or_else(|| "catppuccin-mocha".to_string());
    let starting_opacity = config
        .get_current_opacity_preset()
        .unwrap_or(OpacityPreset::Solid);

    let mut state = PickerState::new(&starting_theme_id, starting_opacity)?;
    let _guard = TerminalGuard::enter()?;

    let effective = get_effective_opacity_for_rendering(&state);
    let _ = crate::cli::set::silent_preview_apply(env, state.get_current_theme_id(), effective);

    let exit_action = event_loop(env, &mut state)?;

    // Picker Enter tactile feedback — brief reverse-video flash before leaving alt screen
    if matches!(exit_action, ExitAction::Commit) {
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            crossterm::style::SetAttribute(crossterm::style::Attribute::Reverse)
        );
        // Re-render current view with inverted colors for a brief tactile flash
        let _ = render(&state, Some("Applied!"));
        let _ = execute!(
            stdout,
            crossterm::style::SetAttribute(crossterm::style::Attribute::Reset)
        );
        let _ = stdout.flush();
        std::thread::sleep(Duration::from_millis(80));
    }

    drop(_guard);

    match exit_action {
        ExitAction::Commit => {
            state.commit();
            let theme_id = state.get_current_theme_id().to_string();
            let opacity = get_effective_opacity_for_rendering(&state);
            crate::cli::set::silent_commit_apply(env, &theme_id, opacity)?;
            render_afterglow_receipt(&state, env)?;
            crate::cli::sound::play_feedback();
        }
        ExitAction::Cancel => {
            let _ = crate::cli::set::silent_preview_apply(
                env,
                state.original_theme_id(),
                state.original_opacity(),
            );
        }
    }

    Ok(())
}

enum ExitAction {
    Commit,
    Cancel,
}

fn event_loop(env: &SlateEnv, state: &mut PickerState) -> Result<ExitAction> {
    let mut flash: Option<Flash> = None;
    let mut dirty = true;

    loop {
        if dirty {
            render(state, flash.as_ref().map(|flash| flash.text.as_str()))?;
            dirty = false;
        }

        if let Some(current_flash) = &flash {
            if Instant::now() >= current_flash.until {
                flash = None;
                dirty = true;
            }
        }

        if !event::poll(Duration::from_millis(150))
            .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?
        {
            continue;
        }

        let first =
            event::read().map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?;
        let mut last_key_event = match &first {
            Event::Key(key) => Some(*key),
            _ => None,
        };
        let mut had_resize = matches!(&first, Event::Resize(_, _));

        while event::poll(Duration::ZERO)
            .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?
        {
            match event::read()
                .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?
            {
                Event::Key(key) => match key.code {
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                        last_key_event = Some(key);
                        break;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        last_key_event = Some(key);
                        break;
                    }
                    _ => {
                        last_key_event = Some(key);
                    }
                },
                Event::Resize(_, _) => {
                    had_resize = true;
                }
                _ => {}
            }
        }

        if let Some(key) = last_key_event {
            match handle_key(key, state, env, &mut flash)? {
                KeyOutcome::Continue => {
                    dirty = true;
                    let effective = get_effective_opacity_for_rendering(state);
                    let _ = crate::cli::set::silent_preview_apply(
                        env,
                        state.get_current_theme_id(),
                        effective,
                    );
                }
                KeyOutcome::Inert => {}
                KeyOutcome::Commit => return Ok(ExitAction::Commit),
                KeyOutcome::Cancel => return Ok(ExitAction::Cancel),
            }
        }

        if had_resize {
            dirty = true;
        }
    }
}

enum KeyOutcome {
    Continue,
    Inert,
    Commit,
    Cancel,
}

fn handle_key(
    key: KeyEvent,
    state: &mut PickerState,
    env: &SlateEnv,
    flash: &mut Option<Flash>,
) -> Result<KeyOutcome> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_up();
            // D-17: picker navigation — NoopSink in Phase 18, SoundSink in Phase 20.
            dispatch(BrandEvent::Navigation(NavKind::PickerMove));
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_down();
            dispatch(BrandEvent::Navigation(NavKind::PickerMove));
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if !crate::detection::TerminalProfile::detect().supports_opacity() {
                return Ok(KeyOutcome::Inert);
            }
            let was_guarded = should_guard_light_theme_opacity(state);
            state.set_opacity_override(true);
            let at_edge = state.move_left();
            if at_edge {
                *flash = Some(Flash {
                    text: "← Solid (hard stop)".to_string(),
                    until: Instant::now() + Duration::from_millis(500),
                });
            } else if was_guarded {
                *flash = Some(Flash {
                    text: "(!) Translucent light themes may reduce text contrast".to_string(),
                    until: Instant::now() + Duration::from_millis(1200),
                });
            }
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if !crate::detection::TerminalProfile::detect().supports_opacity() {
                return Ok(KeyOutcome::Inert);
            }
            let was_guarded = should_guard_light_theme_opacity(state);
            state.set_opacity_override(true);
            let at_edge = state.move_right();
            if at_edge {
                *flash = Some(Flash {
                    text: "→ Clear (hard stop)".to_string(),
                    until: Instant::now() + Duration::from_millis(500),
                });
            } else if was_guarded {
                *flash = Some(Flash {
                    text: "(!) Translucent light themes may reduce text contrast".to_string(),
                    until: Instant::now() + Duration::from_millis(1200),
                });
            } else if state.get_current_opacity() == OpacityPreset::Frosted && !is_ghostty() {
                *flash = Some(Flash {
                    text: "(i) Frosted is approximated here · Ghostty shows full blur".to_string(),
                    until: Instant::now() + Duration::from_millis(1200),
                });
            }
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Char('s') => {
            let text = quick_save_auto(state, env)?;
            *flash = Some(Flash {
                text,
                until: Instant::now() + Duration::from_millis(1200),
            });
            Ok(KeyOutcome::Inert)
        }
        KeyCode::Char('r') => {
            if let Some(text) = quick_resume_auto(state, env) {
                *flash = Some(Flash {
                    text,
                    until: Instant::now() + Duration::from_millis(1200),
                });
            }
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Enter => {
            // D-17: picker Enter → Selection. Fires IN ADDITION to the existing
            // `crate::cli::sound::play_feedback` call from `launch_picker`'s
            // Commit branch — Phase 18 does not delete `sound.rs`; Phase 20's
            // SoundSink will supersede `play_feedback` once registered.
            dispatch(BrandEvent::Selection(SelectKind::PickerEnter));
            Ok(KeyOutcome::Commit)
        }
        KeyCode::Esc | KeyCode::Char('q') => Ok(KeyOutcome::Cancel),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Ok(KeyOutcome::Cancel)
        }
        _ => Ok(KeyOutcome::Inert),
    }
}

#[cfg(test)]
mod tests {
    //! Wave-5 picker key → BrandEvent dispatch unit tests.
    //!
    //! Rather than drive the whole alt-screen event loop, we call
    //! `handle_key` directly with synthetic `KeyEvent`s and assert the
    //! shared `OnceLock` sink tally. Private `handle_key` + `Flash` are
    //! reachable here because this module lives next to them in the same
    //! crate.
    //!
    //! Note: the `brand::events` sink is a process-global `OnceLock`
    //! shared across lib unit tests. We piggy-back on whatever sink was
    //! seated first; if the default `NoopSink` won the race, these tests
    //! degrade to smoke tests (the `handle_key` branches still run
    //! without panicking). Phase 20's integration target will exercise
    //! the fresh-process case against `SoundSink`; the routing contract
    //! lives in `tests/wave5_picker_events.rs`.

    use super::*;
    use crate::brand::events::{set_sink, EventSink};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct PickerCountingSink {
        picker_move: AtomicUsize,
        picker_enter: AtomicUsize,
    }

    impl EventSink for PickerCountingSink {
        fn dispatch(&self, event: BrandEvent) {
            match event {
                BrandEvent::Navigation(NavKind::PickerMove) => {
                    self.picker_move.fetch_add(1, Ordering::SeqCst);
                }
                BrandEvent::Selection(SelectKind::PickerEnter) => {
                    self.picker_enter.fetch_add(1, Ordering::SeqCst);
                }
                _ => {}
            }
        }
    }

    /// Try to seat a `PickerCountingSink`. Returns `None` if another sink
    /// (e.g. `NoopSink` from an earlier test) already won the `OnceLock`,
    /// in which case these tests fall back to smoke-testing that
    /// `handle_key` doesn't panic on the target key codes.
    fn try_seat_picker_sink() -> Option<Arc<PickerCountingSink>> {
        let sink = Arc::new(PickerCountingSink::default());
        match set_sink(sink.clone() as Arc<dyn EventSink>) {
            Ok(()) => Some(sink),
            Err(_) => None,
        }
    }

    fn dummy_env() -> SlateEnv {
        SlateEnv::with_home(PathBuf::from("/tmp/slate-picker-test-home"))
    }

    fn fresh_state() -> PickerState {
        PickerState::new("catppuccin-mocha", OpacityPreset::Solid)
            .expect("picker state must build from registry")
    }

    #[test]
    fn picker_nav_keys_fire_picker_move_event() {
        let sink = try_seat_picker_sink();
        let env = dummy_env();
        let mut state = fresh_state();
        let mut flash: Option<Flash> = None;

        let before_move = sink.as_ref().map(|s| s.picker_move.load(Ordering::SeqCst));
        let _ = handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("Down key must not error");
        let _ = handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("Up key must not error");
        let _ = handle_key(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("j key must not error");
        let _ = handle_key(
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("k key must not error");

        if let (Some(sink), Some(before)) = (sink, before_move) {
            let delta = sink.picker_move.load(Ordering::SeqCst) - before;
            assert_eq!(
                delta, 4,
                "four nav keys (Down/Up/j/k) should dispatch PickerMove exactly 4 times"
            );
        }
        // If the sink couldn't be seated (another test won the OnceLock),
        // the handle_key calls above at least proved no panic on target keys.
    }

    #[test]
    fn picker_enter_fires_picker_enter_event_and_commits() {
        let sink = try_seat_picker_sink();
        let env = dummy_env();
        let mut state = fresh_state();
        let mut flash: Option<Flash> = None;

        let before_enter = sink.as_ref().map(|s| s.picker_enter.load(Ordering::SeqCst));
        let outcome = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("Enter key must not error");

        assert!(
            matches!(outcome, KeyOutcome::Commit),
            "Enter must return Commit, got {outcome:?}"
        );

        if let (Some(sink), Some(before)) = (sink, before_enter) {
            let delta = sink.picker_enter.load(Ordering::SeqCst) - before;
            assert_eq!(delta, 1, "Enter should dispatch PickerEnter exactly once");
        }
    }

    impl std::fmt::Debug for KeyOutcome {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                KeyOutcome::Continue => f.write_str("Continue"),
                KeyOutcome::Inert => f.write_str("Inert"),
                KeyOutcome::Commit => f.write_str("Commit"),
                KeyOutcome::Cancel => f.write_str("Cancel"),
            }
        }
    }
}
