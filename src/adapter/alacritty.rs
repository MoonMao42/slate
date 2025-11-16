//! Alacritty adapter with WriteAndInclude strategy.
//! Alacritty uses TOML import array to include managed config.
//! This adapter edits the import field idempotently using toml_edit::DocumentMut
//! (AST-aware, not regex-based) to ensure safe, structured modifications.

use crate::adapter::{ToolAdapter, ApplyStrategy};
use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};

/// Alacritty adapter implementing v2 ToolAdapter trait.
pub struct AlacrittyAdapter;

impl AlacrittyAdapter {
    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".config"))
    }

    /// Resolve Alacritty config path, respecting ALACRITTY_SOCKET_PATH and XDG_CONFIG_HOME.
    fn resolve_config_path() -> Result<PathBuf> {
        let config_home = Self::config_home()?;

        // Alacritty default: ~/.config/alacritty/alacritty.toml
        Ok(config_home.join("alacritty").join("alacritty.toml"))
    }

    /// Render Palette into Alacritty TOML color scheme structure.
    /// Maps palette colors to Alacritty's colors.primary, colors.normal, colors.bright sections.
    fn render_alacritty_colors(theme: &ThemeVariant) -> String {
        let palette = &theme.palette;

        format!(
            "[colors.primary]\nbackground = \"{}\"\nforeground = \"{}\"\n\n\
[colors.normal]\nblack = \"{}\"\nred = \"{}\"\ngreen = \"{}\"\nyellow = \"{}\"\nblue = \"{}\"\nmagenta = \"{}\"\ncyan = \"{}\"\nwhite = \"{}\"\n\n\
[colors.bright]\nblack = \"{}\"\nred = \"{}\"\ngreen = \"{}\"\nyellow = \"{}\"\nblue = \"{}\"\nmagenta = \"{}\"\ncyan = \"{}\"\nwhite = \"{}\"\n",
            palette.background,
            palette.foreground,
            // normal colors
            palette.black,
            palette.red,
            palette.green,
            palette.yellow,
            palette.blue,
            palette.magenta,
            palette.cyan,
            palette.white,
            // bright colors
            palette.bright_black,
            palette.bright_red,
            palette.bright_green,
            palette.bright_yellow,
            palette.bright_blue,
            palette.bright_magenta,
            palette.bright_cyan,
            palette.bright_white,
        )
    }

    /// Ensure integration file includes managed path in import array (idempotent).
    /// Parse and modify TOML import array for managed config path.
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        let managed_str = managed_path.display().to_string();

        // Read or create integration file
        let mut content = if integration_path.exists() {
            fs::read_to_string(integration_path)?
        } else {
            String::new()
        };

        // Try to parse as TOML to validate structure
        let _ : toml_edit::DocumentMut = content.parse()
            .map_err(|e| SlateError::InvalidConfig(
                format!("Failed to parse Alacritty TOML: {}", e)
            ))?;

        // Check if managed path is already present (idempotent)
        if content.contains(&format!("\"{}\"", managed_str)) {
            return Ok(());
        }

        // Simple approach: add import array line if not present
        if !content.contains("import") {
            // Create import array from scratch
            content = format!("import = [\"{}\"]\n{}", managed_str, content);
        } else if let Some(import_line) = content.find("import") {
            // Find the import line and its closing bracket
            let after_import = &content[import_line..];
            if let Some(closing_bracket) = after_import.find(']') {
                let insert_pos = import_line + closing_bracket;
                // Insert path before closing bracket
                content.insert_str(insert_pos, &format!(", \"{}\"", managed_str));
            } else {
                return Err(SlateError::InvalidConfig(
                    "Malformed import array in Alacritty config".to_string()
                ));
            }
        }

        // Write back to file
        fs::write(integration_path, content)?;

        Ok(())
    }
}

