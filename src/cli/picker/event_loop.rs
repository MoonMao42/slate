//! Event loop and rendering for the interactive crossterm picker.
//! Per , and 06-CONTEXT research on crossterm + live preview.

use crate::cli::auto_theme;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::{ThemeAppearance, ThemeRegistry};
use std::env;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
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
/// Enters alternate screen, sets up raw mode, manages crossterm event loop.
/// Returns Ok if user commits (Enter), or rolls back cleanly on ESC/Ctrl+C.
pub fn launch_picker(env: &SlateEnv) -> Result<()> {
    let config = ConfigManager::with_env(env)?;

    // Resolve starting theme/opacity from current persisted state
    let starting_theme_id = config
        .get_current_theme()?
        .unwrap_or_else(|| "catppuccin-mocha".to_string());
    let starting_opacity = config
        .get_current_opacity_preset()
        .unwrap_or(OpacityPreset::Solid);

    let mut state = PickerState::new(&starting_theme_id, starting_opacity)?;

    // Guard ensures terminal state is restored even on panic.
    let _guard = TerminalGuard::enter()?;

    // Paint the user's starting selection on entry so the preview matches the
    // cursor before any keystrokes. Best-effort — preview failures do not
    // abort the picker.
    let effective = get_effective_opacity_for_rendering(&state);
    let _ = crate::cli::set::silent_preview_apply(env, state.get_current_theme_id(), effective);

    let exit_action = event_loop(env, &mut state)?;

    // Drop the guard explicitly to restore the terminal before we print anything
    // visible to the user (Afterglow receipt or nothing).
    drop(_guard);

    match exit_action {
        ExitAction::Commit => {
            state.commit();
            let theme_id = state.get_current_theme_id().to_string();
            let opacity = get_effective_opacity_for_rendering(&state);
            crate::cli::set::silent_commit_apply(env, &theme_id, opacity)?;
            render_afterglow_receipt(&state, env)?;
        }
        ExitAction::Cancel => {
            // Re-apply the user's original selection to undo any preview-side
            // adapter writes. Best-effort — do not propagate rollback errors.
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
    let mut dirty = true; // Paint on first iteration

    loop {
        if dirty {
            render(state, flash.as_ref())?;
            dirty = false;
        }

        // Auto-expire flashes.
        if let Some(f) = &flash {
            if Instant::now() >= f.until {
                flash = None;
                dirty = true;
            }
        }

        // Poll with a short timeout so flashes can expire and repaint.
        if !event::poll(Duration::from_millis(150))
            .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?
        {
            continue;
        }

        // Read the first event, then drain any queued events to skip to the
        // latest input. This prevents "sliding" when keys are held down.
        let first = event::read().map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?;
        let mut last_key_event = match &first {
            Event::Key(k) => Some(*k),
            _ => None,
        };
        let mut had_resize = matches!(&first, Event::Resize(_, _));

        // Drain remaining queued events with zero-timeout poll
        while event::poll(Duration::ZERO)
            .map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))?
        {
            match event::read().map_err(|e| crate::error::SlateError::IOError(io::Error::other(e)))? {
                Event::Key(k) => {
                    // For navigation keys, keep only the latest one
                    // For action keys (Enter/Esc/Ctrl+C), process immediately
                    match k.code {
                        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                            last_key_event = Some(k);
                            break; // Don't skip action keys
                        }
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                            last_key_event = Some(k);
                            break;
                        }
                        _ => { last_key_event = Some(k); }
                    }
                }
                Event::Resize(_, _) => { had_resize = true; }
                _ => {}
            }
        }

        // Process the final key event
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
    Continue, // Re-apply preview after navigation
    Inert,    // Key handled without changing preview
    Commit,   // Exit with commit
    Cancel,   // Exit without commit
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
            // Explicit opacity navigation unlocks the light-theme guardrail (D-26b).
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
            quick_save_auto(state, env, flash)?;
            Ok(KeyOutcome::Inert)
        }
        KeyCode::Char('r') => {
            quick_resume_auto(state, env, flash)?;
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

/// Write the current theme to auto.toml under its appearance slot without
/// leaving the picker. Shows a flash receipt — no cliclack confirm, because
/// cliclack's internal prompt writer would collide with our raw-mode
/// alternate screen.
fn quick_save_auto(state: &PickerState, env: &SlateEnv, flash: &mut Option<Flash>) -> Result<()> {
    let config = ConfigManager::with_env(env)?;
    let theme = state.get_current_theme()?;
    let theme_id = state.get_current_theme_id();

    let msg = match theme.appearance {
        ThemeAppearance::Dark => {
            config.write_auto_config(Some(theme_id), None)?;
            format!("✓ Auto Dark saved: {}", theme.name)
        }
        ThemeAppearance::Light => {
            config.write_auto_config(None, Some(theme_id))?;
            format!("✓ Auto Light saved: {}", theme.name)
        }
    };

    *flash = Some(Flash {
        text: msg,
        until: Instant::now() + Duration::from_millis(1200),
    });
    Ok(())
}

/// Jump the cursor to the theme that pipeline resolves for the current
/// system appearance.
fn quick_resume_auto(
    state: &mut PickerState,
    env: &SlateEnv,
    flash: &mut Option<Flash>,
) -> Result<()> {
    let config = ConfigManager::with_env(env)?;
    let auto_theme_id = match auto_theme::resolve_auto_theme(env, &config) {
        Ok(id) => id,
        Err(e) => {
            *flash = Some(Flash {
                text: format!("(!) Resume auto failed: {}", e),
                until: Instant::now() + Duration::from_millis(1500),
            });
            return Ok(());
        }
    };

    if let Some(idx) = state.theme_ids().iter().position(|id| id == &auto_theme_id) {
        state.jump_to_theme(idx);
        let appearance = auto_theme::detect_system_appearance()
            .map(|a| match a {
                ThemeAppearance::Dark => "dark",
                ThemeAppearance::Light => "light",
            })
            .unwrap_or("?");
        *flash = Some(Flash {
            text: format!("→ resumed auto ({}): {}", appearance, auto_theme_id),
            until: Instant::now() + Duration::from_millis(1200),
        });
    }
    Ok(())
}

fn render(state: &PickerState, flash: Option<&Flash>) -> Result<()> {
    let mut stdout = io::stdout();
    queue_io(queue!(stdout, Clear(ClearType::All), MoveTo(0, 0)))?;

    // Header
    queue_io(queue!(
        stdout,
        Print("\r\n  "),
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        Print(Symbols::BRAND),
        Print("  slate set"),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print("   theme + opacity picker\r\n\r\n"),
    ))?;

    // Scroll window — reserve lines for chrome (header=3, counter=2, opacity=3, help=3, padding=2)
    let (_cols, rows) = terminal::size().map_err(io_err)?;
    let total_rows = rows as usize;
    // Preview takes ~4 lines (Normal + Bright + Extras + blank); only show if enough room
    let show_preview = total_rows > 20;
    let chrome_lines: usize = if show_preview { 16 } else { 11 };
    let max_visible = total_rows.saturating_sub(chrome_lines).max(3);
    let total = state.theme_ids().len();
    let cursor = state.selected_theme_index();
    let visible = max_visible.min(total);
    let half = visible / 2;
    let mut start = cursor.saturating_sub(half);
    if start + visible > total {
        start = total.saturating_sub(visible);
    }
    let end = (start + visible).min(total);

    let registry = ThemeRegistry::new()?;
    for idx in start..end {
        let id = &state.theme_ids()[idx];
        let theme = registry.get(id);
        let is_sel = idx == cursor;

        if is_sel {
            queue_io(queue!(
                stdout,
                SetForegroundColor(Color::Cyan),
                Print("  › "),
                ResetColor,
            ))?;
        } else {
            queue_io(queue!(stdout, Print("    ")))?;
        }

        match theme {
            Some(t) => {
                if is_sel {
                    queue_io(queue!(
                        stdout,
                        SetForegroundColor(Color::White),
                        SetAttribute(Attribute::Bold),
                        Print(format!("{:20}", t.name)),
                        SetAttribute(Attribute::Reset),
                        SetForegroundColor(Color::DarkGrey),
                        Print(format!(" {}", t.family)),
                        ResetColor,
                    ))?;
                } else {
                    queue_io(queue!(
                        stdout,
                        SetForegroundColor(Color::Grey),
                        Print(format!("{:20}", t.name)),
                        SetForegroundColor(Color::DarkGrey),
                        Print(format!(" {}", t.family)),
                        ResetColor,
                    ))?;
                }
            }
            None => {
                queue_io(queue!(stdout, Print(id.as_str())))?;
            }
        }

        queue_io(queue!(stdout, Print("\r\n")))?;
    }

    // Scroll hint
    queue_io(queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print(format!("\r\n  {}/{}\r\n", cursor + 1, total)),
        ResetColor,
    ))?;

    // ANSI color preview — only show when terminal has enough vertical space
    let current_theme = state.get_current_theme()?;
    if show_preview {
        let preview_raw = super::preview_panel::render_preview(&current_theme.palette);
        let preview_output = preview_raw.replace('\n', "\r\n  ");
        queue_io(queue!(stdout, Print("  ")))?;
        queue_io(queue!(stdout, Print(preview_output)))?;
        queue_io(queue!(stdout, Print("\r\n")))?;
    }

    // Opacity indicator — apply guardrail to the rendered selection
    let effective = get_effective_opacity_for_rendering(state);
    queue_io(queue!(stdout, Print("\r\n  Opacity:  ")))?;
    render_opacity_slot(&mut stdout, OpacityPreset::Solid, effective)?;
    queue_io(queue!(stdout, Print("    ")))?;
    render_opacity_slot(&mut stdout, OpacityPreset::Frosted, effective)?;
    queue_io(queue!(stdout, Print("    ")))?;
    render_opacity_slot(&mut stdout, OpacityPreset::Clear, effective)?;
    queue_io(queue!(stdout, Print("\r\n\r\n")))?;

    // Help bar
    queue_io(queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("  ↑↓/jk theme · ←→/hl opacity · Enter save · Esc cancel\r\n"),
        Print("  s save-auto · r resume-auto\r\n"),
        ResetColor,
    ))?;

    // Flash
    if let Some(f) = flash {
        queue_io(queue!(
            stdout,
            Print("\r\n  "),
            SetForegroundColor(Color::Magenta),
            Print(&f.text),
            ResetColor,
            Print("\r\n"),
        ))?;
    }

    stdout.flush().map_err(crate::error::SlateError::IOError)?;
    Ok(())
}

