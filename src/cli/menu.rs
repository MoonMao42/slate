//! Shared circular menus and opt-in confirmations with Slate styling.
//! Own navigation so migrated entry points have the same edge behavior.
use crate::brand::cliclack_theme::SlateTheme;
use cliclack::{Theme, ThemeState};
use console::{Key, Term};
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste},
    execute, terminal,
};
use std::{
    fmt::Display,
    io::{self, IsTerminal, Write},
};
use unicode_segmentation::UnicodeSegmentation;

mod input;
mod page;
pub(super) use page::ReadOnlyPage;

pub(crate) fn select<T: Clone + Eq>(prompt: impl Display) -> Menu<T, false> {
    Menu::new(prompt)
}

pub(crate) fn multiselect<T: Clone + Eq>(prompt: impl Display) -> Menu<T, true> {
    Menu::new(prompt)
}

/// Destructive consent defaults to No; Escape declines, Ctrl-C interrupts.
/// Preserve cliclack's explicit y/n answers without enabling them in lists.
pub(crate) fn confirm(prompt: impl Display) -> Menu<bool, false> {
    confirm_named(prompt, "No", "Yes")
}

pub(crate) fn confirm_named(prompt: impl Display, cancel: &str, accept: &str) -> Menu<bool, false> {
    let mut menu = select(prompt)
        .item(false, cancel, "")
        .item(true, accept, "")
        .initial_value(false)
        .escape_value(false);
    menu.answer_keys = vec![('y', true), ('n', false)];
    menu
}

struct Item<T> {
    value: T,
    label: String,
    hint: String,
    checked: bool,
}

/// Back carries pending checks for editing, never installation consent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MultiSelectOutcome<T> {
    Submitted(Vec<T>),
    Back(Vec<T>),
}

pub(crate) struct Menu<T, const MULTI: bool> {
    prompt: String,
    items: Vec<Item<T>>,
    cursor: usize,
    start: usize,
    max_rows: usize,
    required: bool,
    initial: Vec<T>,
    escape_value: Option<T>,
    answer_keys: Vec<(char, T)>,
    escape_back: bool,
    went_back: bool,
}

impl<T: Clone + Eq, const MULTI: bool> Menu<T, MULTI> {
    fn new(prompt: impl Display) -> Self {
        Self {
            prompt: prompt.to_string(),
            items: Vec::new(),
            cursor: 0,
            start: 0,
            max_rows: usize::MAX,
            required: true,
            initial: Vec::new(),
            escape_value: None,
            answer_keys: Vec::new(),
            escape_back: false,
            went_back: false,
        }
    }

