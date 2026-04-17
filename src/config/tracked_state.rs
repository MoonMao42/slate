use super::{state_files, ConfigManager};
use crate::error::Result;
use std::fs;

impl ConfigManager {
    /// Update current theme tracking file.
    pub fn set_current_theme(&self, theme_id: &str) -> Result<()> {
        state_files::write_state_file(&self.current_theme_path(), theme_id)
    }

    /// Get current theme ID from tracking file.
    pub fn get_current_theme(&self) -> Result<Option<String>> {
        state_files::read_optional_state_file(&self.current_theme_path())
    }

    /// Persist user's chosen font family name.
    pub fn set_current_font(&self, font_family: &str) -> Result<()> {
        state_files::write_state_file(&self.current_font_path(), font_family)
    }

    /// Get the user's chosen font family name.
    pub fn get_current_font(&self) -> Result<Option<String>> {
        state_files::read_optional_state_file(&self.current_font_path())
    }

    /// Get the current opacity preset.
    pub fn get_current_opacity(&self) -> Result<Option<String>> {
        state_files::read_optional_state_file(&self.current_opacity_path())
    }

    /// Get the current opacity preset, parsing from file.
    pub fn get_current_opacity_preset(&self) -> Result<crate::opacity::OpacityPreset> {
        self.get_current_opacity()?
            .map(|value| value.parse::<crate::opacity::OpacityPreset>())
            .transpose()?
            .map_or(Ok(crate::opacity::OpacityPreset::Solid), Ok)
    }

    /// Set the current opacity preset, persisting to file.
    pub fn set_current_opacity_preset(&self, preset: crate::opacity::OpacityPreset) -> Result<()> {
        state_files::write_state_file(
            &self.current_opacity_path(),
            &preset.to_string().to_lowercase(),
        )
    }

    /// Check if fastfetch auto-run is enabled via marker file.
    pub fn has_fastfetch_autorun(&self) -> Result<bool> {
        Ok(self.base_path.join("autorun-fastfetch").exists())
    }

    /// Enable fastfetch auto-run by creating marker file atomically.
    pub fn enable_fastfetch_autorun(&self) -> Result<()> {
        state_files::write_state_file(&self.base_path.join("autorun-fastfetch"), "")
    }

    /// Disable fastfetch auto-run by deleting marker file.
    pub fn disable_fastfetch_autorun(&self) -> Result<()> {
        let path = self.base_path.join("autorun-fastfetch");
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}
