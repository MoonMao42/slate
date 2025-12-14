use crate::adapter::{alacritty, ghostty};
use crate::brand::language::Language;
use crate::cli::auto_theme;
use crate::cli::setup_executor::apply_theme_selection;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::ThemeRegistry;

/// Handle `slate set <theme>` command
/// Supports three modes:
/// 1. `slate set <theme>` — Set explicit theme
/// 2. `slate set --auto` — Apply auto-follow based on system appearance
/// 3. `slate set` (no args) — Interactive picker
pub fn handle(args: &[&str]) -> Result<()> {
    // Check for --auto flag
    if args.contains(&"--auto") {
        let env = SlateEnv::from_process()?;
        let config = ConfigManager::with_env(&env)?;

        // Resolve theme based on system appearance
        let theme_id = auto_theme::resolve_auto_theme(&env, &config)?;

        let registry = ThemeRegistry::new()?;
        let theme = registry.get(&theme_id).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!(
                "Auto-resolved theme '{}' not found",
                theme_id
            ))
        })?;

        apply_theme_selection(theme)?;

        println!(
            "{} Theme auto-switched to '{}' (system appearance)",
            Symbols::SUCCESS,
            theme.name
        );
        return Ok(());
    }

    if let Some(theme_arg) = args.first() {
        // Explicit theme argument: resolve and apply
        let registry = ThemeRegistry::new()?;

        // Resolve theme from registry (fail if not found)
        let theme = registry.get(theme_arg).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_arg))
        })?;

        apply_theme_selection(theme)?;

        println!("{} Theme switched to '{}'", Symbols::SUCCESS, theme.name);
    } else {
        // No theme argument: launch interactive picker
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)?;
    }

    Ok(())
}

/// Silent preview apply: updates only the live preview path without persisting theme/opacity state.
/// Called on every keystroke during picker navigation. Updates visual appearance
/// without committing the selection to ~/.config/slate/current and current-opacity.
/// This function:
/// 1. Does NOT write current/current-opacity files
/// 2. Updates terminal opacity/blur via adapters (Ghostty, Alacritty)
/// 3. Applies theme palette to adapters for visual preview
/// 4. Sends SIGUSR2 to Ghostty for hot-reload (best-effort)
/// 5. Produces NO stdout output (silent)
pub fn silent_preview_apply(env: &SlateEnv, theme_id: &str, opacity: OpacityPreset) -> Result<()> {
    let registry = ThemeRegistry::new()?;
    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    // Do NOT persist state files (current, current-opacity)
    // Just apply visual changes for preview

    // Apply theme palette to adapters (visual preview)
    let _config = ConfigManager::with_env(env)?;
    let adapter_registry = crate::adapter::ToolRegistry::default();
    let _results = adapter_registry.apply_theme_to_all(theme);

    // Update opacity/blur for Ghostty (best-effort)
    let _ = ghostty::write_opacity_config(env, opacity);
    let _ = ghostty::write_blur_radius(env, opacity);

    // Update opacity for Alacritty (best-effort)
    let _ = alacritty::write_opacity_config(env, opacity);

    // Attempt Ghostty hot-reload (best-effort, no error if fails)
    if let Some(ghostty_adapter) = adapter_registry.get_adapter("ghostty") {
        let _ = ghostty_adapter.reload();
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
pub fn silent_commit_apply(env: &SlateEnv, theme_id: &str, opacity: OpacityPreset) -> Result<()> {
    let config = ConfigManager::with_env(env)?;
    let registry = ThemeRegistry::new()?;

    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    // Persist state files
    config.set_current_theme(&theme.id)?;
    config.set_current_opacity_preset(opacity)?;

    // Write shell integration
    config.write_shell_integration_file(theme)?;

    // Apply theme to all adapters
    let adapter_registry = crate::adapter::ToolRegistry::default();
    let _results = adapter_registry.apply_theme_to_all(theme);

    // Update opacity/blur for terminal adapters
    let _ = ghostty::write_opacity_config(env, opacity);
    let _ = ghostty::write_blur_radius(env, opacity);
    let _ = alacritty::write_opacity_config(env, opacity);

    // Hot-reload Ghostty
    if let Some(ghostty_adapter) = adapter_registry.get_adapter("ghostty") {
        let _ = ghostty_adapter.reload();
    }

    Ok(())
}
