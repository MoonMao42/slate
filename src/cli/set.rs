use crate::error::Result;
use crate::brand::language::Language;
use crate::design::symbols::Symbols;
use crate::config::ConfigManager;
use crate::theme::ThemeRegistry;

/// Handle `slate set <theme>` command
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement full interactive picker
    
    if let Some(theme_arg) = args.first() {
        // Explicit theme argument: resolve and apply
        let config_mgr = ConfigManager::new()?;
        let registry = ThemeRegistry::new()?;
        
        // Resolve theme from registry (fail if not found)
        let theme = registry.get(theme_arg)
            .ok_or_else(|| crate::error::SlateError::InvalidThemeData(
                format!("Theme '{}' not found", theme_arg)
            ))?;
        
        // Persist current theme
        config_mgr.set_current_theme(&theme.id)?;
        
        // Write shell integration file (regenerates env.zsh)
        config_mgr.write_shell_integration_file(theme)?;
        
        println!("{} Theme switched to '{}'", Symbols::SUCCESS, theme.name);
    } else {
        // No theme argument: will implement interactive picker
        println!("{}", Language::SET_PICKER_PENDING);
    }

    Ok(())
}
