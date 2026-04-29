use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
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
/// dispatches on `state.preview_mode_full` between the
/// list-dominant layout (default) and the full-screen preview layout.
pub(super) fn render(state: &PickerState, flash_text: Option<&str>) -> Result<()> {
    let (cols, rows) = terminal::size().map_err(io_err)?;
    let mut stdout = io::stdout();
    render_into(&mut stdout, state, flash_text, cols, rows)?;
    stdout.flush().map_err(crate::error::SlateError::IOError)?;
    Ok(())
}

/// Core renderer — writable-target-agnostic so tests can feed `Vec<u8>`.
/// Mode-dispatches on `state.preview_mode_full`:
/// * `false` → [`render_list_dominant`] (// — family
/// section headers + full-width lavender pill cursor + mute description +
/// opacity strip + help line).
/// * `true` → [`render_full_preview`] (responsive fold
/// preview via [`compose::compose_full`]).
pub(super) fn render_into<W: io::Write>(
    out: &mut W,
    state: &PickerState,
    flash_text: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<()> {
    if state.preview_mode_full {
        // Task 02: the full-preview path accepts an optional
        // forked-prompt string. `render_into` is read-only (`&PickerState`)
        // so it does NOT perform the fork itself — that's the event_loop's
        // job (Tab arm; cache populated on new theme entry). Here we
        // simply consult the cache via `cached_prompt` when
        // fork has populated it; otherwise compose self-draws per.
        let override_prompt = state.cached_prompt(state.get_current_theme_id());
        render_full_preview(out, state, flash_text, cols, rows, override_prompt)
    } else {
        render_list_dominant(out, state, flash_text, cols, rows)
    }
}

/// List-dominant layout (default). Existing –18 chrome +
/// family headers full-width pill cursor
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

    // Re-read terminal width early — needed for right-aligning the Tab hint
    // in the chrome header as well as the selected-row pill below.
    let (cols, _) = terminal::size().map_err(io_err).unwrap_or((80, rows));

    // Picker chrome header: "✦ slate theme + opacity picker Tab ▸ preview".
    // `r.logo()` carries the brand-lavender ✦ glyph + `slate` wordmark;
    // `r.path()` dims the descriptor so the eye lands on the wordmark first.
    // The right-aligned `Tab ▸ preview` hint is a permanent affordance
    // first-time users would otherwise miss the Tab shortcut buried in the
    // footer help line.
    let logo = roles
        .as_ref()
        .map(|r| r.logo())
        .unwrap_or_else(|| "✦ slate".to_string());
    let tagline_text = "theme + opacity picker";
    let tagline = roles
        .as_ref()
        .map(|r| r.path(tagline_text))
        .unwrap_or_else(|| tagline_text.to_string());
    let tab_hint_text = "Tab ▸ preview";
    let tab_hint = roles
        .as_ref()
        .map(|r| r.path(tab_hint_text))
        .unwrap_or_else(|| tab_hint_text.to_string());
    // Visible widths (ANSI-stripped). Logo "✦ slate" = 7 visible chars;
    // tagline + hint are plain ASCII plus one ▸ glyph counted as 1 char.
    const LOGO_VISIBLE: usize = 7;
    let left_visible = LOGO_VISIBLE + 3 + tagline_text.chars().count(); // logo + 3sp + tagline
    let right_visible = tab_hint_text.chars().count();
    let gap = (cols as usize)
        .saturating_sub(2 + left_visible + right_visible + 1) // 2 indent + 1 right margin
        .max(2);
    let spacer = " ".repeat(gap);
    queue_io(queue!(
        out,
        Print("\r\n  "),
        Print(&logo),
        Print("   "),
        Print(&tagline),
        Print(&spacer),
        Print(&tab_hint),
        Print("\r\n\r\n"),
    ))?;

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

        // family section header is a render-time band. Emitted whenever
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
        let preview_raw = compose::compose_mini(&current_theme.palette, roles.as_ref());
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
        .map(|r| r.path("s save-auto"))
        .unwrap_or_else(|| "s save-auto".to_string());
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

/// Full-screen preview layout . Delegates body construction to
/// [`compose::compose_full`] — the composer picks the responsive fold tier
/// (4/6/8 blocks) from terminal rows and stacks them with `◆ Heading`
/// labels (see). Opacity strip + help-line chrome is
/// intentionally hidden here (stays in list-dominant only).
/// `prompt_line_override` is glue point: the caller (event
/// loop / `render_into`) looks up `PickerState::cached_prompt`
/// for the current theme and passes `Some(&forked)` when a fork landed;
/// `None` falls back to the self-drawn SAMPLE_TOKENS prompt per.
/// This renderer stays fork-agnostic.
fn render_full_preview<W: io::Write>(
    out: &mut W,
    state: &PickerState,
    flash_text: Option<&str>,
    _cols: u16,
    rows: u16,
    prompt_line_override: Option<&str>,
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
    // Task 02: prompt_line_override is forwarded straight to
    // compose_full. When event_loop's Tab branch populates
    // PickerState::prompt_cache via fork_starship_prompt,
    // render_into picks it up via cached_prompt and passes it here; on
    // fork failure the cache stays empty so this receives None and
    // compose_full self-draws (silent fallback).
    let body = compose::compose_full(
        &current_theme.palette,
        tier,
        roles.as_ref(),
        prompt_line_override,
    );
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

/// Emit a single `◆ FamilyName` section header band.
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

/// Emit a single variant row in list-dominant mode.
/// Selected row: 2-space indent OUTSIDE the pill, then a full-width
/// lavender pill via `Roles::command(padded_body)` (pill width =
/// `cols - 2`).
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
    // Width budget for selected-row pill body.
    // `Roles::command` wraps the body with ONE space of internal padding on
    // each side (ANSI-BG + ` body ` + ANSI-reset), so the visible pill width
    // is `body_width + 2`. Combined with the 2-space indent that sits OUTSIDE
    // the pill, total visible row width = `4 + body_width`.
    // To keep the row strictly inside the terminal (no wrap, no trailing
    // empty lavender band from writing exactly `cols` chars + `\r\n`), leave
    // 1 col of right margin → `body_width ≤ cols - 5`.
    let width = (cols as usize).saturating_sub(5);

    if is_selected {
        // Selected row body: "› {name:20} {desc} Tab ▸" right-aligned
        // inside the pill. `Roles::command` wraps the whole padded string
        // in the alpha pill so the line reads as a single lavender
        // band. The `Tab ▸` tail reinforces the header's Tab-to-preview
        // affordance at the point of action.
        let left = format!("› {:<20}  {}", theme.name, desc);
        const TAIL: &str = "Tab ▸";
        let tail_cols = TAIL.chars().count();
        let left_cols = left.chars().count();
        // Minimum 2-char gap between desc and tail so they don't visually
        // merge. If the row is too narrow to hold both + gap, drop the tail
        // and fall back to plain padding.
        let padded = if width >= left_cols + tail_cols + 2 {
            let gap = width - left_cols - tail_cols;
            format!("{left}{spacer}{TAIL}", spacer = " ".repeat(gap))
        } else {
            format!("{:<width$}", left, width = width)
        };
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
pub(super) fn render_afterglow_receipt(
    state: &PickerState,
    applied_opacity: OpacityPreset,
) -> Result<()> {
    let output = build_afterglow_receipt(state, applied_opacity)?;
    let mut stdout = io::stdout();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

// SWATCH-RENDERER: intentionally raw ANSI. The afterglow receipt composes
// terminal-control escapes (`?1049l`, `?25h`) plus a palette-tinted swatch
// foreground into one `String`; the aggregate migration scanner must ignore
// this helper body the same way it ignores the write-to-stdout wrapper above.
fn build_afterglow_receipt(state: &PickerState, applied_opacity: OpacityPreset) -> Result<String> {
    let current_theme = state.get_current_theme()?;
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
            opacity_to_label(applied_opacity)
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

    Ok(output)
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
    /// `no_raw_styling_ansi_anywhere_in_user_surfaces` aggregate
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

    /// family-header bands are render-time decoration; `theme_ids`
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

    /// the selected-row pill body must span roughly the full
    /// terminal width (indent of 2 cols sits outside the pill).
    /// Acceptance updated 2026-04-20 UAT — `queue_variant_row` now reserves 1
    /// col of right margin so the pill never writes the terminal's last
    /// column (which triggered an auto-wrap + empty trailing lavender band
    /// in Ghostty). Effective visible pill = cols - 1.
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
        // trailing space; we accept anything at or above `cols - 5` to match
        // the current body-width budget (leading 2-space indent + 2-space
        // pill internal padding + 1-col right margin = 5).
        let body = selected_line.trim_start_matches(' ');
        let width_body = body.chars().count();
        assert!(
            width_body + 5 >= cols as usize,
            "pill body shorter than cols-5; got {width_body} of expected {}",
            cols - 5
        );
    }

    /// non-selected rows surface their `get_theme_description` text.
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

    /// Task 02: `render_full_preview` accepts a
    /// `prompt_line_override: Option<&str>` and forwards it into
    /// `compose::compose_full` so starship fork can inject
    /// the real forked prompt into the `◆ Prompt` block.
    /// Contract proof: when called with `Some("__FORK_MARKER__")` the
    /// rendered alt-screen output contains that marker in the prompt
    /// slot. `None` falls back to the self-drawn SAMPLE_TOKENS prompt
    /// (already covered by `mode_dispatch_uses_preview_mode_full`).
    #[test]
    fn render_full_preview_forwards_prompt_override() {
        let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        state.preview_mode_full = true;
        let marker = "__FORK_MARKER__";
        let mut buf = Cursor::new(Vec::<u8>::new());
        render_full_preview(&mut buf, &state, None, 80, 24, Some(marker))
            .expect("render_full_preview with override must succeed");
        let bytes = buf.into_inner();
        let visible = strip_ansi(&bytes);
        assert!(
            visible.contains(marker),
            "render_full_preview must forward prompt_line_override into the \
             ◆ Prompt block so  starship fork output lands in the \
             preview. Got:\n{visible}"
        );
    }

    /// `render_into` dispatches on `state.preview_mode_full`.
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

    #[test]
    fn list_mode_uses_mini_preview_not_legacy_color_matrix() {
        let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
        let out = render_to_vec(&state, 80, 24);
        let visible = strip_ansi(&out);
        assert!(
            visible.contains('❯'),
            "list mode should show the mini prompt preview glyph, got:\n{visible}"
        );
        assert!(
            !visible.contains("Normal:") && !visible.contains("Bright:"),
            "list mode must not render the legacy ANSI matrix preview, got:\n{visible}"
        );
    }

    #[test]
    fn afterglow_receipt_reports_applied_opacity_not_raw_selection() {
        let state = PickerState::new("catppuccin-latte", OpacityPreset::Clear).unwrap();
        let receipt = build_afterglow_receipt(&state, OpacityPreset::Solid)
            .expect("receipt should render for a valid picker state");
        let visible = strip_ansi(receipt.as_bytes());
        assert!(
            visible.contains("Opacity   Solid"),
            "receipt should report the opacity that actually landed, got:\n{visible}"
        );
        assert!(
            !visible.contains("Opacity   Clear"),
            "receipt must not echo the pre-guard selection when a light-theme opacity guard \
             forced a different applied opacity. Got:\n{visible}"
        );
    }
}
