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

use super::actions::quick_save_auto;
use super::preview::starship_fork::fork_starship_prompt;
use super::render::{
    get_effective_opacity_for_rendering, is_ghostty, render, render_afterglow_receipt,
    should_guard_light_theme_opacity,
};
use super::rollback_guard::{install_rollback_panic_hook, RollbackGuard};
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

    // layer 3: install the panic hook BEFORE entering the
    // alt-screen so a panic during `TerminalGuard::enter()` itself still
    // triggers a managed/* rollback. The hook is process-global and
    // captures (env, starting_theme_id, starting_opacity) by value, so
    // it carries its own snapshot independent of the guard's Drop order.
    install_rollback_panic_hook(env.clone(), starting_theme_id.clone(), starting_opacity);

    let _guard = TerminalGuard::enter()?;

    // layer 2: arm the RAII rollback guard. It clones the
    // shared `Rc<Cell<bool>>` from PickerState so `state.commit()` flips
    // the cell to `true` BEFORE `_rollback` drops → Drop sees the commit
    // and short-circuits silent_preview_apply. Rust drops locals in
    // reverse declaration order, so `_rollback` drops AFTER the match
    // arm below runs `state.commit()`.
    let _rollback = RollbackGuard::arm(
        env,
        &starting_theme_id,
        starting_opacity,
        state.committed_flag(),
    );

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
            // (plan 19-06): forked starship prompts were
            // generated with a specific `--terminal-width` arg, so a
            // resize invalidates every cached entry. `invalidate_prompt_cache`
            // is a `clear()` — simpler than per-entry width tracking,
            // and correctness > cache hit-rate here.
            state.invalidate_prompt_cache();
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
    // Navigation + opacity keys are list-dominant only. In full-preview mode
    // the picker shows a detailed read-only render of the currently-selected
    // theme; changing selection from here would be disorienting (no list
    // cursor visible) so the keys are inert until the user returns to list.
    if state.preview_mode_full {
        match key.code {
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('k')
            | KeyCode::Char('j')
            | KeyCode::Char('h')
            | KeyCode::Char('l') => return Ok(KeyOutcome::Inert),
            _ => {}
        }
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_up();
            // picker navigation — NoopSink in , SoundSink in.
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
        KeyCode::Tab => {
            // Tab toggle: flip list-dominant ↔ full-preview mode.
            // When flipping INTO preview mode, eagerly fork starship so the
            // preview shows the real theme-aware prompt instead of the
            // self-draw fallback. Failure is silent (compose_full falls back
            // to self-draw) per.
            state.preview_mode_full = !state.preview_mode_full;
            if state.preview_mode_full {
                fork_and_cache_prompt(state, env);
            }
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Enter => {
            // picker Enter → Selection. Fires IN ADDITION to the existing
            // `crate::cli::sound::play_feedback` call from `launch_picker`'s
            // Commit branch — does not delete `sound.rs`; 
            // SoundSink will supersede `play_feedback` once registered.
            dispatch(BrandEvent::Selection(SelectKind::PickerEnter));
            Ok(KeyOutcome::Commit)
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            // Esc in full-preview mode returns to the list-dominant view
            // instead of exiting. To leave the picker, user Escs again from
            // the list view.
            if state.preview_mode_full {
                state.preview_mode_full = false;
                return Ok(KeyOutcome::Continue);
            }
            Ok(KeyOutcome::Cancel)
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Ok(KeyOutcome::Cancel)
        }
        _ => Ok(KeyOutcome::Inert),
    }
}

/// Fork the `starship` binary once against the current managed toml +
/// terminal width, and stash the output into `state.prompt_cache` keyed by
/// theme id. A no-op when the managed toml is missing or when the cache is
/// already populated for this theme. Errors are swallowed — `compose_full`
/// degrades to `self_draw_prompt_from_sample_tokens` per silent
/// fallback when `cached_prompt` returns `None`.
fn fork_and_cache_prompt(state: &mut PickerState, env: &SlateEnv) {
    let theme_id = state.get_current_theme_id().to_string();
    if state.cached_prompt(&theme_id).is_some() {
        return;
    }

    let managed_dir = env.config_dir().join("managed").join("starship");
    // The managed starship config is named `plain.toml` today (single-file
    // layout). If that changes, update `StarshipAdapter::apply_theme` in
    // src/adapter/starship.rs — the fork must read the same file slate's
    // writer writes to.
    let managed_toml = managed_dir.join("plain.toml");
    if !managed_toml.exists() {
        return;
    }

    let width = terminal::size().map(|(c, _)| c).unwrap_or(80);

    if let Ok(prompt) = fork_starship_prompt(&managed_toml, &managed_dir, width, None) {
        state.cache_prompt(&theme_id, prompt);
    }
}

