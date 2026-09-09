use super::*;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn run(
    state: &mut PickerState,
    env: &SlateEnv,
    events: Vec<Event>,
    flash: &mut Option<Flash>,
) -> (input::Batch, usize) {
    let mut events = events.into_iter();
    let mut calls = 0;
    let batch = process_input_batch(
        events.next().unwrap(),
        || Ok(events.next()),
        state,
        env,
        flash,
        |_| {
            calls += 1;
            Ok(())
        },
    )
    .unwrap();
    (batch, calls)
}

#[test]
fn picker_paste_ignored_payload_cannot_change_selection_view_or_preferences() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Frosted).unwrap();
    // Precache every theme so a regression to shortcut processing cannot fork.
    for id in state.theme_ids().to_owned() {
        state.cache_prompt(&id, "cached prompt".into());
    }
    for full in [false, true] {
        state.preview_mode_full = full;
        state.preview_scroll = 7;
        let before = state.preview_selection();
        for payload in [
            "rose\tdawn\r\n".into(),
            "s\nq\rhjkl\t".into(),
            "PRIVATE_PASTE\u{1b}".into(),
            "PRIVATE_PASTE".repeat(400),
            String::new(),
        ] {
            let mut flash = None;
            let (batch, calls) = run(&mut state, &env, vec![Event::Paste(payload)], &mut flash);
            assert!(batch.redraw);
            assert!(batch.exit.is_none());
            assert_eq!(calls, 0);
            assert_eq!(state.preview_selection(), before);
            assert_eq!(state.preview_mode_full, full);
            assert_eq!(state.preview_scroll, 7);
            assert_eq!(
                flash.unwrap().text,
                tr("已忽略粘贴 · 用 ↑↓ 选择", "Paste ignored · use ↑↓")
            );
        }
    }
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn picker_paste_ignored_preserves_ordered_navigation_and_first_exit() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
    let mut expected = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
    expected.move_down();
    let (batch, calls) = run(
        &mut state,
        &env,
        vec![
            key(KeyCode::Down),
            Event::Paste("s\nq".into()),
            key(KeyCode::Enter),
            key(KeyCode::Down),
        ],
        &mut None,
    );
    assert!(matches!(batch.exit, Some(ExitAction::Commit)));
    assert_eq!(state.preview_selection(), expected.preview_selection());
    assert_eq!(calls, 0);
    let (batch, calls) = run(
        &mut state,
        &env,
        vec![Event::Paste("s\nq".into()), key(KeyCode::Esc)],
        &mut None,
    );
    assert!(matches!(batch.exit, Some(ExitAction::Cancel)));
    assert_eq!(calls, 0);
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn picker_no_search_slash_is_inert_and_paste_batches_remain_bounded() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap();
    let before = state.preview_selection();
    let (batch, calls) = run(&mut state, &env, vec![key(KeyCode::Char('/'))], &mut None);
    assert!(!batch.redraw);
    assert!(batch.exit.is_none());
    assert_eq!(calls, 0);
    assert_eq!(state.preview_selection(), before);
    let mut handled = 0;
    let mut reads = 0;
    input::drain(
        Event::Paste("rose".into()),
        || {
            reads += 1;
            Ok(Some(Event::Paste("rose".into())))
        },
        |event| {
            assert!(matches!(event, input::Input::Paste));
            handled += 1;
            Ok(KeyOutcome::Continue)
        },
    )
    .unwrap();
    assert_eq!(reads, input::MAX_BATCH_EVENTS - 1);
    assert_eq!(handled, input::MAX_BATCH_EVENTS);
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}
