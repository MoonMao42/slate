use crate::cli::auto_theme;
use crate::cli::setup_executor::apply_theme_selection;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeRegistry;

/// Handle `slate theme` command
/// Supports three modes:
/// 1. `slate theme <name>` — Apply explicit theme directly
/// 2. `slate theme --auto` — Apply auto-resolved theme based on system appearance
/// 3. `slate theme` (no args) — Launch interactive picker
pub fn handle_theme(theme_name: Option<String>, auto: bool, quiet: bool) -> Result<()> {
    if auto {
        // Auto path: resolve theme based on system appearance
        let env = SlateEnv::from_process()?;
        let config = ConfigManager::with_env(&env)?;

        let theme_id = auto_theme::resolve_auto_theme(&env, &config)?;

        let registry = ThemeRegistry::new()?;
        let theme = registry.get(&theme_id).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!(
                "Auto-resolved theme '{}' not found",
                theme_id
            ))
        })?;

        // In quiet mode, suppress all stderr output from apply_theme_selection
        if quiet {
            // Redirect stderr to /dev/null for the duration of apply
            use std::fs::File;
            use std::os::unix::io::AsRawFd;
            let devnull = File::open("/dev/null").ok();
            let saved_stderr = unsafe { libc::dup(2) };
            if let Some(ref f) = devnull {
                unsafe { libc::dup2(f.as_raw_fd(), 2) };
            }
            let result = apply_theme_selection(theme);
            // Restore stderr
            if saved_stderr >= 0 {
                unsafe { libc::dup2(saved_stderr, 2) };
                unsafe { libc::close(saved_stderr) };
            }
            result?;
        } else {
            apply_theme_selection(theme)?;
            println!(
                "{} Theme auto-switched to '{}' (system appearance)",
                Symbols::SUCCESS,
                theme.name
            );
        }
        Ok(())
    } else if let Some(name) = theme_name {
        // Direct apply path: theme_name is canonical kebab-case
        let registry = ThemeRegistry::new()?;

        let theme = registry.get(&name).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", name))
        })?;

        apply_theme_selection(theme)?;

        println!("{} Theme switched to '{}'", Symbols::SUCCESS, theme.name);
        Ok(())
    } else {
        // Picker path: launch interactive picker
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)
    }
}
