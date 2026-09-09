use super::*;
use crossterm::event::KeyEventKind;
use std::cell::Cell;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn state() -> PickerState {
    PickerState::new("catppuccin-mocha", OpacityPreset::Solid).unwrap()
}

#[test]
fn picker_paging_input_changes_only_the_view_and_keeps_prompt_cached() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let mut state = state();
    state.preview_mode_full = true;
    let prompt = (0..100)
        .map(|i| format!("prompt line {i}\n"))
        .collect::<String>();
    state.cache_prompt("catppuccin-mocha", prompt.clone());
    let selection = state.preview_selection();
    let mut before = Vec::new();
    super::super::render::render_into(&mut before, &state, None, 80, 24).unwrap();
    let (batch, calls) = run(vec![key(KeyCode::PageDown)], &mut state, &env, &mut None);
    assert!(batch.redraw, "PageDown must reveal the next preview page");
    assert_eq!(calls, 0);
    assert_eq!(state.preview_selection(), selection);
    assert_eq!(
        state.cached_prompt("catppuccin-mocha"),
        Some(prompt.as_str())
    );
    let mut after = Vec::new();
    super::super::render::render_into(&mut after, &state, None, 80, 24).unwrap();
    assert_ne!(before, after);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn picker_paging_input_keeps_paging_and_ordered_theme_changes_independent() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let mut state = state();
    for id in state.theme_ids().to_owned() {
        state.cache_prompt(&id, "cached prompt\n".repeat(100));
    }
    let original = state.get_current_theme_id().to_owned();
    let (_, calls) = run(
        vec![key(KeyCode::Tab), key(KeyCode::End)],
        &mut state,
        &env,
        &mut None,
    );
    assert_eq!(calls, 0);
    assert_eq!(state.preview_scroll, usize::MAX);
    let (_, calls) = run(
        vec![key(KeyCode::Down), key(KeyCode::PageDown)],
        &mut state,
        &env,
        &mut None,
    );
    assert_eq!(calls, 1);
    assert_ne!(state.get_current_theme_id(), original);
    assert!(state.preview_scroll > 0);
    let (_, calls) = run(
        vec![key(KeyCode::PageDown), key(KeyCode::Up)],
        &mut state,
        &env,
        &mut None,
    );
    assert_eq!(calls, 1);
    assert_eq!(state.get_current_theme_id(), original);
    assert_eq!(state.preview_scroll, 0);
    run(
        vec![key(KeyCode::End), key(KeyCode::Tab), key(KeyCode::Tab)],
        &mut state,
        &env,
        &mut None,
    );
    assert_eq!(state.preview_scroll, 0);
    let (batch, calls) = run(
        vec![key(KeyCode::End), key(KeyCode::Enter), key(KeyCode::Home)],
        &mut state,
        &env,
        &mut None,
    );
    assert!(matches!(batch.exit, Some(ExitAction::Commit)));
    assert_eq!(calls, 0);
    assert_eq!(
        state.preview_scroll,
        usize::MAX,
        "keys after commit stay unread"
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn run(
    events: Vec<Event>,
    state: &mut PickerState,
    env: &SlateEnv,
    flash: &mut Option<Flash>,
) -> (input::Batch, usize) {
    let mut events = events.into_iter();
    let calls = Cell::new(0);
    let result = process_input_batch(
        events.next().unwrap(),
        || Ok(events.next()),
        state,
        env,
        flash,
        |_| {
            calls.set(calls.get() + 1);
            Ok(())
        },
    )
    .unwrap();
    (result, calls.get())
}

#[test]
fn picker_input_batch_orders_navigation_and_stops_at_first_exit() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    for (prefix, exit, commit) in [
        (0, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), true),
        (2, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), true),
        (0, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), false),
        (
            2,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            false,
        ),
        (
            0,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            false,
        ),
    ] {
        let mut state = state();
        let mut expected = self::state();
        let mut events = Vec::new();
        for _ in 0..prefix {
            expected.move_down();
            events.push(key(KeyCode::Down));
        }
        events.extend([
            Event::Key(exit),
            key(KeyCode::Char('s')),
            key(KeyCode::Enter),
        ]);
        let mut events = events.into_iter();
        let batch = process_input_batch(
            events.next().unwrap(),
            || Ok(events.next()),
            &mut state,
            &env,
            &mut None,
            |_| panic!("an exit must not publish another preview"),
        )
        .unwrap();
        assert_eq!(
            state.get_current_theme_id(),
            expected.get_current_theme_id()
        );
        assert_eq!(matches!(batch.exit, Some(ExitAction::Commit)), commit);
        assert!(batch.exit.is_some());
        assert_eq!(events.next(), Some(key(KeyCode::Char('s'))));
        assert!(!env.managed_file("auto.toml").exists());
    }
}

