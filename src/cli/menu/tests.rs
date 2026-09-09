use super::*;

#[test]
fn multiselect_focus_tracks_identity_without_changing_checks() {
    let mut menu = multiselect("Tools")
        .items(&[
            ("third", "Three", ""),
            ("first", "One", ""),
            ("second", "Two", ""),
        ])
        .focus_value(Some(&"second"));
    assert_eq!(menu.focused_value(), Some(&"second"));
    assert!(menu.items.iter().all(|item| !item.checked));
    let (_, _, visible) = menu.viewport(&ThemeState::Active, 80, 6);
    assert!(visible);
    assert_eq!(menu.focused_value(), Some(&"second"));
    assert!(menu.start > 0);
    menu = menu.focus_value(Some(&"removed"));
    assert_eq!(menu.focused_value(), Some(&"third"));
    assert!(menu.items.iter().all(|item| !item.checked));
    assert_eq!(
        multiselect::<&str>("Empty")
            .focus_value(Some(&"missing"))
            .focused_value(),
        None
    );
}

#[test]
fn confirmation_answers_are_opt_in_and_default_to_decline() {
    let menu = confirm("Restore?");
    assert_eq!(menu.initial, vec![false]);
    assert_eq!(menu.escape_value, Some(false));
    assert_eq!(menu.answer_keys, vec![('y', true), ('n', false)]);
    assert!(select::<bool>("ordinary list").answer_keys.is_empty());
    assert!(multiselect::<bool>("ordinary checklist")
        .answer_keys
        .is_empty());
}

#[test]
fn viewport_budget_never_exceeds_actual_items_in_tall_terminals() {
    let mut menu = select("Tall").item(0, "only", "");
    let (frame, rows, visible) = menu.viewport(&ThemeState::Active, 80, u16::MAX as usize);
    assert!(visible && frame.contains("only"));
    assert_eq!(rows, 1);
    let mut wrapped = select("wrapped").item(0, "x".repeat(2000), "");
    let (frame, rows, visible) = wrapped.viewport(&ThemeState::Active, 1, 1000);
    assert!(!visible);
    assert_eq!(rows, 1);
    assert!(screen_lines(&frame, 1) <= 1);
}

#[test]
fn expanding_viewport_reveals_previously_scrolled_items_without_moving_selection() {
    let mut menu = select("Resize");
    for index in 0..6 {
        menu = menu.item(index, format!("item {index}"), "");
    }
    menu.jump_to_edge(true, 2);
    let (small, _, visible) = menu.viewport(&ThemeState::Active, 80, 6);
    assert!(visible && small.contains("item 5") && !small.contains("item 0"));
    let (large, _, visible) = menu.viewport(&ThemeState::Active, 80, 20);
    assert!(visible);
    assert_eq!(menu.cursor, 5);
    assert_eq!(menu.start, 0);
    for index in 0..6 {
        assert!(large.contains(&format!("item {index}")));
    }
    assert!(!large.contains("6/6"));
}

#[test]
fn clipped_lists_show_position_without_polluting_full_lists_or_receipts() {
    let mut menu = select("Choose").items(&[(0, "first", ""), (1, "middle", ""), (2, "last", "")]);
    assert!(menu.render(&ThemeState::Active, 2).contains("Choose · 1/3"));
    menu.jump_to_edge(true, 2);
    assert!(menu.render(&ThemeState::Active, 2).contains("Choose · 3/3"));
    assert!(!menu.render(&ThemeState::Active, 3).contains("/3"));
    assert!(!menu.render(&ThemeState::Submit, 2).contains("/3"));
    assert!(!menu.render(&ThemeState::Cancel, 2).contains("/3"));
}

