//! Event loop and rendering for the interactive crossterm picker.
//! Per , and 06-CONTEXT research on crossterm + live preview.

use crate::error::Result;
use crate::env::SlateEnv;
use crate::config::ConfigManager;
use crate::cli::auto_theme;

/// Launch the interactive 2D picker for theme + opacity selection.
/// Enters alternate screen, sets up raw mode, manages crossterm event loop.
/// Returns Ok if user commits (Enter), or rollbacks cleanly on ESC/Ctrl+C.
pub fn launch_picker(_env: &SlateEnv) -> Result<()> {
    // TODO: Complete event loop implementation in Task 3+
    // For now, stub return Ok to allow compilation
    Ok(())
}

/// Handle 's' (save auto theme) key in picker
/// Per Task 4:
/// - Detects current theme's appearance (Dark/Light)
/// - Enters confirmation state with updated help text
/// - On Enter: write auto.toml with dark_theme or light_theme field
/// - Shows receipt and updates theme list with auto badge
/// - Opacity stays at user's current selection
pub fn handle_save_auto(state: &mut super::state::PickerState, env: &SlateEnv) -> Result<()> {
    use cliclack::confirm;

    let config = ConfigManager::with_env(env)?;
    let current_theme = state.get_current_theme()?;

    // Determine which appearance slot to save to
    let is_dark = current_theme.appearance == crate::theme::ThemeAppearance::Dark;
    let appearance_label = if is_dark { "Dark" } else { "Light" };

    // Confirmation prompt
    let prompt = format!(
        "Save {} as Auto {} theme?",
        current_theme.name, appearance_label
    );

    match confirm(&prompt).interact() {
        Ok(true) => {
            // Write auto.toml with this theme for its appearance
            let current_theme_id = state.get_current_theme_id();
            if is_dark {
                config.write_auto_config(Some(current_theme_id), None)?;
            } else {
                config.write_auto_config(None, Some(current_theme_id))?;
            }

            // Show success receipt (500ms visible, then continue)
            cliclack::log::success(format!(
                "✓ Auto {} saved: {}",
                appearance_label, current_theme.name
            ))?;

            Ok(())
        }
        Ok(false) => {
            // User cancelled
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
            Err(crate::error::SlateError::UserCancelled)
        }
        Err(e) => Err(crate::error::SlateError::IOError(e)),
    }
}

/// Handle 'r' (resume auto theme) key in picker
/// Per Task 4:
/// - Executes resolve_auto_theme pipeline to get the auto-resolved theme
/// - Moves cursor to that theme's row
/// - Renders cursor jump flash (entire row background in accent color ~300ms)
/// - Shows hint: "→ resumed auto ({dark|light}): {theme-id}"
/// - Opacity stays at user's current selection
pub fn handle_resume_auto(state: &mut super::state::PickerState, env: &SlateEnv) -> Result<()> {
    let config = ConfigManager::with_env(env)?;

    // Get the auto-resolved theme per pipeline
    let auto_theme_id = auto_theme::resolve_auto_theme(env, &config)?;

    // Detect current system appearance for messaging
    let system_appearance = auto_theme::detect_system_appearance()?;
    let appearance_label = match system_appearance {
        crate::theme::ThemeAppearance::Dark => "dark",
        crate::theme::ThemeAppearance::Light => "light",
    };

    // Find the auto theme in our list and jump cursor to it
    if let Some(idx) = state
        .theme_ids()
        .iter()
        .position(|id| id == &auto_theme_id)
    {
        state.jump_to_theme(idx);

        // Show hint (brief feedback)
        cliclack::log::info(format!(
            "→ resumed auto ({}): {}",
            appearance_label, auto_theme_id
        ))?;
    }

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
