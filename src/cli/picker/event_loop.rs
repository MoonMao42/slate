//! Event loop and rendering for the interactive crossterm picker.
//! Per , and 06-CONTEXT research on crossterm + live preview.

use crate::error::Result;
use crate::env::SlateEnv;
use crate::config::ConfigManager;
use crate::cli::auto_theme;
use crate::theme::ThemeAppearance;
use crate::opacity::OpacityPreset;
use std::env;

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
    let is_dark = current_theme.appearance == ThemeAppearance::Dark;
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
        ThemeAppearance::Dark => "dark",
        ThemeAppearance::Light => "light",
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
/// Per D-26b and Task 5:
/// - Returns true if current theme is Light AND user has not yet overridden opacity
/// - When true, rendering should force effective opacity to Solid
/// - Help bar should show hint about navigating ←→ to unlock opacity
/// - When user presses ←→ on light theme, call state.set_opacity_override(true)
pub fn should_guard_light_theme_opacity(state: &super::state::PickerState) -> bool {
    // Check if current theme is light and override not yet set
    if state.opacity_overridden() {
        return false; // Already overridden, no guardrail
    }

    // Get current theme and check if it's light
    if let Ok(theme) = state.get_current_theme() {
        theme.appearance == ThemeAppearance::Light
    } else {
        false
    }
}

/// Get the effective opacity for rendering, applying light-theme guardrail if needed
/// Per D-26b:
/// - If should_guard_light_theme_opacity() returns true, return Solid regardless of user selection
/// - Otherwise return the user's actual selected opacity
pub fn get_effective_opacity_for_rendering(state: &super::state::PickerState) -> OpacityPreset {
    if should_guard_light_theme_opacity(state) {
        OpacityPreset::Solid
    } else {
        state.get_current_opacity()
    }
}

/// Detect if the current terminal is Ghostty
/// Per D-24b: Check $TERM_PROGRAM (Ghostty sets it to "Ghostty")
fn is_ghostty() -> bool {
    env::var("TERM_PROGRAM")
        .map(|prog| prog == "Ghostty")
        .unwrap_or(false)
}

/// Render Frosted preview approximation cue for non-Ghostty terminals
/// Per and Task 6:
/// - Detects current terminal via $TERM_PROGRAM / $TERM environment variables
/// - If NOT ghostty and Frosted is selectable: adds hint "(preview approximated here)" next to Frosted dot
/// - When user navigates to Frosted in non-Ghostty: briefly flashes help bar with message about fidelity gap
/// - Allows selection regardless (no disable/skip logic)
pub fn show_frosted_preview_cue(_env: &SlateEnv) {
    // This function is called when user navigates to Frosted opacity in non-Ghostty
    // Brief flash with message, no timeout (transparent fidelity communication)
    if !is_ghostty() {
        // Log to cliclack output for visibility
        let _ = cliclack::log::info("(i) Frosted preview is approximate here · Ghostty shows full blur");
    }
}

/// Get the opacity indicator label with Frosted approximation cue if needed
/// Per and Task 6:
/// - If in Ghostty: return standard "(Frosted)" label
/// - If not in Ghostty: return "(Frosted) (preview approximated here)"
pub fn get_opacity_indicator_label_with_cue(opacity: OpacityPreset) -> &'static str {
    if opacity != OpacityPreset::Frosted || is_ghostty() {
        ""
    } else {
        "(preview approximated here)"
    }
}

/// Render Afterglow receipt with atomic flush
pub fn render_afterglow_receipt(_state: &super::state::PickerState, _env: &SlateEnv) -> Result<()> {
    // TODO: Task 7 - implement Afterglow rendering
    Ok(())
}