#[test]
fn page_navigation_uses_visible_rows_and_clamps_without_toggling_choices() {
    let mut menu = multiselect("pages");
    for index in 0..9 {
        menu = menu.item(index, index, "");
    }
    menu.items[1].checked = true;
    for expected in [3, 6, 8, 8] {
        menu.navigate_page(false, 3);
        assert_eq!(menu.cursor, expected);
        assert!((menu.start..menu.start + 3).contains(&menu.cursor));
    }
    for expected in [5, 2, 0, 0] {
        menu.navigate_page(true, 3);
        assert_eq!(menu.cursor, expected);
        assert!((menu.start..menu.start + 3).contains(&menu.cursor));
    }
    assert_eq!(
        menu.items
            .iter()
            .filter(|item| item.checked)
            .map(|item| item.value)
            .collect::<Vec<_>>(),
        vec![1]
    );
    menu.navigate_page(false, 0);
    assert_eq!(menu.cursor, 1);
    let mut empty = select::<u8>("empty");
    empty.navigate_page(false, usize::MAX);
    empty.navigate_page(true, 0);
    assert_eq!((empty.cursor, empty.start), (0, 0));
}

#[test]
fn edge_navigation_scrolls_without_changing_multiselect_choices() {
    let mut menu =
        multiselect("fixture").items(&[(0, "first", ""), (1, "middle", ""), (2, "last", "")]);
    menu.items[1].checked = true;
    menu.jump_to_edge(true, 1);
    assert_eq!((menu.cursor, menu.start), (2, 2));
    menu.jump_to_edge(false, 1);
    assert_eq!((menu.cursor, menu.start), (0, 0));
    assert_eq!(
        menu.items
            .iter()
            .map(|item| item.checked)
            .collect::<Vec<_>>(),
        vec![false, true, false]
    );
    let mut empty = select::<u8>("empty");
    empty.jump_to_edge(true, 0);
    empty.jump_to_edge(false, 0);
    assert_eq!((empty.cursor, empty.start), (0, 0));
}

