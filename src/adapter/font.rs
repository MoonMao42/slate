//! Nerd Font adapter for font detection and installation support.
//! Per through Detects installed Nerd Fonts on macOS and provides
//! installation command mapping. Scope: detect + install mapping only (no config writing).

use std::path::PathBuf;
use std::fs;
use crate::adapter::{ToolAdapter, ApplyStrategy};
use crate::error::Result;
use crate::theme::ThemeVariant;

/// Nerd Font adapter implementing v2 ToolAdapter trait.
pub struct FontAdapter;

impl FontAdapter {
    /// Get home directory
    fn home() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| crate::error::SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home))
    }

    /// Detect installed Nerd Fonts by scanning font directories
    /// Returns list of installed Nerd Font names (without extension)
    pub fn detect_installed_fonts() -> Result<Vec<String>> {
        let mut fonts = Vec::new();

        // Scan user fonts directory
        if let Ok(user_fonts) = fs::read_dir(Self::home()?.join("Library/Fonts")) {
            for entry in user_fonts.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.contains("NerdFont") || name.contains("Nerd Font") {
                        // Extract font name without extension
                        let font_name = name.split('.').next().unwrap_or(&name).to_string();
                        if !fonts.contains(&font_name) {
                            fonts.push(font_name);
                        }
                    }
                }
            }
        }

        // Scan system fonts directory
        if let Ok(sys_fonts) = fs::read_dir("/Library/Fonts") {
            for entry in sys_fonts.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.contains("NerdFont") || name.contains("Nerd Font") {
                        let font_name = name.split('.').next().unwrap_or(&name).to_string();
                        if !fonts.contains(&font_name) {
                            fonts.push(font_name);
                        }
                    }
                }
            }
        }

        Ok(fonts)
    }

    /// Map font name to brew cask name
    /// Example: "JetBrains Mono Nerd Font" -> "font-jetbrains-mono-nerd-font"
    pub fn font_to_cask_name(font_name: &str) -> String {
        // Remove "Nerd Font" suffix if present
        let base_name = font_name
            .strip_suffix(" Nerd Font")
            .unwrap_or(font_name)
            .trim();

        // Convert to kebab-case
        let kebab = base_name
            .to_lowercase()
            .replace(" ", "-")
            .replace("_", "-");

        // Ensure font- prefix and nerd-font suffix
        let cask_name = if kebab.starts_with("font-") {
            kebab
        } else {
            format!("font-{}", kebab)
        };

        if cask_name.ends_with("-nerd-font") {
            cask_name
        } else {
            format!("{}-nerd-font", cask_name)
        }
    }
}

impl ToolAdapter for FontAdapter {
    fn tool_name(&self) -> &'static str {
        "nerd-font"
    }

    fn is_installed(&self) -> Result<bool> {
        match Self::detect_installed_fonts() {
            Ok(fonts) => Ok(!fonts.is_empty()),
            Err(_) => {
                // Gracefully handle permission errors
                Ok(false)
            }
        }
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let home = Self::home()?;
        Ok(home.join("Library/Fonts"))
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate")
        } else {
            PathBuf::from(".config/slate")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::DetectAndInstall
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<()> {
        // Per design: Nerd Font adapter only handles detection and installation
        // No theme application needed (fonts are tool-independent)
        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Fonts don't need reload
        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // Return name of first installed Nerd Font, if any
        match Self::detect_installed_fonts() {
            Ok(fonts) => Ok(fonts.first().cloned()),
            Err(_) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = FontAdapter;
        assert_eq!(adapter.tool_name(), "nerd-font");
    }

    #[test]
    fn test_apply_strategy_returns_detect_and_install() {
        let adapter = FontAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::DetectAndInstall);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = FontAdapter;
        let path = adapter.managed_config_path();
        assert!(path.to_string_lossy().contains(".config/slate"));
    }

    #[test]
    fn test_integration_config_path_returns_fonts_dir() {
        let adapter = FontAdapter;
        let result = adapter.integration_config_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("Library/Fonts"));
    }

    #[test]
    fn test_apply_theme_returns_ok() {
        let adapter = FontAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let result = adapter.apply_theme(&theme);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reload_returns_ok() {
        let adapter = FontAdapter;
        let result = adapter.reload();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_current_theme_returns_option() {
        let adapter = FontAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        // Result may be None or Some depending on installed fonts
    }

    #[test]
    fn test_font_to_cask_name_jetbrains_mono() {
        let cask = FontAdapter::font_to_cask_name("JetBrains Mono Nerd Font");
        assert_eq!(cask, "font-jetbrains-mono-nerd-font");
    }

    #[test]
    fn test_font_to_cask_name_fira_code() {
        let cask = FontAdapter::font_to_cask_name("Fira Code Nerd Font");
        assert_eq!(cask, "font-fira-code-nerd-font");
    }

    #[test]
    fn test_font_to_cask_name_iosevka() {
        let cask = FontAdapter::font_to_cask_name("Iosevka Term Nerd Font");
        assert_eq!(cask, "font-iosevka-term-nerd-font");
    }

    #[test]
    fn test_font_to_cask_name_hack() {
        let cask = FontAdapter::font_to_cask_name("Hack Nerd Font");
        assert_eq!(cask, "font-hack-nerd-font");
    }

    #[test]
    fn test_is_installed_returns_result() {
        let adapter = FontAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
        // Result may be true or false depending on installed fonts
    }
}
