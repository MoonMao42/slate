/// Single source of truth for all user-facing text.
/// Future i18n: translate this module only.
pub struct Language;

impl Language {
    // Setup wizard (playful)
    pub const SETUP_WELCOME: &str = "✦ slate — beautiful terminal in 30 seconds";
    pub const SETUP_DETECTING: &str = "Detecting installed tools...";
    pub const SETUP_INSTALLING: &str = "Installing {tool}...";
    pub const SETUP_FONT_SELECT: &str = "Select Nerd Font (recommended):";
    pub const SETUP_THEME_SELECT: &str = "Choose color scheme:";
    pub const SETUP_REVIEW: &str = "Review and confirm:";
    pub const SETUP_APPLYING: &str = "Applying configuration...";
    pub const SETUP_COMPLETE: &str = "✦ Your terminal is now beautiful!";
    pub const SETUP_QUICK_PENDING: &str = "Quick setup mode lands in Phase 2.";
    pub const SETUP_INTERACTIVE_PENDING: &str = "Interactive setup wizard lands in Phase 2.";

    // Receipt and completion polish
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

    // Tool selling points (one-liner visual value)
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
    pub const PITCH_KITTY: &str = "Feature-rich GPU terminal";

    // Daily commands (minimal)
    pub const SET_SUCCESS: &str = "✓ {theme}";
    pub const STATUS_LABEL_CURRENT: &str = "current:";
    pub const STATUS_LABEL_TERMINAL: &str = "terminal:";
    pub const STATUS_LABEL_FONT: &str = "font:";
    pub const PREFLIGHT_HEADER: &str = "✓ Preflight Checks";
    pub const STATUS_PLATFORM_CAPABILITIES: &str = "Platform Capabilities";
    pub const LIST_HEADER: &str = "Available themes";
    pub const RESTORE_SUCCESS: &str = "✓ Configuration restored";
    pub const SET_PICKER_PENDING: &str =
        "Interactive theme picker coming soon. For now, use: slate set <theme-name>";
    pub const STATUS_PENDING: &str = "Status display lands in Phase 7.";
    pub const LIST_PENDING: &str = "Theme listing lands in Phase 7.";

    // Status indicators
    pub const INSTALLED: &str = "✓ installed";
    pub const NOT_INSTALLED: &str = "○ not installed";
    pub const FAILED: &str = "✗ failed";

