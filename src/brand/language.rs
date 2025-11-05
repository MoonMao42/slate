/// Single source of truth for all user-facing text.
/// Future i18n: translate this module only.
pub struct Language;

impl Language {
    // Setup wizard (playful per)
    pub const SETUP_WELCOME: &str = "✦ slate — beautiful terminal in 30 seconds";
    pub const SETUP_DETECTING: &str = "Detecting installed tools...";
    pub const SETUP_INSTALLING: &str = "Installing {tool}...";
    pub const SETUP_FONT_SELECT: &str = "Select Nerd Font (recommended):";
    pub const SETUP_THEME_SELECT: &str = "Choose color scheme:";
    pub const SETUP_REVIEW: &str = "Review and confirm:";
    pub const SETUP_APPLYING: &str = "Applying configuration...";
    pub const SETUP_COMPLETE: &str = "✦ Your terminal is now beautiful!";

    // Tool selling points (one-liner visual value per)
    pub const PITCH_GHOSTTY: &str = "Makes your terminal glow";
    pub const PITCH_STARSHIP: &str = "Transforms your prompt";
    pub const PITCH_BAT: &str = "Beautiful code syntax highlighting";
    pub const PITCH_DELTA: &str = "Stunning git diffs";
    pub const PITCH_EZA: &str = "Colorful file listing";
    pub const PITCH_LAZYGIT: &str = "Git client that shines";
    pub const PITCH_FASTFETCH: &str = "System info with style";
    pub const PITCH_ZSH_SYNTAX: &str = "Code highlighting in your shell";
    pub const PITCH_TMUX: &str = "Themed terminal multiplexer";
    pub const PITCH_ALACRITTY: &str = "GPU-accelerated terminal beauty";

    // Daily commands (minimal per)
    pub const SET_SUCCESS: &str = "✓ {theme}";
    pub const STATUS_LABEL_CURRENT: &str = "current:";
    pub const STATUS_LABEL_TERMINAL: &str = "terminal:";
    pub const STATUS_LABEL_FONT: &str = "font:";
    pub const LIST_HEADER: &str = "Available themes";
    pub const RESTORE_SUCCESS: &str = "✓ Configuration restored";

    // Status indicators (per)
    pub const INSTALLED: &str = "✓ installed";
    pub const NOT_INSTALLED: &str = "○ not installed";
    pub const FAILED: &str = "✗ failed";

    // Error messages (professional + actionable per)
    pub fn error_tool_not_installed(tool: &str) -> String {
        format!(
            "{} is not installed. Run 'slate setup' to configure it.",
            tool
        )
    }

    pub fn error_config_not_found(tool: &str, path: &str) -> String {
        format!(
            "Config for {} not found at {}.\nTry running 'slate setup' to initialize it.",
            tool, path
        )
    }

    pub fn error_file_write(path: &str, reason: &str) -> String {
        format!(
            "Failed to write config to {}. Reason: {}\nVerify you have write permissions.",
            path, reason
        )
    }

    pub fn error_permission_denied(path: &str) -> String {
        format!(
            "Permission denied: {}.\nCheck file ownership and permissions.",
            path
        )
    }

    pub fn error_invalid_theme(theme: &str) -> String {
        format!(
            "Theme '{}' not found. Run 'slate list' to see available themes.",
            theme
        )
    }

    pub fn error_backup_failed(reason: &str) -> String {
        format!(
            "Failed to create backup. Reason: {}\nConfig was not modified.",
            reason
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_messages_exist() {
        assert!(!Language::SETUP_WELCOME.is_empty());
        assert!(!Language::SETUP_COMPLETE.is_empty());
    }

    #[test]
    fn test_error_messages_format() {
        let msg = Language::error_tool_not_installed("ghostty");
        assert!(msg.contains("ghostty"));
        assert!(msg.contains("setup"));
    }

    #[test]
    fn test_status_indicators() {
        assert_eq!(Language::INSTALLED, "✓ installed");
        assert_eq!(Language::NOT_INSTALLED, "○ not installed");
        assert_eq!(Language::FAILED, "✗ failed");
    }
}
