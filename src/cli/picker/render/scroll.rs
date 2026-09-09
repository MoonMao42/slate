//! Preview-only paging and SGR-aware clipping of already sanitized frame text.
use super::*;
use crossterm::event::KeyCode;
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub(super) struct Viewport {
    pub start: usize,
    pub end: usize,
    pub total: usize,
    pub rows: usize,
}

impl Viewport {
    pub fn new(requested: usize, rows: usize, total: usize) -> Self {
        let start = requested.min(total.saturating_sub(rows));
        Self {
            start,
            end: (start + rows).min(total),
            total,
            rows,
        }
    }

    pub fn label(self, cols: u16) -> String {
        let start = if self.end > self.start {
            self.start + 1
        } else {
            0
        };
        if cols >= 60 {
            format!(
                "PgUp/PgDn scroll · Home/End · {start}–{}/{}",
                self.end, self.total
            )
        } else {
            format!("PgUp/PgDn · {start}–{}/{}", self.end, self.total)
        }
    }
}

pub(in crate::cli::picker) fn scroll_preview(
    state: &mut PickerState,
    flash: Option<&str>,
    cols: u16,
    rows: u16,
    key: KeyCode,
) -> Result<bool> {
    if !state.preview_mode_full || !state.has_selection() || cols < 2 {
        return Ok(false);
    }
    // The same layout computes the actual body budget for drawing and input.
    // Roles affect styling, not line count. This path never forks a prompt.
    let frame = layout::full(
        state,
        flash,
        cols,
        rows,
        None,
        state.cached_prompt(state.get_current_theme_id()),
    )?;
    let Some(view) = frame.viewport.filter(|view| view.rows > 0) else {
        return Ok(false);
    };
    let step = view.rows.saturating_sub(1).max(1);
    let requested = match key {
        KeyCode::PageUp => view.start.saturating_sub(step),
        KeyCode::PageDown => view
            .start
            .saturating_add(step)
            .min(view.total.saturating_sub(view.rows)),
        KeyCode::Home => 0,
        KeyCode::End => usize::MAX,
        _ => return Ok(false),
    };
    // Clamp before stepping so PageUp works immediately after a resize or a
    // shorter prompt; End's anchor also works before a deferred first fork.
    state.preview_scroll = requested;
    Ok(Viewport::new(requested, view.rows, view.total).start != view.start)
}

/// Iterate only numeric SGR spans in frame text. External prompt control
/// sequences have already passed compose_full's sanitizer; this is not a
/// replacement for that trust boundary. Unlike console's ANSI iterator, this
/// also recognizes colon-form SGR accepted by the prompt sanitizer.
fn parts(mut text: &str) -> impl Iterator<Item = (&str, bool)> {
    std::iter::from_fn(move || {
        if text.is_empty() {
            return None;
        }
        let (end, sgr) = match text.find('\u{1b}') {
            None => (text.len(), false),
            Some(0) => {
                let end = text
                    .strip_prefix('\u{1b}')
                    .and_then(|tail| tail.strip_prefix('['))
                    .and_then(|tail| {
                        tail.find('m')
                            .filter(|&end| {
                                tail[..end]
                                    .bytes()
                                    .all(|ch| ch.is_ascii_digit() || ch == b';' || ch == b':')
                            })
                            .map(|end| end + 3)
                    });
                end.map_or((1, false), |end| (end, true))
            }
            Some(end) => (end, false),
        };
        let (part, rest) = text.split_at(end);
        text = rest;
        Some((part, sgr))
    })
}

pub(super) fn style_prefix(skipped: &str) -> String {
    parts(skipped)
        .filter_map(|(part, sgr)| sgr.then_some(part))
        .collect()
}

/// Clip visible text, keeping complete SGR spans from both sides of the cut.
/// The skipped tail may reset a style inherited by the following body line.
pub(super) fn clip_line(text: &str, width: usize) -> Cow<'_, str> {
    if width == 0 {
        return Cow::Owned(style_prefix(text));
    }
    let plain: String = parts(text)
        .filter_map(|(part, sgr)| (!sgr).then_some(part))
        .collect();
    if console::measure_text_width(&plain) <= width {
        return Cow::Borrowed(text);
    }
    let clipped = console::truncate_str(&plain, width, "…");
    let mut remaining = clipped.strip_suffix('…').unwrap_or(&clipped).len();
    let mut output = String::new();
    let mut cut = false;
    for (part, sgr) in parts(text) {
        if sgr {
            output.push_str(part);
        } else if !cut {
            let take = remaining.min(part.len());
            output.push_str(&part[..take]);
            remaining -= take;
            if remaining == 0 {
                output.push('…');
                cut = true;
            }
        }
    }
    Cow::Owned(output)
}
