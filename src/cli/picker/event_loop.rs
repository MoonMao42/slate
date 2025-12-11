//! Event loop and rendering for the interactive crossterm picker.
//! Per , and 06-CONTEXT research on crossterm + live preview.

use crate::error::Result;
use crate::env::SlateEnv;

/// Launch the interactive 2D picker for theme + opacity selection.
/// Enters alternate screen, sets up raw mode, manages crossterm event loop.
/// Returns Ok if user commits (Enter), or rollbacks cleanly on ESC/Ctrl+C.
pub fn launch_picker(env: &SlateEnv) -> Result<()> {
    // TODO: Complete event loop implementation in Task 3+
    // For now, stub return Ok to allow compilation
    Ok(())
}

/// Handle 's' (save auto theme) key in picker
pub fn handle_save_auto(_state: &mut super::state::PickerState, _env: &SlateEnv) -> Result<()> {
    // TODO: Implement auto-save logic in Task 4
    Ok(())
}

/// Handle 'r' (resume auto theme) key in picker
pub fn handle_resume_auto(_state: &mut super::state::PickerState, _env: &SlateEnv) -> Result<()> {
    // TODO: Implement auto-resume logic in Task 4
    Ok(())
}

/// Check if light theme opacity guardrail should apply
pub fn should_guard_light_theme_opacity(_state: &super::state::PickerState) -> bool {
    // TODO: Task 5 - implement light-theme guardrail logic
    false
}

/// Render Afterglow receipt with atomic flush
pub fn render_afterglow_receipt(_state: &super::state::PickerState, _env: &SlateEnv) -> Result<()> {
    // TODO: Task 7 - implement Afterglow rendering
    Ok(())
}

/// Show Frosted preview approximation cue for non-Ghostty terminals
pub fn show_frosted_preview_cue(_env: &SlateEnv) {
    // TODO: Task 6 - detect terminal and show cue for Frosted preview approximation
}
