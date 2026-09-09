//! Ordered, bounded input processing. Coalesce rendering, not user actions.
use super::{ExitAction, KeyOutcome};
use crate::error::Result;
use crossterm::event::{Event, KeyEvent, KeyEventKind};

pub(super) const MAX_BATCH_EVENTS: usize = 32;

pub(super) enum Input {
    Key(KeyEvent),
    Paste,
}

#[derive(Default)]
pub(super) struct Batch {
    pub redraw: bool,
    pub resized: bool,
    pub exit: Option<ExitAction>,
}

pub(super) fn drain(
    first: Event,
    mut next: impl FnMut() -> Result<Option<Event>>,
    mut handle: impl FnMut(Input) -> Result<KeyOutcome>,
) -> Result<Batch> {
    let mut batch = Batch::default();
    let mut event = first;
    for index in 0..MAX_BATCH_EVENTS {
        let outcome = match event {
            // Enhanced keyboard protocols can report releases separately.
            // They are not a second navigation, save, toggle or confirmation.
            Event::Key(key) if key.kind != KeyEventKind::Release => Some(handle(Input::Key(key))?),
            Event::Paste(_) => Some(handle(Input::Paste)?),
            Event::Resize(_, _) => {
                batch.resized = true;
                None
            }
            _ => None,
        };
        if let Some(outcome) = outcome {
            match outcome {
                KeyOutcome::Continue => batch.redraw = true,
                KeyOutcome::Inert => {}
                KeyOutcome::Commit => batch.exit = Some(ExitAction::Commit),
                KeyOutcome::Cancel => batch.exit = Some(ExitAction::Cancel),
            }
        }
        // Do not read past an exit or starve rendering with a continuous queue.
        if batch.exit.is_some() || index + 1 == MAX_BATCH_EVENTS {
            break;
        }
        let Some(ready) = next()? else {
            break;
        };
        event = ready;
    }
    Ok(batch)
}
