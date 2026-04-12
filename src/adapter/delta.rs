//! delta adapter with marker block integration.
//! delta uses git config [include] blocks to reference managed config.
//! The adapter uses the MarkerBlock module for safe, validated editing,
//! and synchronizes bat --theme with delta --syntax-theme so the two agree.

use crate::adapter::{marker_block, ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::path::{Path, PathBuf};

/// delta adapter implementing the ToolAdapter trait.
pub struct DeltaAdapter;

impl DeltaAdapter {
    /// Path to ~/.gitconfig (integration file)
    fn gitconfig_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Self::gitconfig_path_with_env(&env)
    }

    fn gitconfig_path_with_env(env: &SlateEnv) -> Result<PathBuf> {
        Ok(env.home().join(".gitconfig"))
    }

    /// Format include path for gitconfig
    fn format_gitconfig_include_path(config_path: &Path) -> String {
        let escaped = config_path
            .display()
            .to_string()
            .replace('\\', r"\\")
            .replace('"', "\\\"");
        format!(r#""{}""#, escaped)
    }

    /// Render delta config in gitconfig INI format with marker blocks
    fn render_delta_config(_theme: &ThemeVariant, managed_path: &Path) -> String {
        let managed_str = Self::format_gitconfig_include_path(managed_path);
        format!(
            "{}\n[include]\n\tpath = {}\n{}\n",
            marker_block::START,
            managed_str,
            marker_block::END
        )
    }

    /// Render delta color theme settings (for managed config file).
    /// `dark` / `light` mirrors the active theme's appearance so delta picks
    /// the right default `+`/`-` line backgrounds and context-line styling.
    /// Hard-coding `dark = true` made every light-theme diff render with
    /// dark-terminal-tuned defaults — context lines washed out against the
    /// cream bg.
    fn render_delta_colors(theme: &ThemeVariant) -> String {
        let syntax_theme = theme
            .tool_refs
            .get("delta")
            .map(|s| s.as_str())
            .unwrap_or("catppuccin-mocha");
        let appearance_flag = match theme.appearance {
            crate::theme::ThemeAppearance::Light => "light = true",
            crate::theme::ThemeAppearance::Dark => "dark = true",
        };
        format!(
            "[delta]\n\
             syntax-theme = {}\n\
             {}\n\
             line-numbers = true\n",
            syntax_theme, appearance_flag
        )
    }
}

impl ToolAdapter for DeltaAdapter {
    fn tool_name(&self) -> &'static str {
        "delta"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::gitconfig_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("delta")
        } else {
            PathBuf::from(".config/slate/managed/delta")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render delta colors config
        let delta_colors = Self::render_delta_colors(theme);

        // Write managed config
        let config_mgr = ConfigManager::with_env(env)?;
        config_mgr.write_managed_file("delta", "colors", &delta_colors)?;

        let gitconfig_path = Self::gitconfig_path_with_env(env)?;
        let managed_path = config_mgr.managed_dir("delta").join("colors");
        let new_block = Self::render_delta_config(theme, &managed_path);
        marker_block::upsert_managed_block_file(&gitconfig_path, &new_block)?;

        // delta reads colors from git config on every invocation; no shell
        // restart required.
        Ok(ApplyOutcome::applied_no_shell())
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
        assert!(
            !output.contains("light = true"),
            "Dark theme must not emit light = true"
        );
    }

    #[test]
    fn test_render_delta_colors_emits_light_for_light_themes() {
        let mut theme = create_test_theme();
        theme.appearance = crate::theme::ThemeAppearance::Light;
        let output = DeltaAdapter::render_delta_colors(&theme);

        assert!(
            output.contains("light = true"),
            "Light theme must emit light = true so delta picks light-bg defaults; got:\n{output}"
        );
        assert!(
            !output.contains("dark = true"),
            "Light theme must not emit dark = true (was the bug — washed out context lines on cream Ghostty bg); got:\n{output}"
        );
    }

    #[test]
    fn test_is_installed() {
        let adapter = DeltaAdapter;
        let _result = adapter.is_installed();
    }
}
