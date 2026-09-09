use crate::error::Result;
use std::io::Write;

// Names and error paths may contain terminal control sequences. Keep ordinary
// Unicode readable, but render controls and bidi overrides as literal escapes.
pub(crate) fn terminal_text(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_control() || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            text.extend(c.escape_default());
        } else {
            text.push(c);
        }
    }
    text
}

/// Read-only output: ignore an early consumer exit, not semantic blockers.
/// Callers must still check their plan/inventory outcome after writing.
pub(crate) fn write_output(output: &str) -> Result<()> {
    match write_required(output) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Before a mutating action, every output error (including BrokenPipe) matters.
pub(crate) fn write_required(output: &str) -> std::io::Result<()> {
    std::io::stdout().lock().write_all(output.as_bytes())
}
