use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::{ThemeAppearance, ThemeRegistry};
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::env;
use std::io::{self, Write};

use super::state::PickerState;

pub(super) fn render(state: &PickerState, flash_text: Option<&str>) -> Result<()> {
    let mut stdout = io::stdout();
    queue_io(queue!(stdout, Clear(ClearType::All), MoveTo(0, 0)))?;

    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    // Picker chrome header: "✦ slate  theme + opacity picker".
    // `r.logo()` carries the brand-lavender ✦ glyph + `slate` wordmark;
    // `r.path()` dims the descriptor so the eye lands on the wordmark first.
    let logo = roles
        .as_ref()
        .map(|r| r.logo())
        .unwrap_or_else(|| "✦ slate".to_string());
    let tagline = roles
        .as_ref()
        .map(|r| r.path("theme + opacity picker"))
        .unwrap_or_else(|| "theme + opacity picker".to_string());
    queue_io(queue!(
        stdout,
        Print("\r\n  "),
        Print(&logo),
        Print("   "),
        Print(&tagline),
        Print("\r\n\r\n"),
    ))?;

    let (_cols, rows) = terminal::size().map_err(io_err)?;
    let total_rows = rows as usize;
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
        let is_selected = idx == cursor;

        if is_selected {
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
            Some(theme) => {
                if is_selected {
                    queue_io(queue!(
                        stdout,
                        SetForegroundColor(Color::White),
                        SetAttribute(Attribute::Bold),
                        Print(format!("{:20}", theme.name)),
                        SetAttribute(Attribute::Reset),
                        SetForegroundColor(Color::DarkGrey),
                        Print(format!(" {}", theme.family)),
                        ResetColor,
                    ))?;
                } else {
                    queue_io(queue!(
                        stdout,
                        SetForegroundColor(Color::Grey),
                        Print(format!("{:20}", theme.name)),
                        SetForegroundColor(Color::DarkGrey),
                        Print(format!(" {}", theme.family)),
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

    queue_io(queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print(format!("\r\n  {}/{}\r\n", cursor + 1, total)),
        ResetColor,
    ))?;

    let current_theme = state.get_current_theme()?;
    if show_preview {
        let preview_raw = super::preview_panel::render_preview(&current_theme.palette);
        let preview_output = preview_raw.replace('\n', "\r\n  ");
        queue_io(queue!(stdout, Print("  ")))?;
        queue_io(queue!(stdout, Print(preview_output)))?;
        queue_io(queue!(stdout, Print("\r\n")))?;
    }

    let supports_opacity = crate::detection::TerminalProfile::detect().supports_opacity();
    if supports_opacity {
        let effective = get_effective_opacity_for_rendering(state);
        queue_io(queue!(stdout, Print("\r\n  Opacity:  ")))?;
        render_opacity_slot(&mut stdout, OpacityPreset::Solid, effective)?;
        queue_io(queue!(stdout, Print("    ")))?;
        render_opacity_slot(&mut stdout, OpacityPreset::Frosted, effective)?;
        queue_io(queue!(stdout, Print("    ")))?;
        render_opacity_slot(&mut stdout, OpacityPreset::Clear, effective)?;
    }
    queue_io(queue!(stdout, Print("\r\n\r\n")))?;

    let help_body = if supports_opacity {
        "↑↓/jk theme · ←→/hl opacity · Enter save · Esc cancel"
    } else {
        "↑↓/jk theme · Enter save · Esc cancel"
    };
    let help_line = roles
        .as_ref()
        .map(|r| r.path(help_body))
        .unwrap_or_else(|| help_body.to_string());
    let save_line = roles
        .as_ref()
        .map(|r| r.path("s save-auto · r resume-auto"))
        .unwrap_or_else(|| "s save-auto · r resume-auto".to_string());
    queue_io(queue!(
        stdout,
        Print("  "),
        Print(&help_line),
        Print("\r\n  "),
        Print(&save_line),
        Print("\r\n"),
    ))?;

    if let Some(text) = flash_text {
        queue_io(queue!(
            stdout,
            Print("\r\n  "),
            SetForegroundColor(Color::Magenta),
            Print(text),
            ResetColor,
            Print("\r\n"),
        ))?;
    }

    stdout.flush().map_err(crate::error::SlateError::IOError)?;
    Ok(())
}

pub(super) fn should_guard_light_theme_opacity(state: &PickerState) -> bool {
    if state.opacity_overridden() {
        return false;
    }

    if let Ok(theme) = state.get_current_theme() {
        theme.appearance == ThemeAppearance::Light
    } else {
        false
    }
}

pub(super) fn get_effective_opacity_for_rendering(state: &PickerState) -> OpacityPreset {
    if should_guard_light_theme_opacity(state) {
        OpacityPreset::Solid
    } else {
        state.get_current_opacity()
    }
}

pub(super) fn is_ghostty() -> bool {
    env::var("TERM_PROGRAM")
        .map(|program| program.eq_ignore_ascii_case("ghostty"))
        .unwrap_or(false)
}

// SWATCH-RENDERER: intentionally raw ANSI. `render_afterglow_receipt`
// renders the active theme's foreground color directly onto the receipt
// lines so the user immediately sees the theme they just committed. The
// alt-screen-leave + cursor-restore sequences at the top are terminal
// control, not styling, and the `38;2;R;G;B` fg is a palette swatch that
// MUST carry the theme's hex for the receipt to land. Chrome glyphs +
// labels inside this fn flow through the Roles API (brand/heading/path),
// wrapped by the swatch fg so everything inherits the theme tint.
pub(super) fn render_afterglow_receipt(state: &PickerState, _env: &SlateEnv) -> Result<()> {
    let current_theme = state.get_current_theme()?;
    let current_opacity = state.get_current_opacity();
    let text_rgb = parse_hex_color(&current_theme.palette.foreground);

    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    let mut output = String::new();
    output.push_str("\x1b[?1049l");
    output.push_str("\x1b[?25h");
    output.push('\n');

    let brand_glyph = roles
        .as_ref()
        .map(|r| r.brand("✦"))
        .unwrap_or_else(|| "✦".to_string());
    let diamond_glyph = roles
        .as_ref()
        .map(|r| r.heading("").trim_end().to_string())
        .unwrap_or_else(|| "◆".to_string());
    let theme_label = roles
        .as_ref()
        .map(|r| r.path("Theme"))
        .unwrap_or_else(|| "Theme".to_string());
    let opacity_label = roles
        .as_ref()
        .map(|r| r.path("Opacity"))
        .unwrap_or_else(|| "Opacity".to_string());
    let theme_name = roles
        .as_ref()
        .map(|r| r.theme_name(&current_theme.name))
        .unwrap_or_else(|| current_theme.name.clone());

    let theme_line = format!("  {}  {}     {}\n", brand_glyph, theme_label, theme_name);
    let show_opacity = crate::detection::TerminalProfile::detect().supports_opacity();
    let opacity_line = if show_opacity {
        format!(
            "  {}  {}   {}\n",
            diamond_glyph,
            opacity_label,
            opacity_to_label(current_opacity)
        )
    } else {
        String::new()
    };

    if let Some((r, g, b)) = text_rgb {
        let text_color = format!("\x1b[38;2;{};{};{}m", r, g, b);
        output.push_str(&text_color);
        output.push_str(&theme_line);
        output.push_str(&opacity_line);
        output.push_str("\x1b[0m");
    } else {
        output.push_str(&theme_line);
        output.push_str(&opacity_line);
    }

    let mut stdout = io::stdout();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;
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

fn io_err(error: io::Error) -> crate::error::SlateError {
    crate::error::SlateError::IOError(error)
}

fn opacity_to_label(opacity: OpacityPreset) -> &'static str {
    match opacity {
        OpacityPreset::Solid => "Solid",
        OpacityPreset::Frosted => "Frosted",
        OpacityPreset::Clear => "Clear",
    }
}

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
