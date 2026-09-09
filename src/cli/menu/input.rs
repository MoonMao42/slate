//! Pasted text is data, never a sequence of menu actions. Share the existing
//! crossterm event decoder with the theme picker rather than parsing paste bytes.
use console::Key;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

pub(super) fn menu_key(event: Event) -> Option<Key> {
    let Event::Key(key) = event else {
        return None;
    };
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    if key.modifiers == KeyModifiers::CONTROL {
        return match key.code {
            KeyCode::Char('c') => Some(Key::Char('\x03')),
            KeyCode::Char('p') => Some(Key::Char('\x10')),
            KeyCode::Char('n') => Some(Key::Char('\x0e')),
            _ => None,
        };
    }
    if !(key.modifiers - KeyModifiers::SHIFT).is_empty() {
        return None;
    }
    // Enhanced keyboard protocols distinguish held-key repeats. Navigation
    // may repeat, but a held confirmation must not submit successive menus or
    // repeatedly toggle a multi-select row. Legacy repeated Press events are
    // indistinguishable from intentional presses and remain unchanged.
    if key.kind == KeyEventKind::Repeat
        && !matches!(
            key.code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Char('h' | 'j' | 'k' | 'l')
        )
    {
        return None;
    }
    match key.code {
        KeyCode::Up => Some(Key::ArrowUp),
        KeyCode::Down => Some(Key::ArrowDown),
        KeyCode::Left => Some(Key::ArrowLeft),
        KeyCode::Right => Some(Key::ArrowRight),
        KeyCode::Home => Some(Key::Home),
        KeyCode::End => Some(Key::End),
        KeyCode::PageUp => Some(Key::PageUp),
        KeyCode::PageDown => Some(Key::PageDown),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Escape),
        KeyCode::Char(value) => Some(Key::Char(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn menu_paste_and_nonkey_events_never_become_actions() {
        for event in [
            // ANSI-FIXTURE: raw input for escaping or width checks.
            Event::Paste("\x1b[A\r\n hjkl \t".into()),
            Event::Paste(String::new()),
            Event::Resize(10, 2),
            Event::FocusGained,
            Event::FocusLost,
        ] {
            assert_eq!(menu_key(event), None);
        }
    }

    #[test]
    fn menu_keys_keep_navigation_and_control_shortcuts_but_ignore_releases() {
        for (code, modifiers, expected) in [
            (KeyCode::Up, KeyModifiers::NONE, Some(Key::ArrowUp)),
            (KeyCode::Down, KeyModifiers::NONE, Some(Key::ArrowDown)),
            (KeyCode::Home, KeyModifiers::NONE, Some(Key::Home)),
            (KeyCode::End, KeyModifiers::NONE, Some(Key::End)),
            (KeyCode::PageUp, KeyModifiers::NONE, Some(Key::PageUp)),
            (KeyCode::PageDown, KeyModifiers::NONE, Some(Key::PageDown)),
            (KeyCode::Enter, KeyModifiers::NONE, Some(Key::Enter)),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                Some(Key::Char('\x10')),
            ),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                Some(Key::Char('\x0e')),
            ),
            (
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                Some(Key::Char('\x03')),
            ),
            (KeyCode::Enter, KeyModifiers::ALT, None),
        ] {
            for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
                let key = KeyEvent::new_with_kind(code, modifiers, kind);
                let expected = if kind == KeyEventKind::Repeat && code == KeyCode::Enter {
                    None
                } else {
                    expected.clone()
                };
                assert_eq!(menu_key(Event::Key(key)), expected);
            }
            let key = KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Release);
            assert_eq!(menu_key(Event::Key(key)), None);
        }
    }

    #[test]
    fn repeated_confirmation_and_back_keys_do_not_cross_menu_boundaries() {
        for code in [
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Char(' '),
            KeyCode::Char('y'),
            KeyCode::Char('n'),
        ] {
            for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
                assert!(menu_key(Event::Key(KeyEvent::new_with_kind(
                    code,
                    modifiers,
                    KeyEventKind::Press
                )))
                .is_some());
                assert_eq!(
                    menu_key(Event::Key(KeyEvent::new_with_kind(
                        code,
                        modifiers,
                        KeyEventKind::Repeat
                    ))),
                    None
                );
            }
        }
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
        ] {
            assert!(menu_key(Event::Key(KeyEvent::new_with_kind(
                code,
                KeyModifiers::NONE,
                KeyEventKind::Repeat
            )))
            .is_some());
        }
    }
}