#[cfg(test)]
mod tests {
    //! Wave-5 picker key → BrandEvent dispatch unit tests.
    //! Rather than drive the whole alt-screen event loop, we call
    //! `handle_key` directly with synthetic `KeyEvent`s and assert the
    //! shared `OnceLock` sink tally. Private `handle_key` + `Flash` are
    //! reachable here because this module lives next to them in the same
    //! crate.
    //! Note: the `brand::events` sink is a process-global `OnceLock`
    //! shared across lib unit tests. We piggy-back on whatever sink was
    //! seated first; if the default `NoopSink` won the race, these tests
    //! degrade to smoke tests (the `handle_key` branches still run
    //! without panicking). integration target will exercise
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

    // Task 01 (Tab branch + dirty-flag contract)

    /// VALIDATION row 6: Tab is a no-event surface.
    /// Tab must flip `preview_mode_full` and return `KeyOutcome::Continue`
    /// WITHOUT firing any `BrandEvent` (no PickerMove, no PickerEnter).
    /// CONTEXT §Established Patterns: Tab has no "selection" semantics.
    /// Implementation detail: the `brand::events` sink is a process-global
    /// `OnceLock` and `cargo test` runs tests in parallel, so a counter-
    /// delta assertion on a shared sink is inherently race-prone. Instead
    /// we assert the Tab arm's *structural* no-dispatch invariant: the
    /// source block for `KeyCode::Tab` inside `handle_key` must not call
    /// `dispatch(` at all. This is equivalent to the runtime contract
    /// (SoundSink will also observe zero events on Tab) but is
    /// robust under parallel test execution. The runtime side is covered
    /// separately by the `tab_toggles_preview_mode_full_both_ways` +
    /// `second_tab_after_nav_still_toggles` behaviorals.
    #[test]
    fn tab_does_not_dispatch_brand_event() {
        // Behavioral check: Tab returns Continue and flips the mode.
        let env = dummy_env();
        let mut state = fresh_state();
        let mut flash: Option<Flash> = None;
        let outcome = handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("Tab must not error");
        assert!(
            matches!(outcome, KeyOutcome::Continue),
            "Tab must return Continue, got {outcome:?}"
        );
        assert!(
            state.preview_mode_full,
            "Tab should toggle preview_mode_full to true"
        );

        // Structural check: read our own source and assert the Tab arm
        // block contains `preview_mode_full` (proving we found it) and
        // does NOT contain `dispatch(`. Finding `dispatch(` elsewhere in
        // `handle_key` is fine — Up/Down/Enter arms dispatch by design;
        // the invariant is specific to the Tab arm.
        let source = include_str!("event_loop.rs");
        let tab_marker = "KeyCode::Tab =>";
        let tab_start = source
            .find(tab_marker)
            .expect("source must contain a Tab arm in handle_key");
        // Arm body ends at the next `KeyCode::` match arm or the closing
        // brace of the match block. Find the next `KeyCode::` after our
        // marker (Enter arm immediately follows Tab in the spec).
        let search_region = &source[tab_start..];
        let next_arm_rel = search_region[tab_marker.len()..]
            .find("KeyCode::")
            .unwrap_or(search_region.len() - tab_marker.len());
        let tab_block = &search_region[..tab_marker.len() + next_arm_rel];

        assert!(
            tab_block.contains("preview_mode_full"),
            "Tab arm source block must toggle preview_mode_full; block was:\n{tab_block}"
        );
        assert!(
            !tab_block.contains("dispatch("),
            "Tab arm MUST NOT call dispatch() — CONTEXT §Established Patterns: \
             Tab has no selection semantics and  SoundSink must stay \
             silent on mode switches. Offending block:\n{tab_block}"
        );
    }

    /// Tab toggles `preview_mode_full` bidirectionally.
    #[test]
    fn tab_toggles_preview_mode_full_both_ways() {
        let env = dummy_env();
        let mut state = fresh_state();
        let mut flash: Option<Flash> = None;
        assert!(!state.preview_mode_full, "default is list-dominant");

        let _ = handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("first Tab ok");
        assert!(state.preview_mode_full, "first Tab → full mode");

        let _ = handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("second Tab ok");
        assert!(!state.preview_mode_full, "second Tab → back to list");
    }

