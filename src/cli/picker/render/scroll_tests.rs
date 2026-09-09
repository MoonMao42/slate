use super::*;
use crossterm::event::KeyCode;

fn state() -> PickerState {
    let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
    state.preview_mode_full = true;
    state
}

fn frame(state: &PickerState, cols: u16, rows: u16) -> String {
    let mut output = Vec::new();
    // Deterministic roles: no active-profile reads in layout assertions.
    layout::full(
        state,
        None,
        cols,
        rows,
        None,
        state.cached_prompt(state.get_current_theme_id()),
    )
    .unwrap()
    .write(&mut output, cols, rows)
    .unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn picker_paging_reaches_every_block_with_sticky_help_and_clamps_after_resize() {
    for (cols, rows) in [(24, 8), (40, 12), (80, 24)] {
        let mut state = state();
        let selection = state.preview_selection();
        let mut pages = String::new();
        loop {
            let raw = frame(&state, cols, rows);
            let visible = console::strip_ansi_codes(&raw);
            assert!(visible.split("\r\n").count() <= rows as usize);
            assert!(visible
                .lines()
                .all(|line| console::measure_text_width(line) < cols as usize));
            assert!(visible.contains("PgUp/PgDn"));
            assert!(visible.contains(tr("Esc 取消", "Esc cancel")));
            pages.push_str(&visible);
            if !scroll_preview(&mut state, None, cols, rows, KeyCode::PageDown).unwrap() {
                break;
            }
        }
        for heading in [
            tr("调色板", "Palette"),
            tr("命令提示符", "Prompt"),
            tr("代码", "Code"),
            tr("文件", "Files"),
            "Git",
            tr("差异", "Diff"),
            "Lazygit",
            "Nvim",
        ] {
            assert!(
                pages.contains(&format!("◆ {heading}")),
                "missing {heading} at {cols}x{rows}"
            );
        }
        assert_eq!(state.preview_selection(), selection);
        assert!(!scroll_preview(&mut state, None, cols, rows, KeyCode::PageDown).unwrap());
        scroll_preview(&mut state, None, cols, rows, KeyCode::End).unwrap();
        assert_eq!(state.preview_scroll, usize::MAX);
        // PageUp must clamp the old bottom anchor to the newly enlarged view
        // before subtracting, not get stuck decrementing usize::MAX.
        assert!(scroll_preview(&mut state, None, 100, 30, KeyCode::PageUp).unwrap());
        assert_ne!(state.preview_scroll, usize::MAX);
        scroll_preview(&mut state, None, cols, rows, KeyCode::Home).unwrap();
        assert_eq!(state.preview_scroll, 0);
        assert!(!scroll_preview(&mut state, None, cols, rows, KeyCode::PageUp).unwrap());
    }
}

#[test]
fn picker_paging_long_prompt_end_anchor_and_inherited_styles_survive_clipping() {
    let mut state = state();
    let red = crossterm::style::SetBackgroundColor(Color::Red).to_string();
    let reset = SetAttribute(Attribute::Reset).to_string();
    state.cache_prompt(
        "catppuccin-mocha",
        format!(
            "{red}{}",
            (0..100)
                .map(|i| format!("prompt-{i:03} 中文🦀\n"))
                .collect::<String>()
        ),
    );
    scroll_preview(&mut state, None, 40, 12, KeyCode::PageDown).unwrap();
    scroll_preview(&mut state, None, 40, 12, KeyCode::PageDown).unwrap();
    let raw = frame(&state, 40, 12);
    let first_visible_prompt = raw.find("prompt-").unwrap();
    assert!(
        !raw.contains("prompt-000"),
        "skipped text must never be replayed"
    );
    let inherited = raw[..first_visible_prompt].rfind(&red).unwrap();
    assert!(raw[..first_visible_prompt].rfind(&reset).unwrap() < inherited);
    for label in ["PgUp/PgDn", tr("Enter 保存", "Enter save")] {
        let footer = raw.find(label).unwrap();
        assert!(raw[..footer].rfind(&reset).unwrap() > raw[..footer].rfind(&red).unwrap());
    }
    scroll_preview(&mut state, None, 80, 24, KeyCode::End).unwrap();
    assert!(console::strip_ansi_codes(&frame(&state, 80, 24)).contains("◆ Nvim"));
    // A deferred first fork or resize can replace the document after End.
    state.cache_prompt("catppuccin-mocha", "longer\n".repeat(200));
    assert!(console::strip_ansi_codes(&frame(&state, 80, 24)).contains("◆ Nvim"));
    state.cache_prompt("catppuccin-mocha", "short".into());
    assert!(console::strip_ansi_codes(&frame(&state, 80, 24)).contains("◆ Nvim"));
    assert!(scroll_preview(&mut state, None, 80, 24, KeyCode::PageUp).unwrap());
}

#[test]
// SWATCH-RENDERER: colon-form and split SGR fixtures verify display-only clipping.
fn picker_paging_clips_unicode_without_splitting_sgr_or_losing_tail_resets() {
    let red = "\x1b[48:2::255:0:0m";
    let reset = SetAttribute(Attribute::Reset).to_string();
    for text in ["🦀中文abcdef", "a\u{301}bcdef", "plain text", "👩‍💻abcdef"] {
        let styled = format!("{red}{text}{reset}");
        for width in 0..=20 {
            let clipped = scroll::clip_line(&styled, width);
            let plain = clipped.replace(red, "").replace(&reset, "");
            assert!(
                console::measure_text_width(&plain) <= width,
                "{width}: {plain:?}"
            );
            assert!(clipped.starts_with(red));
            assert!(clipped.ends_with(&reset));
            assert!(!plain.contains('\u{1b}'));
        }
    }
    let styled = format!("{red}ab{reset}cdef");
    assert_eq!(scroll::clip_line(&styled, 3), format!("{red}ab…{reset}"));
    assert_eq!(
        scroll::style_prefix(&format!("{red}hidden\n{reset}")),
        format!("{red}{reset}")
    );
}

#[test]
fn picker_paging_tiny_and_list_views_are_inert_and_theme_changes_reset() {
    let mut state = state();
    for (cols, rows) in [(0, 0), (1, 24), (40, 6)] {
        for key in [
            KeyCode::PageDown,
            KeyCode::PageUp,
            KeyCode::Home,
            KeyCode::End,
        ] {
            assert!(!scroll_preview(&mut state, None, cols, rows, key).unwrap());
            assert_eq!(state.preview_scroll, 0);
        }
    }
    scroll_preview(&mut state, None, 80, 24, KeyCode::End).unwrap();
    state.move_down();
    assert_eq!(state.preview_scroll, 0);
    scroll_preview(&mut state, None, 80, 24, KeyCode::End).unwrap();
    state.move_up();
    assert_eq!(state.preview_scroll, 0);
    scroll_preview(&mut state, None, 80, 24, KeyCode::End).unwrap();
    state.preview_mode_full = false;
    assert!(!scroll_preview(&mut state, None, 80, 24, KeyCode::PageDown).unwrap());
}