    pub(crate) fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
        self.items.push(Item {
            value,
            label: label.to_string(),
            hint: hint.to_string(),
            checked: false,
        });
        self
    }

    pub(crate) fn items(mut self, items: &[(T, impl Display, impl Display)]) -> Self {
        for (value, label, hint) in items {
            self = self.item(value.clone(), label, hint);
        }
        self
    }

    pub(crate) fn max_rows(mut self, height: usize) -> Self {
        self.max_rows = height.max(1);
        self
    }

    fn keep_visible(&mut self, rows: usize) {
        let rows = rows.max(1);
        // A larger viewport must reclaim earlier rows rather than leaving an
        // obsolete scroll offset and unused space below the final item.
        self.start = self.start.min(self.items.len().saturating_sub(rows));
        if self.cursor < self.start {
            self.start = self.cursor;
        } else if self.cursor.saturating_sub(self.start) >= rows {
            self.start = self.cursor + 1 - rows;
        }
    }

    fn jump_to_edge(&mut self, last: bool, rows: usize) {
        self.cursor = if last {
            self.items.len().saturating_sub(1)
        } else {
            0
        };
        self.keep_visible(rows.max(1));
    }

    fn navigate(&mut self, up: bool, rows: usize) {
        let count = self.items.len();
        if count == 0 {
            return;
        }
        self.cursor = if up {
            if self.cursor == 0 {
                count - 1
            } else {
                self.cursor - 1
            }
        } else if self.cursor + 1 == count {
            0
        } else {
            self.cursor + 1
        };
        self.keep_visible(rows);
    }

    fn navigate_page(&mut self, up: bool, rows: usize) {
        let step = rows.max(1);
        self.cursor = if up {
            self.cursor.saturating_sub(step)
        } else {
            self.cursor
                .saturating_add(step)
                .min(self.items.len().saturating_sub(1))
        };
        self.keep_visible(step);
    }

    fn render(&self, state: &ThemeState, rows: usize) -> String {
        let theme = SlateTheme;
        let finished = matches!(state, ThemeState::Submit | ThemeState::Cancel);
        // Only clipped lists need a position cue. Keep it in the existing
        // header; viewport measurement accounts for wrapping on narrow screens.
        let heading = if !finished && rows < self.items.len() {
            format!("{} · {}/{}", self.prompt, self.cursor + 1, self.items.len())
        } else {
            self.prompt.clone()
        };
        let mut frame = theme.format_header(state, &heading);
        // The receipt must include checked items on earlier pages, too.
        let (start, count) = if finished {
            (0, self.items.len())
        } else {
            (self.start, rows)
        };
        for (index, item) in self.items.iter().enumerate().skip(start).take(count) {
            frame.push_str(&if MULTI {
                theme.format_multiselect_item(
                    state,
                    item.checked,
                    index == self.cursor,
                    &item.label,
                    &item.hint,
                )
            } else {
                theme.format_select_item(state, index == self.cursor, &item.label, &item.hint)
            });
        }
        frame.push_str(&theme.format_footer(state));
        frame
    }

    fn run(&mut self) -> io::Result<Vec<T>> {
        self.went_back = false;
        if self.items.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No items added to the list",
            ));
        }
        if self
            .escape_value
            .as_ref()
            .is_some_and(|value| !self.items.iter().any(|item| &item.value == value))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Menu escape destination is missing",
            ));
        }
        if MULTI {
            for item in &mut self.items {
                item.checked |= self.initial.contains(&item.value);
            }
        } else if let Some(index) = self
            .items
            .iter()
            .position(|item| self.initial.contains(&item.value))
        {
            self.cursor = index;
        }
        let mut term = Term::stderr();
        if !term.is_term() || !io::stdin().is_terminal() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        let terminal = MenuTerminal::enter(&mut term)?;
        let result = self.run_on(&mut term);
        terminal.finish(result)
    }

    fn run_on(&mut self, term: &mut Term) -> io::Result<Vec<T>> {
        let mut state = ThemeState::Active;
        let mut previous = String::new();
        loop {
            let (height, width) = term.size();
            let (frame, rows, choices_visible) =
                self.viewport(&state, usize::from(width), usize::from(height));
            if frame != previous {
                term.clear_last_lines(screen_lines(&previous, usize::from(width)))?;
                // Raw input mode disables the terminal's automatic LF -> CRLF.
                term.write_all(frame.replace('\n', "\r\n").as_bytes())?;
                term.flush()?;
                previous = frame;
            }
            match state {
                ThemeState::Submit => {
                    return Ok(if MULTI {
                        self.items
                            .iter()
                            .filter(|item| item.checked)
                            .map(|item| item.value.clone())
                            .collect()
                    } else {
                        vec![self.items[self.cursor].value.clone()]
                    })
                }
                ThemeState::Cancel => return Err(io::ErrorKind::Interrupted.into()),
                _ => {}
            }
            state = ThemeState::Active;
            let key = event::read().map(input::menu_key);
            let resized = term.size() != (height, width);
            match key {
                Ok(Some(Key::Home)) => self.jump_to_edge(false, rows),
                Ok(Some(Key::End)) => self.jump_to_edge(true, rows),
                Ok(Some(Key::PageUp)) => self.navigate_page(true, rows),
                Ok(Some(Key::PageDown)) => self.navigate_page(false, rows),
                Ok(Some(Key::ArrowUp | Key::ArrowLeft | Key::Char('k' | 'h' | '\x10'))) => {
                    self.navigate(true, rows)
                }
                Ok(Some(Key::ArrowDown | Key::ArrowRight | Key::Char('j' | 'l' | '\x0e'))) => {
                    self.navigate(false, rows)
                }
                Ok(Some(Key::Escape)) => {
                    if MULTI && self.escape_back {
                        term.clear_last_lines(screen_lines(&previous, usize::from(width)))?;
                        term.flush()?;
                        self.went_back = true;
                        return Ok(self
                            .items
                            .iter()
                            .filter(|item| item.checked)
                            .map(|item| item.value.clone())
                            .collect());
                    }
                    state =
                        if let Some(index) = self.escape_value.as_ref().and_then(|value| {
                            self.items.iter().position(|item| &item.value == value)
                        }) {
                            self.cursor = index;
                            ThemeState::Submit
                        } else {
                            ThemeState::Cancel
                        };
                }
                Ok(Some(Key::Char('\x03'))) => state = ThemeState::Cancel,
                Ok(Some(Key::Char(' '))) if MULTI && choices_visible && !resized => {
                    self.items[self.cursor].checked ^= true
                }
                Ok(Some(Key::Enter)) if choices_visible && !resized => {
                    state = if MULTI && self.required && !self.items.iter().any(|item| item.checked)
                    {
                        ThemeState::Error("Input required".into())
                    } else {
                        ThemeState::Submit
                    };
                }
                Ok(Some(Key::Char(key))) if !MULTI && choices_visible && !resized => {
                    if let Some(index) = self.answer_keys.iter().find_map(|(shortcut, value)| {
                        (key.to_ascii_lowercase() == *shortcut)
                            .then(|| self.items.iter().position(|item| &item.value == value))
                            .flatten()
                    }) {
                        self.cursor = index;
                        state = ThemeState::Submit;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    state = ThemeState::Cancel
                }
                Err(error) => return Err(error),
                _ => {}
            }
        }
    }

    fn viewport(
        &mut self,
        state: &ThemeState,
        width: usize,
        height: usize,
    ) -> (String, usize, bool) {
        // No useful layout candidate has more rows than actual items. In a
        // tall, narrow terminal, avoid retrying the same wrapped frame for
        // every unused terminal row before reaching a smaller item budget.
        let mut rows = self
            .max_rows
            .min(self.items.len().max(1))
            .min(height.saturating_sub(4).max(1));
        loop {
            self.keep_visible(rows);
            let frame = self.render(state, rows);
            if matches!(state, ThemeState::Submit | ThemeState::Cancel)
                || screen_lines(&frame, width) < height
            {
                return (frame, rows, true);
            }
            if rows == 1 {
                // Never accept a selection that the terminal cannot display.
                let notice = console::truncate_str(
                    if self.escape_value.is_some() || self.escape_back {
                        super::ui_language::tr(
                            "窗口太小，请放大；Esc 离开此页。",
                            "Window too small; enlarge. Esc back.",
                        )
                    } else {
                        super::ui_language::tr(
                            "窗口太小，请放大；Esc 取消。",
                            "Window too small; enlarge. Esc cancel.",
                        )
                    },
                    width.max(1),
                    "",
                );
                return (format!("{notice}\n"), rows, false);
            }
            rows -= 1;
        }
    }
}