    // Error messages (professional + actionable)
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
        format!("{} lands in Phase 5.", theme)
    }

    // Restore messages (real restore UX)
    pub const RESTORE_HEADER: &str = "Restore from a previous snapshot";
    pub const RESTORE_LIST_HEADER: &str = "Available restore points:";
    pub const RESTORE_NO_POINTS: &str = "No restore points found. Run 'slate setup' to create one.";
    pub const RESTORE_CHOOSE_POINT: &str = "Choose restore point to restore to:";
    pub const RESTORE_DELETED: &str = "✓ Restore point deleted";
    pub const RESTORE_LISTED: &str = "Restore points:";

    pub fn restore_point_summary(id: &str, theme: &str, count: usize) -> String {
        format!("  {} — {} ({} files)", id, theme, count)
    }

    pub fn restore_receipt_header(theme: &str) -> String {
        format!("✓ Restored to: {}", theme)
    }

    pub fn restore_receipt_detail(succeeded: usize, failed: usize) -> String {
        if failed == 0 {
            format!("{} file(s) restored successfully", succeeded)
        } else {
            format!("{} file(s) restored, {} failed", succeeded, failed)
        }
    }

    pub fn restore_receipt_failures(display_tool: &str, error: &str) -> String {
        format!("  ✗ {}: {}", display_tool, error)
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

    /// Hub entry messages ()
    pub const HUB_WELCOME: &str = "✦ Welcome to slate. Let's set it up.";
    pub const HUB_TITLE: &str = "✦ slate";
    pub const HUB_WHAT_TO_DO: &str = "What would you like to do?";
    pub const AUTO_CONFIGURED: &str = "✓ Auto theme configured. Run slate set --auto to apply.";

    // CLI surface
    pub const SLATE_SET_DEPRECATION_TIP: &str = "(i) Tip: 'slate set' is transitioning to 'slate theme'. Try 'slate theme <name>' next time.";

    /// Demo hint shown once per process after `slate setup` or `slate theme <id>`
    /// success (per D-C4). Brand-voiced, curiosity-lure — NOT `(i) Tip:` advisory tone.
    /// Start with the ✦ glyph; keep ≤76 chars so `Typography::explanation`
    /// (2-space indent) doesn't wrap at 80 cols.
    pub const DEMO_HINT: &str = "✦ See this palette come alive — run `slate demo`";

    /// Brand-voiced size-gate rejection for `slate demo`. Reports both the
    /// minimum required (80×24) and the actual terminal (cols, rows) so the
    /// user understands the gap.
    pub fn demo_size_error(cols: u16, rows: u16) -> String {
        format!(
            "✦ slate demo needs an 80×24 window to breathe. Your terminal is {cols}×{rows}. Resize and try again."
        )
    }

    // Hub menu labels
    pub const HUB_SWITCH_THEME: &str = "✦ Switch Theme";
    pub const HUB_PAUSE_AUTO_PICK: &str = "✦ Pause Auto & Pick Theme";
    pub const HUB_CHANGE_FONT: &str = "✦ Change Font";
    pub const HUB_TOGGLE_AUTO_ON: &str = "✦ Auto-Theme (enabled)";
    pub const HUB_TOGGLE_AUTO_OFF: &str = "✦ Auto-Theme (disabled)";
    pub const HUB_VIEW_STATUS: &str = "◆ View Status";
    pub const HUB_QUIT: &str = "Quit";
    pub const HUB_RESUME_AUTO: &str = "✦ Resume Auto";

    // Hub tool toggles
    pub const HUB_TOGGLE_FASTFETCH_ON: &str = "Fastfetch · on";
    pub const HUB_TOGGLE_FASTFETCH_OFF: &str = "Fastfetch · off";
    pub const HUB_RUN_SETUP: &str = "Run Setup";

    // Status line labels
    pub const STATUS_AUTO_WATCHER_RUNNING: &str = "enabled and running";
    pub const STATUS_AUTO_WATCHER_IDLE_GHOSTTY: &str =
        "enabled, waiting for the next Ghostty shell";
    pub const STATUS_AUTO_WATCHER_IDLE_OTHER: &str =
        "enabled, but not running — re-enable to restart it";
    pub const STATUS_AUTO_WATCHER_DISABLED: &str = "disabled";
    pub const STATUS_AUTO_WATCHER_DRIFT: &str =
        "disabled in config, but the watcher is still running";
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_restore_receipt_format() {
        let summary =
            Language::restore_point_summary("2026-04-09T10-00-00Z", "Catppuccin Mocha", 5);
        assert!(summary.contains("2026-04-09T10-00-00Z"));
        assert!(summary.contains("Catppuccin Mocha"));
        assert!(summary.contains("5"));
    }

    #[test]
    fn test_demo_hint_format() {
        let hint = Language::DEMO_HINT;
        assert!(hint.starts_with('✦'), "hint must start with ✦ glyph");
        assert!(hint.contains("slate demo"), "hint must mention `slate demo`");
        assert!(
            !hint.starts_with("(i)"),
            "hint must NOT use `(i) Tip:` advisory tone per D-C4"
        );
        assert!(
            hint.chars().count() <= 76,
            "hint is {} chars; must be ≤76 so 2-space-indent output doesn't wrap at 80 cols",
            hint.chars().count()
        );
    }

    #[test]
    fn test_demo_size_error_mentions_required_and_actual() {
        let msg = Language::demo_size_error(79, 23);
        assert!(msg.contains("80"), "error must mention minimum cols");
        assert!(msg.contains("79"), "error must include actual cols");
        assert!(msg.contains("23"), "error must include actual rows");
        assert!(
            msg.contains("slate demo"),
            "error must name the failing command"
        );
    }
}