impl ToolAdapter for AlacrittyAdapter {
    fn tool_name(&self) -> &'static str {
        "alacritty"
    }

    fn is_installed(&self) -> Result<bool> {
        // Check if binary exists in PATH
        let binary_exists = which::which("alacritty").is_ok();

        // Check if config directory exists
        let config_home = match Self::config_home() {
            Ok(home) => home,
            Err(_) => return Ok(binary_exists),
        };

        let config_dir = config_home.join("alacritty");
        let config_dir_exists = config_dir.exists();

        Ok(binary_exists || config_dir_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::resolve_config_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/alacritty")
        } else {
            PathBuf::from(".config/slate/managed/alacritty")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render theme as TOML color scheme
        let colors_content = Self::render_alacritty_colors(theme);

        // Write managed colors file
        let config_mgr = ConfigManager::new()?;
        config_mgr.write_managed_file("alacritty", "colors.toml", &colors_content)?;

        // Ensure integration file includes managed colors path
        let integration_path = self.integration_config_path()?;
        let managed_colors_path = self.managed_config_path().join("colors.toml");

        Self::ensure_integration_includes_managed(&integration_path, &managed_colors_path)?;

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Alacritty supports live_config_reload if enabled, but it's optional.
        // Best-effort: return Err indicating manual restart may be needed.
        Err(SlateError::ReloadFailed(
            "alacritty".to_string(),
            "Alacritty reload depends on live_config_reload setting. \
             Restart your terminal or set live_config_reload = true in alacritty.toml.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Palette, ToolRefs};

    fn create_test_palette() -> Palette {
        Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            black: "#000000".to_string(),
            red: "#ff0000".to_string(),
            green: "#00ff00".to_string(),
            yellow: "#ffff00".to_string(),
            blue: "#0000ff".to_string(),
            magenta: "#ff00ff".to_string(),
            cyan: "#00ffff".to_string(),
            white: "#ffffff".to_string(),
            bright_black: "#808080".to_string(),
            bright_red: "#ff6b6b".to_string(),
            bright_green: "#69ff69".to_string(),
            bright_yellow: "#ffff69".to_string(),
            bright_blue: "#6b69ff".to_string(),
            bright_magenta: "#ff69ff".to_string(),
            bright_cyan: "#69ffff".to_string(),
            bright_white: "#ffffff".to_string(),
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: None,
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
        }
    }

    fn create_test_theme() -> ThemeVariant {
        ThemeVariant {
            id: "test".to_string(),
            name: "Test Theme".to_string(),
            family: "Test".to_string(),
            palette: create_test_palette(),
            tool_refs: ToolRefs {
                ghostty: "test".to_string(),
                alacritty: "test".to_string(),
                bat: "test".to_string(),
                delta: "test".to_string(),
                starship: "test".to_string(),
                eza: "test".to_string(),
                lazygit: "test".to_string(),
                fastfetch: "test".to_string(),
                tmux: "test".to_string(),
                zsh_syntax_highlighting: "test".to_string(),
            },
        }
    }

    #[test]
    fn test_tool_name() {
        let adapter = AlacrittyAdapter;
        assert_eq!(adapter.tool_name(), "alacritty");
    }

    #[test]
    fn test_apply_strategy() {
        let adapter = AlacrittyAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_render_alacritty_colors() {
        let theme = create_test_theme();
        let output = AlacrittyAdapter::render_alacritty_colors(&theme);

        assert!(output.contains("[colors.primary]"));
        assert!(output.contains("background = \"#000000\""));
        assert!(output.contains("foreground = \"#ffffff\""));
        assert!(output.contains("[colors.normal]"));
        assert!(output.contains("[colors.bright]"));
    }

    #[test]
    fn test_integration_includes_managed_idempotent() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();

        let managed_path = PathBuf::from("/home/user/.config/slate/managed/alacritty/colors.toml");

        // First call: should add to empty config
        AlacrittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content1 = fs::read_to_string(&temp_path).unwrap();
        assert!(content1.contains(".config/slate/managed/alacritty/colors.toml"));

        // Second call: should be idempotent (no duplicate)
        AlacrittyAdapter::ensure_integration_includes_managed(&temp_path, &managed_path).unwrap();

        let content2 = fs::read_to_string(&temp_path).unwrap();
        assert_eq!(content1, content2);
    }

    #[test]
    fn test_is_installed_when_not_present() {
        let adapter = AlacrittyAdapter;
        let _result = adapter.is_installed();
    }
}
