use crate::cli::auto_theme;
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeAppearance;

use super::state::PickerState;

pub(super) fn quick_save_auto(state: &PickerState, env: &SlateEnv) -> Result<String> {
    let config = ConfigManager::with_env(env)?;
    let theme = state.get_current_theme()?;
    let theme_id = state.get_current_theme_id();

    let message = match theme.appearance {
        ThemeAppearance::Dark => {
            config.write_auto_config(Some(theme_id), None)?;
            format!("✓ Auto Dark saved: {}", theme.name)
        }
        ThemeAppearance::Light => {
            config.write_auto_config(None, Some(theme_id))?;
            format!("✓ Auto Light saved: {}", theme.name)
        }
    };

    Ok(message)
}

pub(super) fn quick_resume_auto(state: &mut PickerState, env: &SlateEnv) -> Option<String> {
    let config = match ConfigManager::with_env(env) {
        Ok(config) => config,
        Err(error) => return Some(format!("(!) Resume auto failed: {}", error)),
    };

    let auto_theme_id = match auto_theme::resolve_auto_theme(env, &config) {
        Ok(id) => id,
        Err(error) => return Some(format!("(!) Resume auto failed: {}", error)),
    };

    if let Some(index) = state.theme_ids().iter().position(|id| id == &auto_theme_id) {
        state.jump_to_theme(index);
        let appearance = auto_theme::detect_system_appearance()
            .map(|appearance| match appearance {
                ThemeAppearance::Dark => "dark",
                ThemeAppearance::Light => "light",
            })
            .unwrap_or("?");
        return Some(format!(
            "→ resumed auto ({}): {}",
            appearance, auto_theme_id
        ));
    }

    None
}