#[test]
fn picker_input_batch_coalesces_preview_and_redraws_ui_only_changes() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let mut state = state();
    let mut flash = None;
    let (batch, calls) = run(
        vec![key(KeyCode::Down), key(KeyCode::Down)],
        &mut state,
        &env,
        &mut flash,
    );
    assert!(batch.redraw);
    assert_eq!(calls, 1);
    let (batch, calls) = run(
        vec![key(KeyCode::Down), key(KeyCode::Up)],
        &mut state,
        &env,
        &mut flash,
    );
    assert!(batch.redraw);
    assert_eq!(calls, 0);
    // Avoid external prompt execution: the unchanged theme is already cached.
    let current_theme = state.get_current_theme_id().to_owned();
    state.cache_prompt(&current_theme, "cached fixture".into());
    for full in [true, false] {
        let (batch, calls) = run(vec![key(KeyCode::Tab)], &mut state, &env, &mut flash);
        assert!(batch.redraw);
        assert_eq!(state.preview_mode_full, full);
        assert_eq!(calls, 0);
    }
    let (batch, calls) = run(
        vec![Event::Resize(90, 30), Event::Resize(100, 40)],
        &mut state,
        &env,
        &mut flash,
    );
    assert!(batch.resized);
    assert_eq!(calls, 0);
    assert!(state.cached_prompt(state.get_current_theme_id()).is_none());
    let (batch, calls) = run(vec![key(KeyCode::Char('x'))], &mut state, &env, &mut flash);
    assert!(!batch.redraw);
    assert_eq!(calls, 0);
}

#[test]
fn picker_input_batch_saves_current_row_and_requests_visible_feedback() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let mut state = state();
    let mut expected_save = self::state();
    expected_save.move_down();
    let mut flash = None;
    let (batch, calls) = run(
        vec![
            key(KeyCode::Down),
            key(KeyCode::Char('s')),
            key(KeyCode::Down),
        ],
        &mut state,
        &env,
        &mut flash,
    );
    assert!(batch.redraw);
    assert_eq!(calls, 1);
    let saved = crate::config::ConfigManager::with_env(&env)
        .unwrap()
        .read_auto_config()
        .unwrap()
        .unwrap();
    assert_eq!(
        saved.dark_theme.as_deref(),
        Some(expected_save.get_current_theme_id())
    );
    assert!(flash
        .as_ref()
        .unwrap()
        .text
        .contains(tr("深色配对已保存", "Auto Dark saved")));
    let (batch, calls) = run(vec![key(KeyCode::Char('s'))], &mut state, &env, &mut flash);
    assert!(batch.redraw, "a successful save must request a new frame");
    assert_eq!(
        calls, 0,
        "saving auto selection does not change terminal preview"
    );
}

#[test]
fn picker_input_batch_bounds_reads_and_ignores_releases_without_dropping_repeats() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let reads = Cell::new(0);
    let handled = Cell::new(0);
    input::drain(
        key(KeyCode::Down),
        || {
            reads.set(reads.get() + 1);
            Ok(Some(key(KeyCode::Down)))
        },
        |_| {
            handled.set(handled.get() + 1);
            Ok(KeyOutcome::Continue)
        },
    )
    .unwrap();
    assert_eq!(reads.get(), input::MAX_BATCH_EVENTS - 1);
    assert_eq!(handled.get(), input::MAX_BATCH_EVENTS);

    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let mut state = state();
    let mut expected = self::state();
    expected.move_down();
    let mut events: Vec<_> = [
        KeyCode::Down,
        KeyCode::Tab,
        KeyCode::Char('s'),
        KeyCode::Enter,
        KeyCode::Esc,
    ]
    .map(|code| {
        Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
    })
    .into();
    events.push(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    )));
    let (batch, calls) = run(events, &mut state, &env, &mut None);
    assert!(batch.exit.is_none());
    assert_eq!(calls, 1);
    assert_eq!(
        state.get_current_theme_id(),
        expected.get_current_theme_id()
    );
    assert!(!state.preview_mode_full);
    assert!(!env.managed_file("auto.toml").exists());
}

#[test]
fn picker_input_batch_propagates_read_action_and_preview_errors() {
    let _sink = crate::brand::events::reset_sink_for_tests();
    let error = || crate::error::SlateError::Internal("injected input failure".into());
    let reads = Cell::new(0);
    assert!(input::drain(
        key(KeyCode::Down),
        || {
            reads.set(reads.get() + 1);
            Ok(Some(key(KeyCode::Enter)))
        },
        |_| Err(error())
    )
    .is_err());
    assert_eq!(
        reads.get(),
        0,
        "do not read confirmation after an action error"
    );
    assert!(input::drain(
        Event::FocusGained,
        || Err(error()),
        |_| Ok(KeyOutcome::Inert)
    )
    .is_err());

    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let result = process_input_batch(
        key(KeyCode::Down),
        || Ok(None),
        &mut state(),
        &env,
        &mut None,
        |_| Err(error()),
    );
    assert!(
        result.is_err(),
        "failed preview must terminate the event loop"
    );
}
