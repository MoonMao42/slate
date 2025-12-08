use crate::brand::language::Language;
use crate::cli::setup_executor::apply_theme_selection;
use crate::cli::auto_theme;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeRegistry;

/// Handle `slate set <theme>` command
/// Supports three modes:
/// 1. `slate set <theme>` — Set explicit theme
/// 2. `slate set --auto` — Apply auto-follow based on system appearance
/// 3. Interactive picker (deferred)
pub fn handle(args: &[&str]) -> Result<()> {
    // Check for --auto flag
    if args.contains(&"--auto") {
        let env = SlateEnv::from_process()?;
        let config = ConfigManager::with_env(&env)?;
        
        // Resolve theme based on system appearance
        let theme_id = auto_theme::resolve_auto_theme(&env, &config)?;
        
        let registry = ThemeRegistry::new()?;
        let theme = registry.get(&theme_id).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(
                format!("Auto-resolved theme '{}' not found", theme_id)
            )
        })?;
        
        apply_theme_selection(theme)?;
        
        println!("{} Theme auto-switched to '{}' (system appearance)", Symbols::SUCCESS, theme.name);
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
        // No theme argument: interactive picker deferred to 
        println!("{}", Language::SET_PICKER_PENDING);
    }

    Ok(())
}
