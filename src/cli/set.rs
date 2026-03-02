use crate::brand::Language;
use crate::design::typography::Typography;
use crate::env::SlateEnv;
use crate::error::Result;

/// Handle `slate set` compatibility alias using structured clap arguments.
/// Routes to the noun-driven theme surface while preserving the current CLI:
/// 1. `slate set <theme>` → `slate theme <theme>` + dim tip
/// 2. `slate set --auto` → `slate theme --auto`
/// 3. `slate set` → theme picker + dim tip
pub fn handle(theme_name: Option<&str>, auto: bool) -> Result<()> {
    if auto {
        crate::cli::theme::handle_theme(None, true, false)?;
        return Ok(());
    }

    if let Some(theme_arg) = theme_name {
        crate::cli::theme::handle_theme(Some(theme_arg.to_string()), false, false)?;

        print_dim_tip();
        Ok(())
    } else {
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)?;

        print_dim_tip();
        Ok(())
    }
}

/// Print a dim tip teaching users about the new `slate theme` surface
fn print_dim_tip() {
    println!();
    println!(
        "{}",
        Typography::explanation(Language::SLATE_SET_DEPRECATION_TIP)
    );
}

/// Check if running in Ghostty terminal.
fn is_ghostty() -> bool {
    std::env::var("TERM_PROGRAM")
        .map(|t| t.to_lowercase() == "ghostty")
        .unwrap_or(false)
}

/// Silent preview apply: updates only the live preview path without persisting theme/opacity state.
/// Called on every keystroke during picker navigation. Updates visual appearance
/// without committing the selection to ~/.config/slate/current and current-opacity.
/// This function:
/// 1. Does NOT write current/current-opacity files
/// 2. Updates terminal opacity/blur via adapters (Ghostty, Alacritty)
/// 3. Applies theme palette to adapters for visual preview
/// 4. Attempts Ghostty live reload with permission-aware behavior (per)
/// 5. Produces NO stdout output (silent)
pub fn silent_preview_apply(
    env: &SlateEnv,
    theme_id: &str,
    opacity: crate::opacity::OpacityPreset,
) -> Result<()> {
    let registry = crate::theme::ThemeRegistry::new()?;
    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    // Do NOT persist state files (current, current-opacity)
    // Just apply visual changes for preview

    // Apply theme palette to adapters (visual preview)
    let config = crate::config::ConfigManager::with_env(env)?;
    let adapter_registry = crate::adapter::ToolRegistry::default();
    let _results = adapter_registry.apply_theme_to_all(theme);

    // Update opacity/blur for Ghostty (best-effort)
    let _ = crate::adapter::ghostty::write_opacity_config(env, opacity);
    let _ = crate::adapter::ghostty::write_blur_radius(env, opacity);

    // Update opacity for Alacritty (best-effort)
    let _ = crate::adapter::alacritty::write_opacity_config(env, opacity);

    // Update opacity for Kitty (best-effort, Kitty auto-reloads on file change)
    let _ = crate::adapter::kitty::write_opacity_config(env, opacity);

    // Attempt Ghostty live preview with permission-aware behavior (best-effort).
    // Per , Check if Ghostty reload permission is already known.
    // If permission state is unknown, try once and remember the result.
    // If permission is known to be disabled, skip reload silently.
    if let Some(ghostty_adapter) = adapter_registry.get_adapter("ghostty") {
        // Only attempt reload if we're in Ghostty
        if is_ghostty() {
            match config.is_live_preview_state_known() {
                Ok(true) => {
                    // Permission state is known
                    if let Ok(enabled) = config.is_live_preview_enabled() {
                        if enabled {
                            // Permission is known to be enabled, attempt reload
                            let _ = ghostty_adapter.reload();
                        }
                        // If enabled is false, skip reload silently (user declined)
                    }
                }
                Ok(false) => {
                    // Permission state is unknown, attempt reload once and remember result
                    match ghostty_adapter.reload() {
                        Ok(()) => {
                            // Success: remember permission as enabled
                            let _ = config.set_live_preview_enabled(true);
                        }
                        Err(_) => {
                            // Failed: remember permission as disabled
                            let _ = config.set_live_preview_enabled(false);
                        }
                    }
                }
                Err(_) => {
                    // Error reading config, fall back to best-effort reload attempt
                    let _ = ghostty_adapter.reload();
                }
            }
        }
    }

    // Kitty live preview: push colors via kitten @ set-colors (best-effort)
    if let Some(kitty_adapter) = adapter_registry.get_adapter("kitty") {
        let _ = kitty_adapter.reload();
    }

    Ok(())
}

/// Silent commit apply: persists theme/opacity state, then performs full apply.
/// Called on Enter key in picker. Commits the selection to persistent state,
/// then runs the full apply path including reload signals.
/// This function:
/// 1. Writes current file (theme ID)
/// 2. Writes current-opacity file (opacity preset)
/// 3. Applies theme palette to all adapters
/// 4. Updates opacity/blur configs
/// 5. Sends SIGUSR2 to Ghostty for hot-reload
/// 6. Produces NO stdout output (silent, Afterglow receipt printed by caller)
pub fn silent_commit_apply(
    env: &SlateEnv,
    theme_id: &str,
    opacity: crate::opacity::OpacityPreset,
) -> Result<()> {
    let registry = crate::theme::ThemeRegistry::new()?;

    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    crate::cli::theme_apply::ThemeApplyCoordinator::new(env).apply(theme)?;

    let config = crate::config::ConfigManager::with_env(env)?;
    config.set_current_opacity_preset(opacity)?;

    // Update opacity/blur for terminal adapters
    let _ = crate::adapter::ghostty::write_opacity_config(env, opacity);
    let _ = crate::adapter::ghostty::write_blur_radius(env, opacity);
    let _ = crate::adapter::alacritty::write_opacity_config(env, opacity);
    let _ = crate::adapter::kitty::write_opacity_config(env, opacity);

    Ok(())
}