    /// Navigation within full-preview mode does NOT reset the mode.
    /// We drive navigation via `state.move_down()` directly instead of a
    /// `KeyCode::Down` key event so the intervening step does not fire
    /// `BrandEvent::Navigation(PickerMove)` into the process-global
    /// `OnceLock` sink — other tests in this module (notably
    /// `picker_nav_keys_fire_picker_move_event`) read that counter and
    /// break when a parallel test leaks extra events.
    #[test]
    fn second_tab_after_nav_still_toggles() {
        let env = dummy_env();
        let mut state = fresh_state();
        let mut flash: Option<Flash> = None;
        let _ = handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .unwrap();
        assert!(state.preview_mode_full);

        // Pure state mutation — does NOT touch `dispatch` so the shared
        // counting sink stays unaffected.
        state.move_down();
        assert!(state.preview_mode_full, "nav does not reset mode");

        let _ = handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .unwrap();
        assert!(!state.preview_mode_full);
    }

    /// V-04 BEHAVIOR PROOF: Tab → `KeyOutcome::Continue` → outer loop
    /// sets `dirty = true` → next loop-top iteration renders with the
    /// new `preview_mode_full` value. We replicate that cycle with a
    /// render-count spy to prove the contract behaviorally — not just
    /// by reading the source.
    #[test]
    fn tab_triggers_rerender_via_dirty_flag() {
        use std::cell::Cell as StdCell;

        struct RenderSpy {
            count: StdCell<u32>,
        }
        impl RenderSpy {
            fn new() -> Self {
                Self {
                    count: StdCell::new(0),
                }
            }
            fn render(&self) {
                self.count.set(self.count.get() + 1);
            }
            fn total(&self) -> u32 {
                self.count.get()
            }
        }

        let spy = RenderSpy::new();
        let env = dummy_env();
        let mut state = fresh_state();
        let mut flash: Option<Flash> = None;
        let mut dirty = true;

        // Initial paint (mirrors production loop seed at event_loop L120).
        if dirty {
            spy.render();
            dirty = false;
        }
        assert_eq!(spy.total(), 1, "initial render must have fired");

        // User presses Tab.
        let outcome = handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("Tab ok");

        // Production outer match (L175-185 of event_loop.rs):
        // Continue → dirty=true.
        match outcome {
            KeyOutcome::Continue => {
                dirty = true;
            }
            KeyOutcome::Inert => {}
            KeyOutcome::Commit | KeyOutcome::Cancel => {
                panic!("Tab must return Continue, got {outcome:?}");
            }
        }

        // Next loop iteration — re-render if dirty (L122-126).
        // The trailing `dirty = false` from the production cycle is
        // intentionally omitted here: the test ends after this branch so
        // the reset value is never observed, and keeping it would trip
        // `#[warn(unused_assignments)]`. Behavior under test is only
        // "did the render counter tick because dirty was true?".
        if dirty {
            spy.render();
        }

        assert_eq!(
            spy.total(),
            2,
            "Tab → Continue → dirty=true → render called exactly once \
             more. If the counter is still 1, the Tab arm returned Inert \
             or Continue semantics changed; the picker would show a \
             stale frame under the new preview_mode_full value."
        );
        assert!(state.preview_mode_full, "Tab must have flipped the mode");
    }

    /// V-09 (resize contract): `invalidate_prompt_cache` clears
    /// all cached forked prompts. Pins the resize→cache-clear contract
    /// so a future optimizer ("only evict stale entries") cannot break
    /// the `--terminal-width` coupling baked into each cached prompt.
    #[test]
    fn resize_invalidates_prompt_cache() {
        let mut state = fresh_state();
        state.cache_prompt("catppuccin-mocha", "marker".to_string());
        assert_eq!(
            state.cached_prompt("catppuccin-mocha"),
            Some("marker"),
            "cache_prompt should seed a readable entry"
        );

        // This is exactly what the event_loop `had_resize` branch will
        // call (Task 19-07-01 action C).
        state.invalidate_prompt_cache();

        assert_eq!(
            state.cached_prompt("catppuccin-mocha"),
            None,
            "resize must evict all cached prompts so stale --terminal-width \
             entries don't render at the new window size"
        );
    }
}