impl<T: Clone + Eq> Menu<T, false> {
    pub(crate) fn answer_keys(mut self, keys: Vec<(char, T)>) -> Self {
        self.answer_keys = keys;
        self
    }

    /// Explicit non-mutating Back/Quit destination; never infer it from labels.
    /// Ctrl-C remains an interruption; escape destinations are always explicit.
    pub(crate) fn escape_value(mut self, value: T) -> Self {
        self.escape_value = Some(value);
        self
    }

    pub(crate) fn initial_value(mut self, value: T) -> Self {
        self.initial = vec![value];
        self
    }

    pub(crate) fn interact(&mut self) -> io::Result<T> {
        Ok(self.run()?.remove(0))
    }
}

impl<T: Clone + Eq> Menu<T, true> {
    /// Restore focus by identity after adding items, without checking a box.
    pub(crate) fn focus_value(mut self, value: Option<&T>) -> Self {
        self.cursor = value
            .and_then(|value| self.items.iter().position(|item| &item.value == value))
            .unwrap_or(0);
        self
    }

    pub(crate) fn focused_value(&self) -> Option<&T> {
        self.items.get(self.cursor).map(|item| &item.value)
    }

    /// Return pending choices on Back without treating them as submitted.
    pub(crate) fn interact_with_back(&mut self) -> io::Result<MultiSelectOutcome<T>> {
        self.escape_back = true;
        let values = self.run()?;
        Ok(if self.went_back {
            MultiSelectOutcome::Back(values)
        } else {
            MultiSelectOutcome::Submitted(values)
        })
    }

    pub(crate) fn initial_values(mut self, values: Vec<T>) -> Self {
        self.initial = values;
        self
    }

    pub(crate) fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
}

fn screen_lines(frame: &str, width: usize) -> usize {
    let width = width.max(1);
    frame
        .lines()
        .map(|line| {
            let plain = console::strip_ansi_codes(line);
            let (mut rows, mut columns) = (1, 0);
            for grapheme in plain.graphemes(true) {
                let cells = console::measure_text_width(grapheme);
                if cells == 0 {
                    continue;
                }
                // A wide glyph is indivisible; unused space at the right edge
                // cannot be carried over to fit it on the next physical row.
                if columns > 0 && columns + cells > width {
                    rows += 1;
                    columns = 0;
                }
                columns += cells;
            }
            rows
        })
        .sum()
}

struct MenuTerminal {
    term: Option<Term>,
    raw_owned: bool,
}

impl MenuTerminal {
    fn enter(term: &mut Term) -> io::Result<Self> {
        let raw_owned = !terminal::is_raw_mode_enabled()?;
        if raw_owned {
            terminal::enable_raw_mode()?;
        }
        // Construct the guard before subsequent IO, including partial failure.
        let guard = Self {
            term: Some(term.clone()),
            raw_owned,
        };
        execute!(term, EnableBracketedPaste)?;
        term.hide_cursor()?;
        Ok(guard)
    }

    fn restore(&mut self) -> io::Result<()> {
        let raw = if self.raw_owned {
            terminal::disable_raw_mode()
        } else {
            Ok(())
        };
        if raw.is_ok() {
            self.raw_owned = false;
        }
        let display = if let Some(mut term) = self.term.take() {
            let paste = execute!(term, DisableBracketedPaste);
            let cursor = term.show_cursor();
            paste.and(cursor)
        } else {
            Ok(())
        };
        raw.and(display)
    }

    fn finish<T>(mut self, result: io::Result<T>) -> io::Result<T> {
        let restored = self.restore();
        result.and_then(|value| restored.map(|()| value))
    }
}

impl Drop for MenuTerminal {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests;
