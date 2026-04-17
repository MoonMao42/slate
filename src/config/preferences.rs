use super::{auto_theme, flags, AutoConfig, ConfigManager};
use crate::error::Result;

impl ConfigManager {
    /// Read auto.toml from ~/.config/slate/auto.toml if it exists.
    pub fn read_auto_config(&self) -> Result<Option<AutoConfig>> {
        auto_theme::read_auto_config(&self.base_path)
    }

    /// Write auto.toml with the specified dark and light themes.
    pub fn write_auto_config(
        &self,
        dark_theme: Option<&str>,
        light_theme: Option<&str>,
    ) -> Result<()> {
        auto_theme::write_auto_config(&self.base_path, dark_theme, light_theme)
    }

    /// Check if auto-theme is enabled via config.toml [auto_theme].enabled field.
    pub fn is_auto_theme_enabled(&self) -> Result<bool> {
        Ok(flags::config_flag(&self.base_path, "auto_theme", "enabled")?.unwrap_or(false))
    }

    /// Write auto-theme enabled flag to config.toml.
    pub fn set_auto_theme_enabled(&self, enabled: bool) -> Result<()> {
        flags::set_config_flag(&self.base_path, "auto_theme", "enabled", enabled)
    }

    /// Check if starship prompt initialization is enabled in shell integration.
    pub fn is_starship_enabled(&self) -> Result<bool> {
        Ok(flags::config_flag(&self.base_path, "tools", "starship")?.unwrap_or(true))
    }

    /// Enable or disable starship prompt initialization in shell integration.
    pub fn set_starship_enabled(&self, enabled: bool) -> Result<()> {
        flags::set_config_flag(&self.base_path, "tools", "starship", enabled)
    }

    /// Check if zsh syntax highlighting initialization is enabled in shell integration.
    pub fn is_zsh_highlighting_enabled(&self) -> Result<bool> {
        Ok(flags::config_flag(&self.base_path, "tools", "zsh_highlighting")?.unwrap_or(true))
    }

    /// Enable or disable zsh syntax highlighting initialization in shell integration.
    pub fn set_zsh_highlighting_enabled(&self, enabled: bool) -> Result<()> {
        flags::set_config_flag(&self.base_path, "tools", "zsh_highlighting", enabled)
    }

    /// Check if sound feedback is enabled.
    pub fn is_sound_enabled(&self) -> Result<bool> {
        Ok(flags::config_flag(&self.base_path, "preferences", "sound")?.unwrap_or(true))
    }

    /// Enable or disable sound feedback.
    pub fn set_sound_enabled(&self, enabled: bool) -> Result<()> {
        flags::set_config_flag(&self.base_path, "preferences", "sound", enabled)
    }

    /// Check if live preview is enabled for Ghostty reload in config.toml.
    pub fn is_live_preview_enabled(&self) -> Result<bool> {
        Ok(flags::config_flag(&self.base_path, "live_preview", "enabled")?.unwrap_or(false))
    }

    /// Check if live preview permission has been explicitly determined.
    pub fn is_live_preview_state_known(&self) -> Result<bool> {
        Ok(flags::config_flag(&self.base_path, "live_preview", "enabled")?.is_some())
    }

    /// Write live preview permission state to config.toml.
    pub fn set_live_preview_enabled(&self, enabled: bool) -> Result<()> {
        flags::set_config_flag(&self.base_path, "live_preview", "enabled", enabled)
    }
}