fn render_opacity_slot(
    stdout: &mut io::Stdout,
    slot: OpacityPreset,
    effective: OpacityPreset,
) -> Result<()> {
    let is_active = slot == effective;
    let label = opacity_to_label(slot);
    let dot = if is_active { "●" } else { "○" };

    if is_active {
        queue_io(queue!(
            stdout,
            SetForegroundColor(Color::Cyan),
            Print("< "),
            SetAttribute(Attribute::Bold),
            Print(format!("{} {}", dot, label)),
            SetAttribute(Attribute::Reset),
            Print(" >"),
            ResetColor,
        ))?;
    } else {
        queue_io(queue!(
            stdout,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("  {} {}  ", dot, label)),
            ResetColor,
        ))?;
    }
    Ok(())
}

fn queue_io<T>(result: std::result::Result<T, io::Error>) -> Result<()> {
    result
        .map(|_| ())
        .map_err(crate::error::SlateError::IOError)
}

fn io_err(e: io::Error) -> crate::error::SlateError {
    crate::error::SlateError::IOError(e)
}

/// Check if light theme opacity guardrail should apply
/// Per D-26b and Task 5:
/// - Returns true if current theme is Light AND user has not yet overridden opacity
/// - When true, rendering should force effective opacity to Solid
/// - Help bar should show hint about navigating ←→ to unlock opacity
/// - When user presses ←→ on light theme, call state.set_opacity_override(true)
pub fn should_guard_light_theme_opacity(state: &super::state::PickerState) -> bool {
    // Check if current theme is light and override not yet set
    if state.opacity_overridden() {
        return false; // Already overridden, no guardrail
    }

    // Get current theme and check if it's light
    if let Ok(theme) = state.get_current_theme() {
        theme.appearance == ThemeAppearance::Light
    } else {
        false
    }
}

