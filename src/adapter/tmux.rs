//! tmux adapter with marker block system for .tmux.conf theming.
//! Per and tmux uses source-file directive to include managed config.
//! This adapter uses the MarkerBlock module for safe, validated editing.
//! Detects tmux installation but doesn't require it (optional tool).

use crate::adapter::{ToolAdapter, ApplyStrategy, marker_block};
use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// tmux adapter implementing v2 ToolAdapter trait.
pub struct TmuxAdapter;

impl TmuxAdapter {
    /// Path to ~/.tmux.conf (integration file)
    fn tmux_conf_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".tmux.conf"))
    }

    /// Render tmux status bar color configuration
    /// Maps palette to tmux status-style, window-status, and pane-border colors
    fn render_tmux_colors(theme: &ThemeVariant) -> String {
        let palette = &theme.palette;

        format!(
            "# tmux status bar colors managed by slate\n\
             set -g status-style \"bg={} fg={}\"\n\
             set -g window-status-current-style \"bg={} fg={} bold\"\n\
             set -g pane-border-style \"fg={}\"\n\
             set -g pane-active-border-style \"fg={}\"\n",
            palette.background,
            palette.foreground,
            palette.blue,      // accent for active window
            palette.foreground,
            palette.black,      // muted for inactive panes
            palette.blue,       // accent for active pane
        )
    }

    /// Render managed block with source-file directive
    fn render_tmux_block(managed_path: &PathBuf) -> String {
        let managed_str = managed_path.display().to_string();
        format!(
            "{}\nsource-file {}\n{}\n",
            marker_block::START,
            managed_str,
            marker_block::END
        )
    }
}

impl ToolAdapter for TmuxAdapter {
    fn tool_name(&self) -> &'static str {
        "tmux"
    }

    fn is_installed(&self) -> Result<bool> {
        // Check if tmux binary exists in PATH
        // This is optional tool: if not found, return false (no error)
        Ok(which::which("tmux").is_ok())
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::tmux_conf_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/tmux")
        } else {
            PathBuf::from(".config/slate/managed/tmux")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render tmux color configuration
        let tmux_colors = Self::render_tmux_colors(theme);

        // Write managed colors file
        let config_mgr = ConfigManager::new()?;
        config_mgr.write_managed_file("tmux", "colors.conf", &tmux_colors)?;

        // Read current .tmux.conf
        let tmux_conf_path = Self::tmux_conf_path()?;
        let tmux_content = if tmux_conf_path.exists() {
            fs::read_to_string(&tmux_conf_path)?
        } else {
            String::new()
        };

        // Validate marker block state before modifying
        marker_block::validate_block_state(&tmux_content)?;

        // Build new marker block with source-file directive
        let managed_colors_path = self.managed_config_path().join("colors.conf");
        let new_block = Self::render_tmux_block(&managed_colors_path);

        // Upsert managed block
        let updated_content = marker_block::upsert_managed_block(&tmux_content, &new_block);

        // Write back to .tmux.conf (atomic per)
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;
        let mut file = AtomicWriteFile::open(&tmux_conf_path)?;
        file.write_all(updated_content.as_bytes())?;
        file.commit()?;

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Attempt to reload tmux server if running
        let result = Command::new("tmux")
            .args(&["source-file", "~/.tmux.conf"])
            .output();

        match result {
            Ok(output) if output.status.success() => Ok(()),
            Ok(_) => Err(SlateError::ReloadFailed(
                "tmux".to_string(),
                "tmux reload failed. Check .tmux.conf syntax.".to_string(),
            )),
            Err(_) => Err(SlateError::ReloadFailed(
                "tmux".to_string(),
                "tmux not running or not found.".to_string(),
            )),
        }
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
        let adapter = TmuxAdapter;
        assert_eq!(adapter.tool_name(), "tmux");
    }

    #[test]
    fn test_apply_strategy() {
        let adapter = TmuxAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_render_tmux_colors() {
        let theme = create_test_theme();
        let output = TmuxAdapter::render_tmux_colors(&theme);

        assert!(output.contains("status-style"));
        assert!(output.contains("window-status-current-style"));
        assert!(output.contains("pane-border-style"));
        assert!(output.contains("#000000")); // background
        assert!(output.contains("#ffffff")); // foreground
    }

    #[test]
    fn test_render_tmux_block() {
        let managed_path = PathBuf::from("/home/user/.config/slate/managed/tmux/colors.conf");
        let output = TmuxAdapter::render_tmux_block(&managed_path);

        assert!(output.contains(marker_block::START));
        assert!(output.contains(marker_block::END));
        assert!(output.contains("source-file"));
        assert!(output.contains(".config/slate/managed/tmux/colors.conf"));
    }

    #[test]
    fn test_is_installed() {
        let adapter = TmuxAdapter;
        let _result = adapter.is_installed();
    }
}
