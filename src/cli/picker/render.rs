use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::{ThemeAppearance, ThemeRegistry, ThemeVariant};
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::env;
use std::io::{self, Write};

use super::preview::compose;
use super::state::PickerState;

/// Public entry: writes to stdout. Reads terminal::size for layout.
///
/// Phase 19 D-12: dispatches on `state.preview_mode_full` between the
/// list-dominant layout (default) and the full-screen preview layout.
pub(super) fn render(state: &PickerState, flash_text: Option<&str>) -> Result<()> {
    let (cols, rows) = terminal::size().map_err(io_err)?;
    let mut stdout = io::stdout();
    render_into(&mut stdout, state, flash_text, cols, rows)?;
    stdout.flush().map_err(crate::error::SlateError::IOError)?;
    Ok(())
}

/// Core renderer — writable-target-agnostic so tests can feed `Vec<u8>`.
///
/// Mode-dispatches on `state.preview_mode_full`:
/// * `false` → [`render_list_dominant`] (Phase 19 D-08/D-09/D-14 — family
///   section headers + full-width lavender pill cursor + mute description +
///   opacity strip + help line).
/// * `true`  → [`render_full_preview`] (Phase 19 D-13 — responsive fold
///   preview via [`compose::compose_full`]).
pub(super) fn render_into<W: io::Write>(
    out: &mut W,
    state: &PickerState,
    flash_text: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<()> {
    if state.preview_mode_full {
        render_full_preview(out, state, flash_text, cols, rows)
    } else {
        render_list_dominant(out, state, flash_text, cols, rows)
    }
}

/// List-dominant layout (D-12 default). Existing Phase 15–18 chrome +
/// Phase 19 D-08 family headers + D-14 full-width pill cursor + D-09
/// opacity strip.
fn render_list_dominant<W: io::Write>(
    out: &mut W,
    state: &PickerState,
    flash_text: Option<&str>,
    _cols: u16,
    rows: u16,
) -> Result<()> {
    queue_io(queue!(out, Clear(ClearType::All), MoveTo(0, 0)))?;

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
        out,
        Print("\r\n  "),
        Print(&logo),
        Print("   "),
        Print(&tagline),
        Print("\r\n\r\n"),
    ))?;

    // Re-read terminal width for full-width pill calculation.
    let (cols, _) = terminal::size().map_err(io_err).unwrap_or((80, rows));

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
    let mut last_family: Option<String> = None;
    for idx in start..end {
        let id = &state.theme_ids()[idx];
        let Some(theme) = registry.get(id) else {
            // Registry miss: render the id as a fallback row (preserves
            // the original behavior from the pre-Phase-19 loop).
            queue_io(queue!(
                out,
                Print("    "),
                Print(id.as_str()),
                Print("\r\n")
            ))?;
            continue;
        };

        // D-08: family section header is a render-time band. Emitted whenever
        // the variant's family differs from the previous row's family. Never
        // appears in `state.theme_ids()` (see `family_headers_are_not_in_theme_ids`
        // invariant in picker::state::tests).
        if last_family.as_deref() != Some(theme.family.as_str()) {
            queue_family_heading(out, roles.as_ref(), &theme.family)?;
            last_family = Some(theme.family.clone());
        }

        let is_selected = idx == cursor;
        queue_variant_row(out, theme, is_selected, cols, roles.as_ref())?;
    }

    queue_io(queue!(
        out,
        SetForegroundColor(Color::DarkGrey),
        Print(format!("\r\n  {}/{}\r\n", cursor + 1, total)),
        ResetColor,
    ))?;

    let current_theme = state.get_current_theme()?;
    if show_preview {
        let preview_raw = super::preview_panel::render_preview(&current_theme.palette);
        let preview_output = preview_raw.replace('\n', "\r\n  ");
        queue_io(queue!(out, Print("  ")))?;
        queue_io(queue!(out, Print(preview_output)))?;
        queue_io(queue!(out, Print("\r\n")))?;
    }

    let supports_opacity = crate::detection::TerminalProfile::detect().supports_opacity();
    if supports_opacity {
        let effective = get_effective_opacity_for_rendering(state);
        queue_io(queue!(out, Print("\r\n  Opacity:  ")))?;
        render_opacity_slot(out, OpacityPreset::Solid, effective)?;
        queue_io(queue!(out, Print("    ")))?;
        render_opacity_slot(out, OpacityPreset::Frosted, effective)?;
        queue_io(queue!(out, Print("    ")))?;
        render_opacity_slot(out, OpacityPreset::Clear, effective)?;
    }
    queue_io(queue!(out, Print("\r\n\r\n")))?;

    let help_body = if supports_opacity {
        "↑↓/jk theme · ←→/hl opacity · Tab preview · Enter save · Esc cancel"
    } else {
        "↑↓/jk theme · Tab preview · Enter save · Esc cancel"
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
        out,
        Print("  "),
        Print(&help_line),
        Print("\r\n  "),
        Print(&save_line),
        Print("\r\n"),
    ))?;

    if let Some(text) = flash_text {
        queue_io(queue!(
            out,
            Print("\r\n  "),
            SetForegroundColor(Color::Magenta),
            Print(text),
            ResetColor,
            Print("\r\n"),
        ))?;
    }

    Ok(())
}

