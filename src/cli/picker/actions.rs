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
