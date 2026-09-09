use super::*;

#[test]
fn picker_layout_english_keeps_bounds_and_confirmation_visible() {
    std::thread::spawn(|| {
        let home = tempfile::tempdir().unwrap();
        let env = crate::env::SlateEnv::with_home(home.path().to_owned());
        crate::config::ui_language::save(&env, crate::config::ui_language::UiLanguage::English)
            .unwrap();
        crate::cli::ui_language::load_saved_ui_language(&env).unwrap();
        picker_layout_bounds_all_color_modes_and_opacity_chrome();
        picker_layout_full_preview_crops_long_prompts_without_styling_the_footer();
        picker_layout_list_fits_small_windows_without_losing_the_selection();
        picker_paste_feedback_remains_visible_in_small_list_and_full_views();
    })
    .join()
    .unwrap();
}

fn visible_frame(state: &PickerState, cols: u16, rows: u16) -> String {
    let mut output = Vec::new();
    render_into(&mut output, state, Some("Saved selection"), cols, rows).unwrap();
    console::strip_ansi_codes(std::str::from_utf8(&output).unwrap()).into_owned()
}

fn assert_bounds(raw: &[u8], cols: u16, rows: u16) -> String {
    let frame = console::strip_ansi_codes(std::str::from_utf8(raw).unwrap()).into_owned();
    if rows == 0 || cols < 2 {
        assert!(frame.is_empty());
    } else {
        assert!(
            frame.split("\r\n").count() <= rows as usize,
            "{cols}x{rows}: {frame:?}"
        );
        for line in frame.split("\r\n") {
            assert!(
                console::measure_text_width(line) < cols as usize,
                "{cols}x{rows}: {line:?}"
            );
        }
    }
    assert!(
        !frame.ends_with('\n'),
        "a trailing newline can scroll a full viewport"
    );
    frame
}

#[test]
fn picker_layout_bounds_all_color_modes_and_opacity_chrome() {
    use crate::brand::render_context::RenderMode;
    let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
    let mut state = PickerState::new(&theme.id, OpacityPreset::Frosted).unwrap();
    for mode in [RenderMode::None, RenderMode::Basic, RenderMode::Truecolor] {
        let ctx = RenderContext {
            theme: &theme,
            mode,
            cached_pill_bg: Some("48;2;50;50;70".into()),
        };
        let roles = Roles::new(&ctx);
        for supports_opacity in [false, true] {
            for (cols, rows) in [
                (0, 0),
                (1, 1),
                (8, 2),
                (20, 4),
                (24, 8),
                (40, 12),
                (80, 24),
                (120, 40),
            ] {
                for _ in 0..state.theme_ids().len() {
                    let before = state.preview_selection();
                    let mut output = Vec::new();
                    layout::list(
                        &state,
                        Some("Saved selection"),
                        cols,
                        rows,
                        Some(&roles),
                        supports_opacity,
                    )
                    .unwrap()
                    .write(&mut output, cols, rows)
                    .unwrap();
                    let frame = assert_bounds(&output, cols, rows);
                    assert_eq!(state.preview_selection(), before);
                    if cols >= 24 && rows >= 8 {
                        assert!(frame.contains(tr("Esc 取消", "Esc cancel")));
                        assert!(frame.contains('›'));
                    }
                    state.move_down();
                }
            }
        }
    }
}

#[test]
fn picker_layout_full_preview_crops_long_prompts_without_styling_the_footer() {
    let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
    let background = crossterm::style::SetBackgroundColor(Color::Red).to_string();
    let prompt = format!(
        "{background}{}",
        "🦀中文line repeated to exceed the viewport width\n".repeat(100)
    );
    for (cols, rows) in [
        (0, 0),
        (1, 1),
        (20, 4),
        (24, 8),
        (40, 12),
        (80, 24),
        (120, 60),
    ] {
        let mut output = Vec::new();
        layout::full(
            &state,
            Some("Saved selection"),
            cols,
            rows,
            None,
            Some(&prompt),
        )
        .unwrap()
        .write(&mut output, cols, rows)
        .unwrap();
        let frame = assert_bounds(&output, cols, rows);
        if cols >= 24 && rows >= 8 {
            assert!(frame.contains(tr("Esc 取消", "Esc cancel")));
            assert!(frame.contains("PgUp/PgDn"));
            let raw = std::str::from_utf8(&output).unwrap();
            for label in ["PgUp/PgDn", tr("Enter 保存", "Enter save")] {
                let next_section = raw.find(label).unwrap();
                if let Some(background) = raw[..next_section].rfind(&background) {
                    let reset = raw[..next_section]
                        .rfind(&SetAttribute(Attribute::Reset).to_string())
                        .unwrap();
                    assert!(reset > background);
                }
            }
        }
    }
}

#[test]
fn picker_layout_propagates_output_failure() {
    struct Broken;
    impl io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "closed private display",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
    assert!(render_into(&mut Broken, &state, None, 80, 24).is_err());
}

#[test]
fn picker_paste_feedback_remains_visible_in_small_list_and_full_views() {
    for rows in [4, 6, 8, 12, 24] {
        for full in [false, true] {
            let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
            state.preview_mode_full = full;
            let before = state.preview_selection();
            let mut out = Vec::new();
            render_into(
                &mut out,
                &state,
                Some(tr("已忽略粘贴 · 用 ↑↓ 选择", "Paste ignored · use ↑↓")),
                24,
                rows,
            )
            .unwrap();
            let visible = assert_bounds(&out, 24, rows);
            assert!(
                visible.contains(tr("已忽略粘贴", "Paste ignored")),
                "full={full} rows={rows}: {visible}"
            );
            assert!(visible.contains(tr("Esc 取消", "Esc cancel")));
            assert!(!visible.contains("Search /"));
            assert_eq!(state.preview_selection(), before);
        }
    }
}

#[test]
fn picker_layout_list_fits_small_windows_without_losing_the_selection() {
    for (cols, rows) in [(24, 8), (40, 12), (80, 24), (100, 40)] {
        for index in [0, 5, 15] {
            let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
            state.jump_to_theme(index);
            let frame = visible_frame(&state, cols, rows);
            assert!(
                frame.split("\r\n").count() <= rows as usize,
                "too many rows at {cols}x{rows}: {frame:?}"
            );
            for line in frame.split("\r\n") {
                assert!(
                    console::measure_text_width(line) < cols as usize,
                    "line wraps at {cols}x{rows}: {line:?}"
                );
            }
            assert!(frame.contains(tr("Esc 取消", "Esc cancel")));
            assert!(frame.contains('›'));
            assert!(!frame.contains("Search /"));
        }
    }
}