/// Full-screen preview layout (D-13). Delegates body construction to
/// [`compose::compose_full`] — the composer picks the responsive fold tier
/// (4/6/8 blocks) from terminal rows and stacks them with `◆ Heading`
/// labels (see Plan 19-04). Opacity strip + help-line chrome is
/// intentionally hidden here (D-09 stays in list-dominant only).
///
/// `prompt_line_override` is passed as `None` at this layer — Plan 19-07
/// event_loop glue will inject a real forked starship prompt; the renderer
/// itself stays fork-agnostic.
fn render_full_preview<W: io::Write>(
    out: &mut W,
    state: &PickerState,
    flash_text: Option<&str>,
    _cols: u16,
    rows: u16,
) -> Result<()> {
    queue_io(queue!(out, Clear(ClearType::All), MoveTo(0, 0)))?;

    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    // Minimal chrome: slate logo + "preview · Tab to return" breadcrumb.
    let logo = roles
        .as_ref()
        .map(|r| r.logo())
        .unwrap_or_else(|| "✦ slate".to_string());
    let breadcrumb = roles
        .as_ref()
        .map(|r| r.path("preview · Tab to return"))
        .unwrap_or_else(|| "preview · Tab to return".to_string());
    queue_io(queue!(
        out,
        Print("\r\n  "),
        Print(&logo),
        Print("   "),
        Print(&breadcrumb),
        Print("\r\n\r\n"),
    ))?;

    let current_theme = state.get_current_theme()?;
    let tier = compose::decide_fold_tier(rows);
    // Plan 19-07 event_loop glue may swap `None` for the forked starship
    // prompt. Compose self-draws when None.
    let body = compose::compose_full(&current_theme.palette, tier, roles.as_ref(), None);
    // Prepend 2-space indent to every line so alt-screen layout matches
    // the list-dominant indent width.
    let indented = body.replace('\n', "\r\n  ");
    queue_io(queue!(out, Print("  "), Print(indented), Print("\r\n")))?;

    if let Some(text) = flash_text {
        let mute = roles
            .as_ref()
            .map(|r| r.path(text))
            .unwrap_or_else(|| text.to_string());
        queue_io(queue!(out, Print("\r\n  "), Print(&mute), Print("\r\n")))?;
    }
    Ok(())
}

/// Emit a single `◆ FamilyName` section header band (D-08).
///
/// Outputs 2-space indent + `Roles::heading(family)` + `\r\n`. Degrades to
/// plain `◆ family` when the registry is unavailable.
fn queue_family_heading<W: io::Write>(
    out: &mut W,
    roles: Option<&Roles<'_>>,
    family: &str,
) -> Result<()> {
    let heading = match roles {
        Some(r) => r.heading(family),
        None => format!("◆ {}", family),
    };
    queue_io(queue!(out, Print("  "), Print(&heading), Print("\r\n")))?;
    Ok(())
}

