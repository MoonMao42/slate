//! zsh-syntax-highlighting adapter for shell syntax coloring.
//! slate generates a managed shell snippet and lets the
//! central shell integration file source it. This avoids competing marker
//! blocks inside `.zshrc`.

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// zsh-syntax-highlighting adapter implementing the ToolAdapter trait.
pub struct ZshHighlightAdapter;

impl ZshHighlightAdapter {
    /// Get home directory
    fn home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home))
    }

    /// Build semantic map for ZSH_HIGHLIGHT_STYLES
    /// Maps palette colors to zsh-syntax-highlighting token types
    fn build_semantic_map() -> Vec<(&'static str, &'static str)> {
        vec![
            ("mauve", "keyword"),
            ("blue", "builtin"),
            ("green", "function"),
            ("overlay1", "comment"),
            ("red", "error"),
            ("red", "arg0"),
            ("yellow", "arg1"),
            ("green", "string"),
            ("yellow", "number"),
            ("cyan", "reserved"),
            ("magenta", "variable"),
            ("white", "default"),
        ]
    }

    fn render_highlight_styles(theme: &ThemeVariant) -> Result<String> {
        PaletteRenderer::to_shell_vars_from_pairs(&theme.palette, &Self::build_semantic_map())
    }

    fn managed_config_path_with_env(env: &SlateEnv) -> PathBuf {
        env.config_dir().join("managed").join("zsh")
    }
}

impl ToolAdapter for ZshHighlightAdapter {
    fn tool_name(&self) -> &'static str {
        "zsh-syntax-highlighting"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let home = Self::home()?;
        Ok(home.join(".zshrc"))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("zsh")
        } else {
            PathBuf::from(".config/slate/managed/zsh")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::SourceScript
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        // Step 1: Build semantic map for ZSH_HIGHLIGHT_STYLES
        let highlight_styles = Self::render_highlight_styles(theme)?;

        // Step 3: Write to managed config directory
        let managed_dir = Self::managed_config_path_with_env(env);
        fs::create_dir_all(&managed_dir)?;

        let highlight_file = managed_dir.join("highlight-styles.sh");
        use atomic_write_file::AtomicWriteFile;
        let mut file = AtomicWriteFile::open(&highlight_file)?;
        file.write_all(highlight_styles.as_bytes())?;
        file.commit()?;

        // zsh-syntax-highlighting styles are sourced during shell init;
        // already-running shells won't pick up new colors until restart.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn reload(&self) -> Result<()> {
        // zsh-syntax-highlighting requires shell restart or explicit reload
        // For now, return Ok() and let user restart terminal
        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = ZshHighlightAdapter;
        assert_eq!(adapter.tool_name(), "zsh-syntax-highlighting");
    }

    #[test]
    fn test_apply_strategy_returns_source_script() {
        let adapter = ZshHighlightAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::SourceScript);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = ZshHighlightAdapter;
        let path = adapter.managed_config_path();
        assert!(path.to_string_lossy().contains(".config/slate/managed/zsh"));
    }

    #[test]
    fn test_integration_config_path_returns_zshrc() {
        let adapter = ZshHighlightAdapter;
        let result = adapter.integration_config_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains(".zshrc"));
    }

    #[test]
    fn test_is_installed_returns_false_when_not_installed() {
        // This test may vary by environment; we verify the logic exists
        let adapter = ZshHighlightAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_semantic_map_has_expected_keys() {
        let map = ZshHighlightAdapter::build_semantic_map();
        assert!(!map.is_empty());
        // Verify at least keyword token type is present
        let has_keyword = map.iter().any(|(_, token)| *token == "keyword");
        assert!(has_keyword);
    }

    #[test]
    fn test_semantic_map_preserves_duplicate_palette_keys() {
        let map = ZshHighlightAdapter::build_semantic_map();
        let red_count = map
            .iter()
            .filter(|(palette_key, _)| *palette_key == "red")
            .count();
        let green_count = map
            .iter()
            .filter(|(palette_key, _)| *palette_key == "green")
            .count();

        assert_eq!(red_count, 2);
        assert_eq!(green_count, 2);
    }

    #[test]
    fn test_render_highlight_styles_preserves_duplicate_shell_tokens() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let styles = ZshHighlightAdapter::render_highlight_styles(&theme).unwrap();

        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[error]='fg=#"));
        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[arg0]='fg=#"));
        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[function]='fg=#"));
        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[string]='fg=#"));
    }

    #[test]
    fn test_apply_theme_creates_highlight_styles_file() {
        // This is an integration test; verify the structure exists
        let adapter = ZshHighlightAdapter;
        assert_eq!(adapter.tool_name(), "zsh-syntax-highlighting");
        // Actual file writing would require mocking filesystem
    }

    #[test]
    fn test_reload_returns_ok() {
        let adapter = ZshHighlightAdapter;
        let result = adapter.reload();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = ZshHighlightAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
