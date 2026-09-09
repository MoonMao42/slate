//! Opt-in scratch screen for read-only pages, never for mutation receipts.
use console::Term;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, IsTerminal};

mod pager;

pub(in crate::cli) struct ReadOnlyPage {
    term: Option<Term>,
}

impl ReadOnlyPage {
    pub(in crate::cli) fn enter() -> io::Result<Self> {
        let enabled = io::stdin().is_terminal()
            && io::stdout().is_terminal()
            && io::stderr().is_terminal()
            && std::env::var("TERM").is_ok_and(|value| value != "dumb" && !value.is_empty());
        let mut page = Self {
            term: enabled.then(Term::stderr),
        };
        // Arm cleanup before writing so partial setup also attempts restoration.
        if let Some(term) = &mut page.term {
            execute!(term, EnterAlternateScreen)?;
        }
        Ok(page)
    }

    pub(in crate::cli) fn clear(&mut self) -> io::Result<()> {
        if let Some(term) = &mut self.term {
            execute!(term, Clear(ClearType::All), MoveTo(0, 0))?;
        }
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        if let Some(term) = &mut self.term {
            execute!(term, LeaveAlternateScreen)?;
            self.term = None;
        }
        Ok(())
    }

    pub(in crate::cli) fn finish<T, E: From<io::Error>>(
        mut self,
        result: Result<T, E>,
    ) -> Result<T, E> {
        let restored = self.restore().map_err(E::from);
        result.and_then(|value| restored.map(|()| value))
    }
}

impl Drop for ReadOnlyPage {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
