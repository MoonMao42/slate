//! Event loop and rendering for the interactive crossterm picker.
//! Built on crossterm for live preview support.

use crate::brand::events::{dispatch, BrandEvent, NavKind, SelectKind};
use crate::cli::ui_language::tr;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

use super::actions::quick_save_auto;
use super::preview::starship_fork::fork_starship_prompt;
use super::preview_snapshot::PreviewSnapshot;
use super::render::{
    get_effective_opacity_for_rendering, is_ghostty, render, render_afterglow_receipt,
    should_guard_light_theme_opacity,
};
use super::rollback_guard::{install_rollback_panic_hook, RollbackGuard};
use super::state::PickerState;

mod input;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod paste_tests;
#[cfg(test)]
mod starship_tests;

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
        let guard = Self;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            Hide
        )
        .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

/// Launch the interactive 2D picker for theme + opacity selection.
pub fn launch_picker(env: &SlateEnv) -> Result<()> {
    let preview = std::sync::Arc::new(PreviewSnapshot::capture(env)?);
    let initial_state = (|| -> Result<PickerState> {
        let config = crate::config::ConfigManager::with_env(env)?;
        let theme = config
            .get_current_theme()?
            .unwrap_or_else(|| "catppuccin-mocha".to_string());
        let opacity = config
            .get_current_opacity_preset()
            .unwrap_or(OpacityPreset::Solid);
        PickerState::new(&theme, opacity)
    })();
    let mut state = match initial_state {
        Ok(state) => state,
        Err(err) => {
            if let Err(cleanup) = preview.restore() {
                eprintln!("warning: {cleanup}");
            }
            return Err(err);
        }
    };
    let committed = state.committed_flag();

    // layer 3: install the panic hook BEFORE entering the
    // alt-screen so a panic during `TerminalGuard::enter()` itself still
    // triggers a managed/* rollback. The hook is process-global and
    // shares the same committed flag as `RollbackGuard`, so a panic after a
    // successful commit will not spuriously restore the original theme.
    // layer 2: arm the RAII rollback guard. It shares the
    // same atomic commit flag as the panic hook, so `state.commit()` flips
    // one bit that suppresses every rollback path after a successful apply.
    // Rust drops locals in reverse declaration order, so `_rollback` drops
    // AFTER the match arm below runs `state.commit()`.
    let _rollback = RollbackGuard::arm(preview.clone(), committed.clone());
    let _panic_hook = install_rollback_panic_hook(&preview, committed);
    let _guard = TerminalGuard::enter()?;

    let effective = get_effective_opacity_for_rendering(&state);
    preview.apply(state.get_current_theme_id(), effective)?;

    let exit_action = event_loop(env, &mut state, &preview)?;

    // Keep the selected palette stable while saving: no reverse-video flash.
    if matches!(exit_action, ExitAction::Commit) {
        let _ = render(&state, Some(tr("正在应用…", "Applying…")));
    }

    drop(_guard);

    // Remove preview mutations before committing too: the coordinator's safety
    // snapshot must contain the user's original files, not the highlighted theme.
    match exit_action {
        ExitAction::Commit => {
            let _write_guard = preview.restore_for_commit()?;
            let theme_id = state.get_current_theme_id().to_string();
            let opacity = get_effective_opacity_for_rendering(&state);
            crate::cli::set::silent_commit_apply(
                env,
                &theme_id,
                opacity,
                state.original_theme_id(),
                state.original_opacity(),
            )?;
            state.commit();
            render_afterglow_receipt(&state, opacity)?;
        }
        ExitAction::Cancel => preview.restore()?,
    }

    Ok(())
}

enum ExitAction {
    Commit,
    Cancel,
}

fn event_loop(
    env: &SlateEnv,
    state: &mut PickerState,
    preview: &PreviewSnapshot,
) -> Result<ExitAction> {
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
        let batch = process_input_batch(
            first,
            || {
                if event::poll(Duration::ZERO)? {
                    Ok(Some(event::read()?))
                } else {
                    Ok(None)
                }
            },
            state,
            env,
            &mut flash,
            |state| {
                preview.apply(
                    state.get_current_theme_id(),
                    get_effective_opacity_for_rendering(state),
                )
            },
        )?;
        if let Some(exit) = batch.exit {
            return Ok(exit);
        }
        dirty |= batch.redraw || batch.resized;
    }
}