/// Emit a single variant row in list-dominant mode (D-14).
///
/// Selected row: 2-space indent OUTSIDE the pill, then a full-width
/// lavender pill via `Roles::command(padded_body)` (pill width =
/// `cols - 2`).
///
/// Non-selected row: 4-space indent + dim `Roles::theme_name(name)` +
/// 2 spaces + dim/mute `Roles::path(desc)` — no pill.
fn queue_variant_row<W: io::Write>(
    out: &mut W,
    theme: &ThemeVariant,
    is_selected: bool,
    cols: u16,
    roles: Option<&Roles<'_>>,
) -> Result<()> {
    let desc = crate::theme::get_theme_description(&theme.id).unwrap_or("");
    // Width budget for selected-row pill body: terminal cols minus the
    // 2-space indent that sits OUTSIDE the pill. Saturating_sub guards
    // against pathologically narrow terminals.
    let width = (cols as usize).saturating_sub(2);

    if is_selected {
        // Selected row body: "› {name:20}  {desc}" padded out to the full
        // pill width. `Roles::command` wraps this in the D-04 alpha pill
        // so the line reads as a single lavender band.
        let body = format!("› {:<20}  {}", theme.name, desc);
        let padded = format!("{:<width$}", body, width = width);
        let pill = match roles {
            Some(r) => r.command(&padded),
            None => padded,
        };
        queue_io(queue!(out, Print("  "), Print(&pill), Print("\r\n")))?;
    } else {
        // Non-selected row: dim theme name in `brand_accent` tint +
        // role-path (mute/italic) description, no pill.
        let name_text = match roles {
            Some(r) => r.theme_name(&format!("{:<20}", theme.name)),
            None => format!("{:<20}", theme.name),
        };
        let desc_text = match roles {
            Some(r) => r.path(desc),
            None => desc.to_string(),
        };
        queue_io(queue!(
            out,
            Print("    "),
            Print(&name_text),
            Print("  "),
            Print(&desc_text),
            Print("\r\n"),
        ))?;
    }
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

fn render_opacity_slot<W: io::Write>(
    out: &mut W,
    slot: OpacityPreset,
    effective: OpacityPreset,
) -> Result<()> {
    let is_active = slot == effective;
    let label = opacity_to_label(slot);
    let dot = if is_active { "●" } else { "○" };

    if is_active {
        queue_io(queue!(
            out,
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
            out,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Render against an in-memory buffer so tests can assert on queued
    /// bytes without touching stdout / terminal::size.
    fn render_to_vec(state: &PickerState, cols: u16, rows: u16) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::<u8>::new());
        render_into(&mut buf, state, None, cols, rows).expect("render_into must succeed");
        buf.into_inner()
    }

    /// Simple ANSI CSI stripper used exclusively by these tests. Removes
    /// ESC `[` ... `m` SGR sequences so assertions can focus on visible
    /// text. (Docstring avoids the raw escape-sequence literal so the
    /// Phase 18 `no_raw_styling_ansi_anywhere_in_user_surfaces` aggregate
    /// scan stays clean.)
    fn strip_ansi(bytes: &[u8]) -> String {
        let s = String::from_utf8_lossy(bytes);
        let mut out = String::new();
        let mut iter = s.chars().peekable();
        while let Some(c) = iter.next() {
            if c == '\x1b' && iter.peek() == Some(&'[') {
                iter.next();
                for nc in iter.by_ref() {
                    if nc.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }

    /// D-08: family-header bands are render-time decoration; `theme_ids`
    /// must not carry them, but the rendered output for a Catppuccin
    /// starting cursor MUST include `◆ Catppuccin`.
    #[test]
    fn family_headers_are_render_time_only() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        for id in state.theme_ids() {
            assert!(
                !id.starts_with("◆"),
                "theme_ids must not contain ◆-prefixed band rows; found: {id}"
            );
        }
        let out = render_to_vec(&state, 80, 24);
        let visible = strip_ansi(&out);
        assert!(
            visible.contains("◆ Catppuccin"),
            "expected family heading band in render output, got:\n{visible}"
        );
    }

    /// D-14: the selected-row pill body must span roughly the full
    /// terminal width (indent of 2 cols sits outside the pill).
    #[test]
    fn pill_cursor_padded_to_terminal_width() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let cols: u16 = 80;
        let out = render_to_vec(&state, cols, 24);
        let visible = strip_ansi(&out);
        let selected_line = visible
            .lines()
            .find(|line| line.trim_start().starts_with("›"))
            .expect("selected row with › prefix should be present");
        // `Roles::command` wraps the padded body with one leading + one
        // trailing space; we accept anything at or above `cols - 4` to be
        // tolerant of the pill wrapper bytes.
        let body = selected_line.trim_start_matches(' ');
        let width_body = body.chars().count();
        assert!(
            width_body + 4 >= cols as usize,
            "pill body shorter than cols-4; got {width_body} of expected {}",
            cols - 4
        );
    }

    /// D-08: non-selected rows surface their `get_theme_description` text.
    #[test]
    fn non_selected_row_shows_description() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let out = render_to_vec(&state, 80, 24);
        let visible = strip_ansi(&out);
        // `catppuccin-frappe` is a sibling variant in the same family and
        // is expected to be in the visible window when the cursor sits on
        // `catppuccin-mocha`.
        let desc = crate::theme::get_theme_description("catppuccin-frappe").unwrap_or("");
        if !desc.is_empty() {
            assert!(
                visible.contains(desc),
                "expected description '{desc}' in non-selected row; got:\n{visible}"
            );
        }
    }

    /// D-12: `render_into` dispatches on `state.preview_mode_full`.
    /// List-dominant mode shows the `◆ Catppuccin` family band but NOT the
    /// preview-block headings; full-preview mode shows `◆ Palette` +
    /// `◆ Code` (from `compose::compose_full`).
    #[test]
    fn mode_dispatch_uses_preview_mode_full() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();

        state.preview_mode_full = false;
        let list_out = render_to_vec(&state, 80, 24);
        let list_visible = strip_ansi(&list_out);
        assert!(
            list_visible.contains("◆ Catppuccin"),
            "list-dominant mode must show family heading; got:\n{list_visible}"
        );
        assert!(
            !list_visible.contains("◆ Palette"),
            "list-dominant mode must NOT show preview-block heading 'Palette'; got:\n{list_visible}"
        );

        state.preview_mode_full = true;
        let full_out = render_to_vec(&state, 80, 24);
        let full_visible = strip_ansi(&full_out);
        assert!(
            full_visible.contains("◆ Palette") && full_visible.contains("◆ Code"),
            "full-preview mode must show Palette + Code block headings; got:\n{full_visible}"
        );
    }
}
