//! Event loop and rendering for the interactive crossterm picker.
//! Per , and 06-CONTEXT research on crossterm + live preview.

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
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_down();
            Ok(KeyOutcome::Continue)
        }
        KeyCode::Left | KeyCode::Char('h') => {
            let terminal = crate::detection::TerminalProfile::detect();
            if !terminal.supports_opacity() {
                *flash = Some(Flash {
                    text: format!("{} does not support opacity", terminal.display_name()),
                    until: Instant::now() + Duration::from_millis(1200),
                });
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
            let terminal = crate::detection::TerminalProfile::detect();
            if !terminal.supports_opacity() {
                *flash = Some(Flash {
                    text: format!("{} does not support opacity", terminal.display_name()),
                    until: Instant::now() + Duration::from_millis(1200),
                });
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
        KeyCode::Enter => Ok(KeyOutcome::Commit),
        KeyCode::Esc | KeyCode::Char('q') => Ok(KeyOutcome::Cancel),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Ok(KeyOutcome::Cancel)
        }
        _ => Ok(KeyOutcome::Inert),
    }
}
