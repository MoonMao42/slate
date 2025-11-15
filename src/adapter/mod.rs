use std::path::PathBuf;
use crate::error::Result;
use crate::theme::ThemeVariant;

pub mod registry;
pub mod marker_block;
pub mod bat;
pub mod ghostty;
pub mod eza;

/// How a tool includes external configuration files.
/// Variants match real-world tool behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyStrategy {
    /// Write managed file + add include directive to integration file
    /// Tools: Alacritty (TOML include path), delta (git config [include]), tmux (source-file), lazygit (LG_CONFIG_FILE)
    WriteAndInclude,

    /// Modify specific fields in tool's integration config, preserving format
    /// Tools: Ghostty (edit color blocks directly), Starship (edit palette field)
    EditInPlace,

    /// Set environment variables emitted by `slate init`
    /// Tools: bat (BAT_THEME env var), eza (EZA_CONFIG_DIR)
    EnvironmentVariable,

    /// Source a generated script emitted by `slate init`
    /// Tools: zsh-syntax-highlighting (source ~/.config/slate/zsh-highlight.zsh)
    SourceScript,
}

/// Trait all tool adapters must implement.
/// Adapters are responsible for:
/// - Detecting if tool is installed
/// - Knowing how to apply themes (include mechanism)
/// - Respecting three-tier config: managed/ (slate owns) + integration (entry point) + user/ (never touch)
pub trait ToolAdapter: Send + Sync {
    /// Tool identifier (e.g., "ghostty", "starship", "bat")
    fn tool_name(&self) -> &'static str;

    /// Check if tool is installed.
    /// Must verify: binary in PATH AND config directory exists
    /// adapter is responsible for robust detection
    fn is_installed(&self) -> Result<bool>;

    /// Path to tool's primary/integration config file.
    /// This is the file user owns and modifies (entry point for includes).
    /// Example: ~/.config/ghostty/config or ~/.config/starship.toml
    fn integration_config_path(&self) -> Result<PathBuf>;

    /// Path where slate writes managed configuration.
    /// This is read-only from user perspective; slate regenerates it.
    /// Example: ~/.config/slate/managed/ghostty/ or ~/.config/slate/managed/starship/
    /// hardcode ~/.config/slate/ not ProjectDirs
    fn managed_config_path(&self) -> PathBuf;

    /// Strategy for applying theme to this tool.
    /// Determines whether adapter writes separate file or edits integration file.
    /// one of four variants
    fn apply_strategy(&self) -> ApplyStrategy;

    /// Apply theme to tool's configuration.
    /// Must:
    /// 1. Write theme data to managed_config_path() 
    /// 2. Ensure integration file includes/references managed path (per apply_strategy)
    /// 3. Never modify user/ directory 
    /// 4. Be idempotent (running twice produces same result)
    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()>;

    /// Hot-reload mechanism for this tool.
    /// Allows theme changes to take effect without closing terminal.
    /// implementation varies by tool
    /// Returns Ok() if reload succeeded, Err if tool doesn't support or failed.
    fn reload(&self) -> Result<()> {
        // Default: no-op. Adapters override if they support reload.
        Ok(())
    }

    /// Get current theme applied to this tool (optional, for status).
    /// Adapters can leave unimplemented; returns None by default.
    fn get_current_theme(&self) -> Result<Option<String>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_strategy_variants() {
        // Verify all 4 variants exist and are distinct
        let write_include = ApplyStrategy::WriteAndInclude;
        let edit_in_place = ApplyStrategy::EditInPlace;
        let env_var = ApplyStrategy::EnvironmentVariable;
        let source_script = ApplyStrategy::SourceScript;

        // All variants should be distinguishable
        assert_ne!(write_include, edit_in_place);
        assert_ne!(env_var, source_script);
        assert_ne!(write_include, env_var);
        assert_ne!(edit_in_place, source_script);
    }

    #[test]
    fn test_apply_strategy_debug() {
        let strategy = ApplyStrategy::WriteAndInclude;
        let debug_str = format!("{:?}", strategy);
        assert!(debug_str.contains("WriteAndInclude"));
    }

    #[test]
    fn test_apply_strategy_clone_copy() {
        let strategy = ApplyStrategy::EditInPlace;
        let cloned = strategy;
        assert_eq!(strategy, cloned);
    }
}
