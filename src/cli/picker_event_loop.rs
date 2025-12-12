//! Event loop and rendering for the interactive crossterm picker.
//! Per , and 06-CONTEXT research on crossterm + live preview.

use crate::error::Result;
use crate::env::SlateEnv;
use crate::cli::picker::PickerState;
use crate::cli::set::{silent_preview_apply, silent_commit_apply};
use crate::opacity::OpacityPreset;
use crate::theme::ThemeRegistry;
use std::io::{self, Write};

/// Launch the interactive 2D picker for theme + opacity selection.
/// Enters alternate screen, sets up raw mode, manages crossterm event loop.
/// Returns Ok if user commits (Enter), or rollbacks cleanly on ESC/Ctrl+C.
pub fn launch_picker(env: &SlateEnv) -> Result<()> {
    // TODO: Complete event loop implementation in Task 3+
    // For now, stub return Ok to allow compilation
    Ok(())
}

/// Render the picker UI showing available themes and opacity.
fn render_picker(_state: &PickerState) {
    // TODO: Implement rendering in Task 3
}

/// Handle keyboard and mouse events.
fn handle_event(_state: &mut PickerState, _key: &str) {
    // TODO: Implement event handling in Task 3
}
