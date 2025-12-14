//! delta adapter migrated to v2 with marker block integration.
//! delta uses git config [include] blocks to reference managed config.
//! This adapter uses the MarkerBlock module from for safe, validated editing.
//! Preserves pager sync logic: synchronizes bat --theme and delta --syntax-theme.

use crate::adapter::{marker_block, ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::PathBuf;

/// delta adapter implementing v2 ToolAdapter trait.
pub struct DeltaAdapter;

impl DeltaAdapter {
    /// Path to ~/.gitconfig (integration file)
    fn gitconfig_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".gitconfig"))
    }

    /// Format include path for gitconfig
    fn format_gitconfig_include_path(config_path: &PathBuf) -> String {
        let escaped = config_path
            .display()
            .to_string()
            .replace('\\', r"\\")
            .replace('"', "\\\"");
        format!(r#""{}""#, escaped)
    }

    /// Render delta config in gitconfig INI format with marker blocks
    fn render_delta_config(_theme: &ThemeVariant, managed_path: &PathBuf) -> String {
        let managed_str = Self::format_gitconfig_include_path(managed_path);
        format!(
            "{}\n[include]\n\tpath = {}\n{}\n",
            marker_block::START,
            managed_str,
            marker_block::END
        )
    }

    /// Render delta color theme settings (for managed config file)
    fn render_delta_colors(theme: &ThemeVariant) -> String {
        let syntax_theme = theme
            .tool_refs
            .get("delta")
            .map(|s| s.as_str())
            .unwrap_or("catppuccin-mocha");
        format!(
            "[delta]\n\
             syntax-theme = {}\n\
             dark = true\n\
             line-numbers = true\n",
            syntax_theme
        )
    }
}

impl ToolAdapter for DeltaAdapter {
    fn tool_name(&self) -> &'static str {
        "delta"
    }

    fn is_installed(&self) -> Result<bool> {
        // delta requires git (git config delta.useDeltas true)
        // Check if git binary exists in PATH
        Ok(which::which("git").is_ok())
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::gitconfig_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        let home = env
            .as_ref()
            .and_then(|e| e.home().to_str().map(|s| s.to_string()));
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/delta")
        } else {
            PathBuf::from(".config/slate/managed/delta")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render delta colors config
        let delta_colors = Self::render_delta_colors(theme);

        // Write managed config
        let config_mgr = ConfigManager::new()?;
        config_mgr.write_managed_file("delta", "colors", &delta_colors)?;

        // Read current gitconfig
        let gitconfig_path = Self::gitconfig_path()?;
        let gitconfig_content = if gitconfig_path.exists() {
            fs::read_to_string(&gitconfig_path)?
        } else {
            String::new()
        };

        // Validate marker block state before modifying
        marker_block::validate_block_state(&gitconfig_content)?;

        // Build new marker block with include directive
        let managed_path = self.managed_config_path().join("colors");
        let new_block = Self::render_delta_config(theme, &managed_path);

        // Upsert managed block
        let updated_content = marker_block::upsert_managed_block(&gitconfig_content, &new_block);

        // Write back to gitconfig (atomic per)
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;
        let mut file = AtomicWriteFile::open(&gitconfig_path)?;
        file.write_all(updated_content.as_bytes())?;
        file.commit()?;

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // delta is a pager, no reload mechanism
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_palette() -> crate::theme::Palette {
        crate::theme::Palette {
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
            bg_dim: None,
            bg_darker: None,
            bg_darkest: None,
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
            extras: HashMap::new(),
        }
    }

    fn create_test_theme() -> ThemeVariant {
        ThemeVariant {
            id: "test".to_string(),
            name: "Test Theme".to_string(),
            family: "Test".to_string(),
            palette: create_test_palette(),
            tool_refs: HashMap::from([
                ("ghostty".to_string(), "test".to_string()),
                ("alacritty".to_string(), "test".to_string()),
                ("bat".to_string(), "test".to_string()),
                ("delta".to_string(), "test".to_string()),
                ("starship".to_string(), "test".to_string()),
                ("eza".to_string(), "test".to_string()),
                ("lazygit".to_string(), "test".to_string()),
                ("fastfetch".to_string(), "test".to_string()),
                ("tmux".to_string(), "test".to_string()),
                ("zsh_syntax_highlighting".to_string(), "test".to_string()),
            ]),
            appearance: crate::theme::ThemeAppearance::Dark,
            auto_pair: None,
        }
    }

    #[test]
    fn test_tool_name() {
        let adapter = DeltaAdapter;
        assert_eq!(adapter.tool_name(), "delta");
    }

    #[test]
    fn test_apply_strategy() {
        let adapter = DeltaAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_render_delta_config() {
        let theme = create_test_theme();
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/delta");
        let output = DeltaAdapter::render_delta_config(&theme, &managed_path);

        assert!(output.contains(marker_block::START));
        assert!(output.contains(marker_block::END));
        assert!(output.contains("[include]"));
        assert!(output.contains(".config/slate/managed/delta"));
    }

    #[test]
    fn test_render_delta_colors() {
        let theme = create_test_theme();
        let output = DeltaAdapter::render_delta_colors(&theme);

        assert!(output.contains("[delta]"));
        assert!(output.contains("syntax-theme = test"));
        assert!(output.contains("dark = true"));
    }

    #[test]
    fn test_is_installed() {
        let adapter = DeltaAdapter;
        let _result = adapter.is_installed();
    }
}
