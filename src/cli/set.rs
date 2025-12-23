use crate::design::typography::Typography;
use crate::env::SlateEnv;
use crate::error::Result;

/// Handle `slate set <theme>` command
/// Thin dispatcher that routes to new noun-driven subcommands:
/// 1. `slate set <theme>` — Delegate to `slate theme <theme>` + show dim tip
/// 2. `slate set --auto` — Delegate to `slate theme --auto` (no tip)
/// 3. `slate set` (no args) — Delegate to `slate theme` picker
pub fn handle(args: &[&str]) -> Result<()> {
    // Check for --auto flag (auto path delegates to theme --auto)
    if args.contains(&"--auto") {
        crate::cli::theme::handle_theme(None, true)?;
        // No dim tip for auto path — it's already using the new surface
        return Ok(());
    }

    // Explicit theme or picker path
    if let Some(theme_arg) = args.first() {
        // Direct theme apply with dispatcher
        crate::cli::theme::handle_theme(Some(theme_arg.to_string()), false)?;

        // Show dim tip for legacy usage
        print_dim_tip();
        Ok(())
    } else {
        // Picker path: launch interactive picker via theme
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)?;

        // After picker returns, show dim tip
        print_dim_tip();
        Ok(())
    }
}

/// Print a dim tip teaching users about the new `slate theme` surface
fn print_dim_tip() {
    let tip = "(i) Tip: 'slate set' is transitioning to 'slate theme'. Try 'slate theme <name>' next time.";
    println!();
    println!("{}", Typography::explanation(tip));
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
pub fn silent_preview_apply(env: &SlateEnv, theme_id: &str, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    let registry = crate::theme::ThemeRegistry::new()?;
    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    // Do NOT persist state files (current, current-opacity)
    // Just apply visual changes for preview

    // Apply theme palette to adapters (visual preview)
    let _config = crate::config::ConfigManager::with_env(env)?;
    let adapter_registry = crate::adapter::ToolRegistry::default();
    let _results = adapter_registry.apply_theme_to_all(theme);

    // Update opacity/blur for Ghostty (best-effort)
    let _ = crate::adapter::ghostty::write_opacity_config(env, opacity);
    let _ = crate::adapter::ghostty::write_blur_radius(env, opacity);

    // Update opacity for Alacritty (best-effort)
    let _ = crate::adapter::alacritty::write_opacity_config(env, opacity);

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
pub fn silent_commit_apply(env: &SlateEnv, theme_id: &str, opacity: crate::opacity::OpacityPreset) -> Result<()> {
    let config = crate::config::ConfigManager::with_env(env)?;
    let registry = crate::theme::ThemeRegistry::new()?;

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
    let _ = crate::adapter::ghostty::write_opacity_config(env, opacity);
    let _ = crate::adapter::ghostty::write_blur_radius(env, opacity);
    let _ = crate::adapter::alacritty::write_opacity_config(env, opacity);

    // Hot-reload Ghostty
    if let Some(ghostty_adapter) = adapter_registry.get_adapter("ghostty") {
        let _ = ghostty_adapter.reload();
    }

    Ok(())
}
