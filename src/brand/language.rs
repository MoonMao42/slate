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
    pub const SETUP_QUICK_PENDING: &str = "Quick setup mode lands in .";
    pub const SETUP_INTERACTIVE_PENDING: &str = "Interactive setup wizard lands in .";

    // Receipt and completion polish (for 02-05 polish pass)
    pub const RECEIPT_HEADER: &str = "Review your setup:";
    pub const RECEIPT_INSTALL_SECTION: &str = "Install";
    pub const RECEIPT_FONT_SECTION: &str = "Font";
    pub const RECEIPT_THEME_SECTION: &str = "Theme";
    pub const RECEIPT_TERMINAL_SECTION: &str = "Terminal";
    pub const RECEIPT_FOOTER: &str = "Ready to apply? This will create backups first.";

    pub const COMPLETION_TIME_TAKEN: &str = "Time to dopamine:";
    pub const COMPLETION_NEXT_STEPS: &str = "What's next:";
    pub const COMPLETION_ACTIVATION_NOTE: &str =
        "Note: Changes may require a new terminal window to take full effect.";
    pub const COMPLETION_CALL_TO_ACTION: &str =
        "Open a fresh terminal to see your new setup shine!";

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
    pub const SET_PICKER_PENDING: &str =
        "Interactive theme picker coming soon. For now, use: slate set <theme-name>";
    pub const STATUS_PENDING: &str = "Status display lands in .";
    pub const LIST_PENDING: &str = "Theme listing lands in .";
    pub const RESTORE_PICKER_PENDING: &str = "Restore point selection lands in .";

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

    pub fn set_pending_theme(theme: &str) -> String {
        format!("{} lands in .", theme)
    }

    pub fn restore_pending_backup(backup_id: &str) -> String {
        format!("Restoring backup: {} lands in .", backup_id)
    }

    // Polish-pass formatting helpers
    pub fn receipt_line(label: &str, value: &str) -> String {
        format!("  → {}: {}", label, value)
    }

    pub fn completion_with_timing(message: &str, duration_ms: u64) -> String {
        format!("{} ({}ms)", message, duration_ms)
    }

    pub fn activation_guidance(tool: &str, activation_type: &str) -> String {
        format!(
            "  {} {} — {}",
            match activation_type {
                "immediate" => "✓",
                "new_window" => "➔",
                "restart" => "⟳",
                _ => "•",
            },
            tool,
            activation_type
        )
    }

    /// Hub entry messages 
    pub const HUB_WELCOME: &str = "✦ Welcome to slate. Let's set it up.";
    pub const HUB_TITLE: &str = "✦ slate";
    pub const HUB_WHAT_TO_DO: &str = "What would you like to do?";
    pub const AUTO_CONFIGURED: &str = "✓ Auto theme configured. Run slate set --auto to apply.";
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
    fn test_receipt_messages_exist() {
        assert!(!Language::RECEIPT_HEADER.is_empty());
        assert!(!Language::RECEIPT_INSTALL_SECTION.is_empty());
        assert!(!Language::RECEIPT_FOOTER.is_empty());
    }

    #[test]
    fn test_completion_messages_exist() {
        assert!(!Language::COMPLETION_TIME_TAKEN.is_empty());
        assert!(!Language::COMPLETION_NEXT_STEPS.is_empty());
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

    #[test]
    fn test_placeholder_formatters() {
        assert!(Language::set_pending_theme("catppuccin-mocha").contains("catppuccin-mocha"));
        assert!(Language::restore_pending_backup("backup-1").contains("backup-1"));
    }

    #[test]
    fn test_receipt_line_format() {
        let line = Language::receipt_line("Font", "JetBrains Mono");
        assert!(line.contains("Font"));
        assert!(line.contains("JetBrains Mono"));
        assert!(line.contains("→"));
    }

    #[test]
    fn test_completion_with_timing() {
        let msg = Language::completion_with_timing("Setup complete", 850);
        assert!(msg.contains("850ms"));
        assert!(msg.contains("Setup complete"));
    }

    #[test]
    fn test_activation_guidance_immediate() {
        let guidance = Language::activation_guidance("Starship", "immediate");
        assert!(guidance.contains("Starship"));
        assert!(guidance.contains("immediate"));
    }

    #[test]
    fn test_activation_guidance_new_window() {
        let guidance = Language::activation_guidance("Ghostty colors", "new_window");
        assert!(guidance.contains("Ghostty colors"));
        assert!(guidance.contains("new_window"));
    }
}