/// Preserve semantic input order, but publish only the final selection in a
/// bounded batch. UI-only actions and navigation back to the same selection do
/// not rewrite terminal files. Exit proceeds directly to cancel/commit cleanup.
fn process_input_batch(
    first: Event,
    next: impl FnMut() -> Result<Option<Event>>,
    state: &mut PickerState,
    env: &SlateEnv,
    flash: &mut Option<Flash>,
    apply: impl FnOnce(&PickerState) -> Result<()>,
) -> Result<input::Batch> {
    let before = (
        state.get_current_theme_id().to_owned(),
        get_effective_opacity_for_rendering(state),
    );
    let batch = input::drain(first, next, |event| {
        match event {
            input::Input::Key(key) => handle_key(key, state, env, flash),
            input::Input::Paste => {
                // Clipboard contents never become shortcuts or a confirmation.
                // Do not inspect or echo the payload, even in feedback.
                *flash = Some(Flash {
                    text: tr("已忽略粘贴 · 用 ↑↓ 选择", "Paste ignored · use ↑↓").into(),
                    until: Instant::now() + Duration::from_millis(1200),
                });
                Ok(KeyOutcome::Continue)
            }
        }
    })?;
    if batch.exit.is_some() {
        return Ok(batch);
    }
    if batch.resized {
        // Forked prompts depend on terminal width. Invalidate before refreshing.
        state.invalidate_prompt_cache();
    }
    if !state.has_selection() {
        return Ok(batch);
    }
    if (batch.redraw || batch.resized) && state.preview_mode_full {
        fork_and_cache_prompt(state, env);
    }
    if before.0 != state.get_current_theme_id()
        || before.1 != get_effective_opacity_for_rendering(state)
    {
        apply(state)?;
    }
    Ok(batch)
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
    if key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        return Ok(KeyOutcome::Cancel);
    }
    if !state.has_selection() {
        return Ok(KeyOutcome::Inert);
    }
    match key.code {
        KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End => {
            let (cols, rows) = terminal::size().unwrap_or((80, 24));
            Ok(
                if super::render::scroll_preview(
                    state,
                    flash.as_ref().map(|flash| flash.text.as_str()),
                    cols,
                    rows,
                    key.code,
                )? {
                    KeyOutcome::Continue
                } else {
                    KeyOutcome::Inert
                },
            )
        }
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
                    text: tr("← 已到不透明", "← Solid (hard stop)").to_string(),
                    until: Instant::now() + Duration::from_millis(500),
                });
            } else if was_guarded {
                *flash = Some(Flash {
                    text: tr(
                        "浅色主题半透明时可能降低文字对比度",
                        "(!) Translucent light themes may reduce text contrast",
                    )
                    .to_string(),
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
                    text: tr("→ 已到透明", "→ Clear (hard stop)").to_string(),
                    until: Instant::now() + Duration::from_millis(500),
                });
            } else if was_guarded {
                *flash = Some(Flash {
                    text: tr(
                        "浅色主题半透明时可能降低文字对比度",
                        "(!) Translucent light themes may reduce text contrast",
                    )
                    .to_string(),
                    until: Instant::now() + Duration::from_millis(1200),
                });
            } else if state.get_current_opacity() == OpacityPreset::Frosted && !is_ghostty() {
                *flash = Some(Flash {
                    text: tr(
                        "此终端近似显示磨砂 · Ghostty 支持完整模糊",
                        "(i) Frosted is approximated here · Ghostty shows full blur",
                    )
                    .to_string(),
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
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Tab => {
            // Tab toggle: flip list-dominant ↔ full-preview mode.
            // Navigation/opacity keys stay live in both modes; Tab is only
            // the layout switch, not a mode lock.
            // The batch coordinator refreshes the prompt once before rendering.
            state.preview_mode_full = !state.preview_mode_full;
            state.preview_scroll = 0;
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Enter => {
            // picker Enter → Selection. SoundSink consumes
            // this via the EventSink seam and runs its priority-fold over
            // the Selection(PickerEnter) event to render the commit SFX.
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

/// Fork the `starship` binary once against the current managed toml +
/// terminal width, and stash the output into `state.prompt_cache` keyed by
/// theme id. A no-op when the managed toml is missing or when the cache is
/// already populated for this theme. Errors are swallowed — `compose_full`
/// degrades to `self_draw_prompt_from_sample_tokens` per silent
/// fallback when `cached_prompt` returns `None`.
fn fork_and_cache_prompt(state: &mut PickerState, env: &SlateEnv) {
    let theme_id = state.get_current_theme_id().to_string();
    if state.prompt_attempted(&theme_id) {
        return;
    }
    // Cache a fallback before any fallible IO. A failed fork/config read should
    // not stall the same theme again on every Tab or opacity change. Resize
    // invalidates successes and failures together, permitting another attempt.
    state.cache_prompt_failure(&theme_id);

    let Ok(config) = crate::config::ConfigManager::with_env(env) else {
        return;
    };
    let Ok(enabled) = config.is_starship_enabled() else {
        return;
    };
    if !enabled {
        state.cache_prompt(&theme_id, disabled_prompt_preview());
        return;
    }

    let Ok(managed_toml) = prepare_preview_starship_config(state, env) else {
        return;
    };
    let Some(managed_dir) = managed_toml.parent() else {
        return;
    };

    let width = terminal::size().map(|(c, _)| c).unwrap_or(80);

    if let Ok(prompt) = fork_starship_prompt(&managed_toml, managed_dir, width, None) {
        state.cache_prompt(&theme_id, prompt);
    }
}

/// Rewrite a picker-only managed starship preview config for the
/// currently-selected theme and return the TOML path the full-preview fork
/// should read.
/// This intentionally avoids `managed/starship/plain.toml`, which may be
/// exported as the live `STARSHIP_CONFIG` for plain-starship shells. Full
/// preview needs a fresh config for the selected row, but mutating the live
/// shell config would leak preview-only themes after Esc/cancel.
fn prepare_preview_starship_config(
    state: &PickerState,
    env: &SlateEnv,
) -> Result<std::path::PathBuf> {
    let theme = state.get_current_theme()?;
    let config = crate::config::ConfigManager::with_env(env)?;
    let content = preview_starship_content(&config, env, &theme)?;
    if content.len() as u64 > crate::config::file_read::MAX_TOOL_CONFIG_BYTES {
        return Err(crate::error::SlateError::InvalidConfig(
            "Starship preview output exceeds 8 MiB".into(),
        ));
    }
    config.write_managed_file("starship", "picker-preview.toml", &content)?;
    Ok(env
        .config_dir()
        .join("managed")
        .join("starship")
        .join("picker-preview.toml"))
}

fn preview_starship_content(
    config: &crate::config::ConfigManager,
    env: &SlateEnv,
    theme: &crate::theme::ThemeVariant,
) -> Result<String> {
    if should_use_plain_starship_preview(config)? {
        return crate::config::prompt::plain_content(env, theme);
    }

    let integration_path = crate::adapter::StarshipAdapter::integration_config_path_with_env(env);
    let Some(content) = crate::config::file_read::read_text(
        &integration_path,
        crate::config::file_read::MAX_TOOL_CONFIG_BYTES,
    )?
    else {
        return crate::config::prompt::plain_content(env, theme);
    };

    crate::adapter::starship::themed_config_from_content(&content, theme).map_err(|_| {
        crate::error::SlateError::InvalidConfig(
            "Cannot prepare prompt preview: invalid Starship configuration".into(),
        )
    })
}

fn should_use_plain_starship_preview(config: &crate::config::ConfigManager) -> Result<bool> {
    if matches!(
        std::env::var("TERM_PROGRAM").as_deref(),
        Ok("Apple_Terminal")
    ) {
        return Ok(true);
    }
    config.should_prefer_plain_starship()
}

fn disabled_prompt_preview() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    format!("{user}\n❯ ")
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
    use std::fs;
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
        let _sink_guard = crate::brand::events::reset_sink_for_tests();
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
        let _sink_guard = crate::brand::events::reset_sink_for_tests();
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

    #[test]
    fn esc_cancels_directly_from_full_preview_mode() {
        let env = dummy_env();
        let mut state = fresh_state();
        state.preview_mode_full = true;
        let mut flash: Option<Flash> = None;

        let outcome = handle_key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut state,
            &env,
            &mut flash,
        )
        .expect("Esc must stay routable in full-preview mode");
        assert!(
            matches!(outcome, KeyOutcome::Cancel),
            "Esc should cancel the picker directly from full-preview mode"
        );
    }

    #[test]
    fn prepare_preview_starship_config_falls_back_to_plain_profile_when_active_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let env = SlateEnv::with_home(tmp.path().to_path_buf());
        let state = fresh_state();

        let plain_toml = env
            .config_dir()
            .join("managed")
            .join("starship")
            .join("plain.toml");
        fs::create_dir_all(plain_toml.parent().expect("parent"))
            .expect("managed starship dir should be creatable");
        fs::write(&plain_toml, "# live shell config\n").expect("seed live shell config");

        let rewritten =
            prepare_preview_starship_config(&state, &env).expect("preview config should rewrite");
        let preview_toml = env
            .config_dir()
            .join("managed")
            .join("starship")
            .join("picker-preview.toml");
        assert_eq!(
            rewritten, preview_toml,
            "preview config helper must target a picker-only managed TOML"
        );

        let expected = crate::config::shell_integration::themed_plain_starship_content(
            &state.get_current_theme().expect("theme should resolve"),
        );
        let actual = fs::read_to_string(&rewritten).expect("preview TOML should exist");
        assert_eq!(
            actual, expected,
            "full-preview should fall back to the plain starship profile when no active user config is available"
        );
        assert_eq!(
            fs::read_to_string(&plain_toml).expect("live plain.toml should remain readable"),
            "# live shell config\n",
            "full-preview must not mutate managed/starship/plain.toml because live shells may read it"
        );
    }

    #[test]
    fn prepare_preview_starship_config_prefers_user_starship_toml_when_available() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let env = SlateEnv::with_home(tmp.path().to_path_buf());
        let state = fresh_state();
        let config = crate::config::ConfigManager::with_env(&env).expect("config manager");
        config
            .set_current_font("JetBrainsMono Nerd Font")
            .expect("nerd font should disable plain-starship fallback");

        let active_toml = env.xdg_config_home().join("starship.toml");
        fs::create_dir_all(active_toml.parent().expect("parent"))
            .expect("xdg config dir should be creatable");
        fs::write(
            &active_toml,
            "format = \"$directory$git_branch$character\"\n[directory]\nstyle = \"bold blue\"\n",
        )
        .expect("seed active starship config");

        let preview_toml =
            prepare_preview_starship_config(&state, &env).expect("preview config should rewrite");
        let expected = crate::adapter::starship::themed_config_from_content(
            &fs::read_to_string(&active_toml).expect("active starship config should stay readable"),
            &state.get_current_theme().expect("theme should resolve"),
        )
        .expect("preview config should reuse the real starship document shape");
        let actual = fs::read_to_string(&preview_toml).expect("preview config should exist");
        assert_eq!(
            actual, expected,
            "full-preview should derive picker-preview.toml from the user's real starship.toml, not the plain fallback template"
        );
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

    #[test]
    fn picker_input_batch_caches_prompt_for_current_theme_in_full_preview() {
        let _sink_guard = crate::brand::events::reset_sink_for_tests();
        let tmp = tempfile::tempdir().expect("tempdir");
        let env = SlateEnv::with_home(tmp.path().to_path_buf());
        let config = crate::config::ConfigManager::with_env(&env).expect("config manager");
        config
            .set_starship_enabled(false)
            .expect("starship flag should be writable");

        let mut state = fresh_state();
        state.preview_mode_full = true;
        let mut next = fresh_state();
        next.move_down();
        let theme_id = next.get_current_theme_id().to_string();
        assert_eq!(
            state.cached_prompt(&theme_id),
            None,
            "newly-selected theme should start as a cache miss"
        );

        let preview = PreviewSnapshot::capture(&env).unwrap();
        let batch = process_input_batch(
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            || Ok(None),
            &mut state,
            &env,
            &mut None,
            |state| {
                preview.apply(
                    state.get_current_theme_id(),
                    get_effective_opacity_for_rendering(state),
                )
            },
        )
        .unwrap();
        assert!(batch.redraw);
        assert_eq!(state.get_current_theme_id(), theme_id);
        preview.restore().unwrap();

        assert_eq!(
            state.cached_prompt(&theme_id),
            Some(disabled_prompt_preview().as_str()),
            "full-preview state sync must populate the prompt cache for the current theme"
        );
    }
}