#[test]
fn menu_escape_requires_an_explicit_existing_destination() {
    let mut menu = select("fixture")
        .item("save", "Save", "")
        .escape_value("back");
    assert_eq!(
        menu.interact().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(select::<u8>("no implicit escape action")
        .escape_value
        .is_none());
}

#[test]
fn circular_menu_crosses_both_edges_and_scrolls_to_the_selected_item() {
    let mut menu = select("fixture")
        .items(&[(0, "first", ""), (1, "middle", ""), (2, "last", "")])
        .max_rows(2);
    menu.navigate(true, 2);
    assert_eq!((menu.cursor, menu.start), (2, 1));
    assert!(menu.render(&ThemeState::Active, 2).contains("last"));
    assert!(!menu.render(&ThemeState::Active, 2).contains("first"));
    menu.navigate(false, 2);
    assert_eq!((menu.cursor, menu.start), (0, 0));
    for _ in 0..3 {
        menu.navigate(false, 2);
    }
    assert_eq!((menu.cursor, menu.start), (0, 0));
}

#[test]
fn circular_menu_empty_and_single_item_lists_are_safe() {
    let mut empty = select::<u8>("empty");
    empty.navigate(true, 1);
    empty.navigate(false, 1);
    assert_eq!(
        empty.interact().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let mut single = select("single").item(1, "only", "").max_rows(0);
    assert_eq!(single.max_rows, 1);
    for up in [true, false, true] {
        single.navigate(up, 1);
        assert_eq!((single.cursor, single.start), (0, 0));
    }
}

#[test]
fn circular_multiselect_keeps_checked_items_across_wrapping_and_receipts() {
    let mut menu = multiselect("fixture")
        .items(&[(0, "first", ""), (1, "middle", ""), (2, "last", "")])
        .max_rows(1);
    menu.items[0].checked = true;
    menu.navigate(true, 1);
    menu.items[2].checked = true;
    menu.navigate(false, 1);
    assert_eq!((menu.cursor, menu.start), (0, 0));
    let receipt = menu.render(&ThemeState::Submit, 1);
    assert!(receipt.contains("first") && receipt.contains("last"));
    assert!(!receipt.contains("middle"));
}

#[test]
fn circular_menu_redraw_counts_visible_columns_not_ansi_bytes() {
    assert_eq!(screen_lines("", 4), 0);
    assert_eq!(screen_lines("12345\n\n中文\n", 4), 4);
    // ANSI-FIXTURE: raw input for escaping or width checks.
    assert_eq!(screen_lines("\x1b[31m12345\x1b[0m\n", 4), 2);
    assert_eq!(screen_lines("x\n", 0), 1);
}

#[test]
fn circular_menu_wide_characters_wrap_without_splitting_terminal_cells() {
    // Each two-column character needs its own row in a three-column terminal.
    assert_eq!(screen_lines("中文中文中文\n", 3), 6);
    // ANSI-FIXTURE: raw input for escaping or width checks.
    assert_eq!(screen_lines("\x1b[31m中文中文中文\x1b[0m\n", 3), 6);
    assert_eq!(screen_lines("a中文\n", 3), 2);
    assert_eq!(screen_lines("e\u{301}e\u{301}\n", 1), 2);
    assert_eq!(screen_lines("👩‍💻👩‍💻\n", 3), 2);
}

#[test]
fn circular_menu_viewport_budgets_wrapped_rows_and_keeps_selection_visible() {
    let mut menu = select("Choose");
    for value in 0..8 {
        menu = menu.item(value, format!("item {value}: a longer label"), "");
    }
    menu.navigate(true, 8);
    let (frame, rows, visible) = menu.viewport(&ThemeState::Active, 18, 12);
    assert!(visible && rows < 8);
    assert!(screen_lines(&frame, 18) < 12);
    assert!(frame.contains("item 7"));
    menu.navigate(false, rows);
    let (frame, _, visible) = menu.viewport(&ThemeState::Active, 18, 12);
    assert!(visible && frame.contains("item 0"));
    assert!(screen_lines(&frame, 18) < 12);
}

#[test]
fn circular_menu_english_tiny_window_keeps_safe_exit_and_hidden_choices() {
    std::thread::spawn(|| {
        let home = tempfile::tempdir().unwrap();
        let env = crate::env::SlateEnv::with_home(home.path().to_owned());
        crate::config::ui_language::save(&env, crate::config::ui_language::UiLanguage::English)
            .unwrap();
        crate::cli::ui_language::load_saved_ui_language(&env).unwrap();
        let mut menu = select("Choose").item(1, "First", "");
        let (frame, _, visible) = menu.viewport(&ThemeState::Active, 40, 2);
        assert!(!visible && frame.contains("Esc cancel"));
        assert_eq!(screen_lines(&frame, 40), 1);
        let mut menu = menu.escape_value(1);
        let (frame, _, visible) = menu.viewport(&ThemeState::Active, 40, 2);
        assert!(!visible && frame.contains("Esc back"));
        assert!(!frame.contains("cancel"));
        assert!(menu.viewport(&ThemeState::Active, 80, 24).2);
        let mut menu = multiselect("Tools").item(1, "First", "");
        let (frame, _, visible) = menu.viewport(&ThemeState::Active, 40, 2);
        assert!(!visible && frame.contains("Esc cancel"));
    })
    .join()
    .unwrap();
}

#[test]
fn circular_menu_requires_visible_choices_before_accepting_input() {
    let mut menu = select("A menu").item(1, "First", "a long hint");
    let (frame, _, visible) = menu.viewport(&ThemeState::Active, 40, 2);
    assert!(!visible);
    assert!(frame.contains("窗口太小"));
    assert!(frame.contains("Esc 取消"));
    assert_eq!(screen_lines(&frame, 40), 1);
    assert!(menu.viewport(&ThemeState::Active, 80, 24).2);
    let mut menu = menu.escape_value(1);
    let (frame, _, visible) = menu.viewport(&ThemeState::Active, 40, 2);
    assert!(!visible && frame.contains("Esc 离开此页"));
    assert!(!frame.contains("取消"));
}

#[test]
fn chinese_discovery_menu_keeps_wrapped_selection_within_viewport() {
    for width in [20, 40, 80] {
        let mut menu = select("你想改善哪一部分？").max_rows(8);
        for (index, label) in [
            "终端窗口",
            "命令提示符与 Shell",
            "文件与系统",
            "开发工具",
            "分屏会话",
        ]
        .into_iter()
        .enumerate()
        {
            menu = menu.item(index, label, "查看用途与配色支持 · 浏览不会安装或修改设置");
        }
        for _ in 0..10 {
            let (frame, rows, visible) = menu.viewport(&ThemeState::Active, width, 24);
            assert!(visible, "width={width}: {frame}");
            assert!(screen_lines(&frame, width) < 24, "width={width}: {frame}");
            assert!(frame.contains(&menu.items[menu.cursor].label));
            menu.navigate(false, rows);
        }
    }
}
