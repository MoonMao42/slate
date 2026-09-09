//! Bounded plain-text reports; never execute commands or accept mutation consent.
use super::*;
use console::Key;
use crossterm::event;
use std::io::Write;
use unicode_segmentation::UnicodeSegmentation;

impl ReadOnlyPage {
    pub(in crate::cli) fn view(&mut self, text: &str, title: &str, back: &str) -> io::Result<()> {
        let Some(term) = &mut self.term else {
            // Dumb terminals retain the ordinary scrollback report.
            console::Term::stdout().write_str(text)?;
            return crate::cli::menu::select(title)
                .item((), back, "")
                .escape_value(())
                .interact();
        };
        let guard = crate::cli::menu::MenuTerminal::enter(term)?;
        let result = view_on(term, text, title, back);
        guard.finish(result)
    }
}

fn wrapped(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in text.lines() {
        let safe = crate::cli::file_output::terminal_text(line);
        let mut row = String::new();
        let mut cells = 0;
        for grapheme in safe.graphemes(true) {
            let size = console::measure_text_width(grapheme);
            if cells + size > width && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                cells = 0;
            }
            if size > width {
                row.push('?');
                cells += 1;
            } else {
                row.push_str(grapheme);
                cells += size;
            }
        }
        rows.push(row);
    }
    rows
}

fn frame(
    text: &str,
    title: &str,
    back: &str,
    height: usize,
    width: usize,
    start: &mut usize,
) -> (String, usize, usize) {
    // Leave the last terminal column/row unused to prevent autowrap scrolling.
    let width = width.saturating_sub(1).max(1);
    let rows = wrapped(text, width);
    let mut footer = wrapped(&format!("◆ {title}\n│ ● {back}\n└"), width);
    let available = height.saturating_sub(1);
    if rows.len() > available.saturating_sub(footer.len()) {
        footer = wrapped(&format!("◆ {title} · ↑↓\n│ ● {back}\n└"), width);
    }
    let visible = available.saturating_sub(footer.len());
    let max_start = rows.len().saturating_sub(visible.max(1));
    *start = (*start).min(max_start);
    let mut output = rows
        .iter()
        .skip(*start)
        .take(visible)
        .cloned()
        .collect::<Vec<_>>();
    if visible == 0 {
        output.extend(footer.into_iter().take(available));
    } else {
        output.extend(footer);
    }
    (output.join("\r\n"), visible.max(1), max_start)
}

fn view_on(term: &mut Term, text: &str, title: &str, back: &str) -> io::Result<()> {
    let mut start = 0;
    let mut previous = None;
    loop {
        let (height, width) = term.size();
        let (output, visible, max_start) =
            frame(text, title, back, height.into(), width.into(), &mut start);
        let current = ((height, width), output);
        if previous.as_ref() != Some(&current) {
            execute!(term, Clear(ClearType::All), MoveTo(0, 0))?;
            term.write_all(current.1.as_bytes())?;
            term.flush()?;
            previous = Some(current);
        }
        match crate::cli::menu::input::menu_key(event::read()?) {
            Some(Key::Escape | Key::Enter) => return Ok(()),
            Some(Key::Char('\x03')) => return Err(io::ErrorKind::Interrupted.into()),
            Some(Key::ArrowUp | Key::Char('k' | '\x10')) => start = start.saturating_sub(1),
            Some(Key::ArrowDown | Key::Char('j' | '\x0e')) => start = (start + 1).min(max_start),
            Some(Key::PageUp) => start = start.saturating_sub(visible),
            Some(Key::PageDown) => start = start.saturating_add(visible).min(max_start),
            Some(Key::Home) => start = 0,
            Some(Key::End) => start = max_start,
            _ => {} // Resizes redraw; pasted text never becomes input.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_frames_fit_small_windows_and_reveal_the_last_row() {
        let text = (0..30)
            .map(|i| format!("工具 {i} · 未检测到\n"))
            .collect::<String>();
        for height in [1, 2, 8, 20, 40] {
            for width in [1, 2, 20, 40, 80] {
                let mut start = usize::MAX;
                let (output, _, _) = frame(&text, "工具总览", "返回", height, width, &mut start);
                assert!(output.lines().count() <= height.saturating_sub(1));
                assert!(output.lines().all(
                    |line| console::measure_text_width(line) <= width.saturating_sub(1).max(1)
                ));
                if height >= 8 && width >= 20 {
                    assert!(output.contains("工具 29"));
                }
            }
        }
    }
    #[test]
    fn report_wrapping_preserves_unicode_and_escapes_controls() {
        assert_eq!(wrapped("中文ab", 4), ["中文", "ab"]);
        assert_eq!(wrapped("e\u{301}abc", 2), ["e\u{301}a", "bc"]);
        assert!(!wrapped("bad\x1b[2J", 80).join("").contains('\x1b'));
    }

    #[test]
    fn narrow_report_rows_preserve_long_paths_and_combining_characters() {
        for source in [
            "/Users/测试用户/Library/Application Support/slate/目录/e\u{301}/configuration.toml",
            "Starship · 找到程序，但不在 PATH 中",
            "Source tag fnv1a64-v1-0123456789abcdef",
        ] {
            for width in 2..=80 {
                let rows = wrapped(source, width);
                assert_eq!(rows.join(""), source, "width {width}");
                assert!(rows
                    .iter()
                    .all(|row| console::measure_text_width(row) <= width));
            }
        }
    }
}
