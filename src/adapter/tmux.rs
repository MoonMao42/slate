//! tmux adapter with marker block system for .tmux.conf theming.
//!
//! tmux uses source-file directive to include managed config.
//! This adapter uses the MarkerBlock module for safe, validated editing.
//! Detects tmux installation but doesn't require it (optional tool).

use crate::adapter::{marker_block, ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::{Path, PathBuf};
use std::process::Command;

/// tmux adapter implementing the ToolAdapter trait.
pub struct TmuxAdapter;

impl TmuxAdapter {
    /// Path to ~/.tmux.conf (integration file)
    fn tmux_conf_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".tmux.conf"))
    }

    /// Render tmux status bar color configuration
    /// Maps palette to 7 tmux color elements:
    /// 1. status-style (bg/fg)
    /// 2. window-status-current-style (bg/fg)
    /// 3. pane-border-style (fg)
    /// 4. pane-active-border-style (fg)
    /// 5. message-style (bg/fg)
    /// 6. mode-style (bg/fg)
    /// 7. message-command-style (bg/fg)
    pub fn render_tmux_colors(theme: &ThemeVariant) -> String {
        let palette = &theme.palette;

        format!(
            "# tmux status bar colors managed by slate\n\
             set -g status-style \"bg={} fg={}\"\n\
             set -g window-status-current-style \"bg={} fg={} bold\"\n\
             set -g pane-border-style \"fg={}\"\n\
             set -g pane-active-border-style \"fg={}\"\n\
             set -g message-style \"bg={} fg={}\"\n\
             set -g mode-style \"bg={} fg={}\"\n\
             set -g message-command-style \"bg={} fg={}\"\n",
            palette.background, // status bg
            palette.foreground, // status fg
            palette.blue,       // active window bg (accent)
            palette.foreground, // active window fg
            palette.black,      // inactive pane fg (muted)
            palette.blue,       // active pane fg (accent)
            palette.background, // message bg
            palette.foreground, // message fg
            palette.black,      // mode selection bg (muted)
            palette.blue,       // mode selection fg (accent)
            palette.background, // message-command bg
            palette.foreground  // message-command fg
        )
    }

    /// Render managed block with source-file directive
    fn render_tmux_block(managed_path: &Path) -> String {
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
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::tmux_conf_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("tmux")
        } else {
            PathBuf::from(".config/slate/managed/tmux")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render tmux color configuration
        let tmux_colors = Self::render_tmux_colors(theme);

        // Write managed colors file
        let config_mgr = ConfigManager::new()?;
        config_mgr.write_managed_file("tmux", "colors.conf", &tmux_colors)?;

        let tmux_conf_path = Self::tmux_conf_path()?;
        let managed_colors_path = self.managed_config_path().join("colors.conf");
        let new_block = Self::render_tmux_block(&managed_colors_path);
        marker_block::upsert_managed_block_file(&tmux_conf_path, &new_block)?;

        // tmux source-file is issued in reload() against the running server
        // so existing sessions pick up the new colors immediately.
        Ok(ApplyOutcome::applied_no_shell())
    }

    fn reload(&self) -> Result<()> {
        // Attempt to reload tmux server if running
        let tmux_conf_path = Self::tmux_conf_path()?;
        let result = Command::new("tmux")
            .arg("source-file")
            .arg(&tmux_conf_path)
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
    use crate::theme::Palette;

    fn create_test_palette() -> Palette {
        Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            brand_accent: "#7287fd".to_string(),
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
            bg_dim: None,
            bg_darker: None,
            bg_darkest: None,
            extras: std::collections::HashMap::new(),
        }
    }

    fn create_test_theme() -> ThemeVariant {
        ThemeVariant {
            id: "test".to_string(),
            name: "Test Theme".to_string(),
            family: "Test".to_string(),
            palette: create_test_palette(),
            tool_refs: std::collections::HashMap::from([
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

        // Verify all 7 elements are present
        assert!(output.contains("status-style"));
        assert!(output.contains("window-status-current-style"));
        assert!(output.contains("pane-border-style"));
        assert!(output.contains("pane-active-border-style"));
        assert!(output.contains("message-style"));
        assert!(output.contains("mode-style"));
        assert!(output.contains("message-command-style"));

        // Verify color values
        assert!(output.contains("#000000")); // background
        assert!(output.contains("#ffffff")); // foreground

        // Verify 7 set -g directives (count)
        let count = output.matches("set -g").count();
        assert_eq!(count, 7, "Expected 7 set -g directives, found {}", count);
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