/// Get the effective opacity for rendering, applying light-theme guardrail if needed
/// Per D-26b:
/// - If should_guard_light_theme_opacity() returns true, return Solid regardless of user selection
/// - Otherwise return the user's actual selected opacity
pub fn get_effective_opacity_for_rendering(state: &super::state::PickerState) -> OpacityPreset {
    if should_guard_light_theme_opacity(state) {
        OpacityPreset::Solid
    } else {
        state.get_current_opacity()
    }
}

/// Detect if the current terminal is Ghostty
/// Per D-24b: Check $TERM_PROGRAM (case-insensitive, Ghostty may report "ghostty" or "Ghostty")
fn is_ghostty() -> bool {
    env::var("TERM_PROGRAM")
        .map(|prog| prog.eq_ignore_ascii_case("ghostty"))
        .unwrap_or(false)
}

/// Format an opacity preset as a user-friendly string
fn opacity_to_label(opacity: OpacityPreset) -> &'static str {
    match opacity {
        OpacityPreset::Solid => "Solid",
        OpacityPreset::Frosted => "Frosted",
        OpacityPreset::Clear => "Clear",
    }
}

/// Parse hex color string (#RRGGBB) into RGB tuple
/// Returns (r, g, b) where each is 0-255, or None if invalid
fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some((r, g, b))
}

/// Render Afterglow receipt with atomic flush
/// Per D-17b and Task 7:
/// - Called after picker commit (Enter pressed)
/// - Constructs receipt panel with theme and opacity info
/// - Extracts colors from new theme's palette (use foreground for text)
/// - Pre-assembles entire output to String with ANSI codes
/// - Single atomic write + flush (no per-line writes)
/// - Only renders on committed Enter path; ESC/q/Ctrl+C skip entirely
pub fn render_afterglow_receipt(state: &super::state::PickerState, _env: &SlateEnv) -> Result<()> {
    let current_theme = state.get_current_theme()?;
    let current_opacity = state.get_current_opacity();

    // Extract colors from theme palette
    // Per D-17b: Use new theme's primary text color (foreground)
    let text_color_hex = &current_theme.palette.foreground;

    // Parse hex color to RGB
    let text_rgb = parse_hex_color(text_color_hex);

    // Format Afterglow panel per D-17b spec
    let mut output = String::new();

    // ANSI codes for screen restoration (move to alternate → normal screen)
    // Per Task 7: LeaveAlternateScreen + Show Cursor
    output.push_str("\x1b[?1049l"); // Leave alternate screen
    output.push_str("\x1b[?25h"); // Show cursor

    // Output newline to separate from alternate screen
    output.push('\n');

    // Build receipt lines with theme colors
    // Theme line: ✦ Theme {theme_name}
    let theme_line = format!("  {}  Theme     {}\n", Symbols::BRAND, current_theme.name);

    // Opacity line: ◆ Opacity {opacity_label}
    let opacity_line = format!(
        "  {}  Opacity   {}\n",
        Symbols::DIAMOND,
        opacity_to_label(current_opacity)
    );

    // Apply theme colors to the output
    // Use ANSI 24-bit RGB (38;2;R;G;B) from palette
    if let Some((r, g, b)) = text_rgb {
        let text_color = format!("\x1b[38;2;{};{};{}m", r, g, b);
        let reset_color = "\x1b[0m";

        // Construct colored receipt
        output.push_str(&text_color);
        output.push_str(&theme_line);
        output.push_str(&opacity_line);
        output.push_str(reset_color);
    } else {
        // Fallback if color parsing fails (no color codes)
        output.push_str(&theme_line);
        output.push_str(&opacity_line);
    }

    // Atomic write to stdout
    let mut stdout = io::stdout();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;

    Ok(())
}
