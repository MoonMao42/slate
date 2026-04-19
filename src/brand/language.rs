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

    // ────────────────────────────────────────────────────────────
    // Phase 16 (LS-03 / UX-03) — brand-voiced shell-integration copy
    // ────────────────────────────────────────────────────────────

    /// macOS variant of the reveal-framed new-shell reminder.
    /// Points to `⌘N` as the canvas where the new palette lives. Active voice;
    /// no "please"; no "you need to". ≤76 chars so 2-space indent fits 80 cols.
    pub const NEW_SHELL_REMINDER_MACOS: &str =
        "✦ ⌘N for a fresh shell — your new palette lives there";

    /// Non-macOS variant. Frames the new terminal (not "this one") as the
    /// canvas. Active voice; no "please"; no "you need to". ≤76 chars.
    pub const NEW_SHELL_REMINDER_LINUX: &str =
        "✦ Open a new terminal — your new palette lives there";

    /// UX-03 (D-D7): platform-aware reveal-framed reminder emitted at the tail
    /// of `slate setup` / `theme` / `font` / `config` when any successful
    /// adapter declared `RequiresNewShell`. Compile-time branch per RESEARCH
    /// §Pattern 7 — simpler than routing through `platform::packages` and
    /// keeps `Language` self-contained.
    pub fn new_shell_reminder() -> &'static str {
        if cfg!(target_os = "macos") {
            Self::NEW_SHELL_REMINDER_MACOS
        } else {
            Self::NEW_SHELL_REMINDER_LINUX
        }
    }

    /// LS-03 (D-B4): one-time macOS BSD-`ls` capability message emitted from
    /// the setup preflight when `gls` (GNU ls from coreutils) is absent.
    /// Shape: observation → consequence → `brew install coreutils`. Tone
    /// mirrors `demo_size_error`: gentle, brand-voiced, ends with the fix.
    /// Multi-line so it breathes inside the preflight printout block.
    pub fn ls_capability_message() -> &'static str {
        "✦ This macOS ships with BSD `ls`; the slate-managed LS_COLORS needs GNU `ls` to render.\n  Install it with `brew install coreutils` and your next shell lights up."
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

    // ────────────────────────────────────────────────────────────
    // Editor integration (Phase 17 — D-09 3-way consent prompt +
    // capability hints). Copy pulled verbatim from 17-RESEARCH.md
    // §Pattern 7 (prompt copy) + §Pattern 8 (capability hints).
    // Brand voice: playful, premium, never generic — no "please",
    // no "you need to".
    // ────────────────────────────────────────────────────────────

    pub const NVIM_CONSENT_HEADER: &str = "✦ slate can auto-switch your Neovim colors";

    pub const NVIM_CONSENT_OPTION_A: &str = "Add it for me (recommended — one-step done)";

    pub const NVIM_CONSENT_OPTION_B: &str = "Show me the line, I'll paste it myself";

    pub const NVIM_CONSENT_OPTION_C: &str = "Skip — I'll run `:colorscheme slate-…` manually";

    pub const NVIM_CONSENT_MARKER_COMMENT: &str =
        "-- slate-managed: keep or delete, safe either way";

    pub const NVIM_MISSING_HINT: &str =
        "tip: install Neovim (≥ 0.8) to let slate color your editor too → `brew install neovim`";

    pub const NVIM_TOO_OLD_HINT: &str =
        "tip: your Neovim is older than 0.8 — slate's editor adapter needs nvim_set_hl. \
         Upgrade via `brew upgrade neovim` to enable it.";
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
        assert!(
            hint.contains("slate demo"),
            "hint must mention `slate demo`"
        );
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

    // ────────────────────────────────────────────────────────────
    // Phase 16 Plan 03 — LS-03 / UX-03 brand-voice contract
    // ────────────────────────────────────────────────────────────

    #[test]
    fn ls_capability_message_shape() {
        let msg = Language::ls_capability_message();
        // Observation: mentions BSD or macOS
        let obs_ok = msg.contains("BSD") || msg.contains("macOS");
        assert!(
            obs_ok,
            "capability message must observe the macOS BSD ls situation: {msg:?}"
        );
        // Consequence: mentions LS_COLORS / coreutils / GNU
        let consequence_ok =
            msg.contains("LS_COLORS") || msg.contains("coreutils") || msg.contains("GNU");
        assert!(
            consequence_ok,
            "capability message must name the consequence (LS_COLORS / GNU / coreutils): {msg:?}"
        );
        // Fix: ends with actionable brew command
        assert!(
            msg.contains("brew install coreutils"),
            "capability message must end with the brew install command: {msg:?}"
        );

        // Order check: observation → consequence → fix
        let obs_idx = msg
            .find("BSD")
            .or_else(|| msg.find("macOS"))
            .expect("observation token present");
        let consequence_idx = msg
            .find("LS_COLORS")
            .or_else(|| msg.find("coreutils"))
            .or_else(|| msg.find("GNU"))
            .expect("consequence token present");
        let fix_idx = msg
            .find("brew install coreutils")
            .expect("fix token present");
        assert!(
            obs_idx <= consequence_idx,
            "observation must come before consequence"
        );
        assert!(
            consequence_idx <= fix_idx,
            "consequence must come before fix"
        );
    }

    #[test]
    fn ls_capability_message_brand_voice() {
        let msg = Language::ls_capability_message();
        assert!(
            msg.starts_with('✦'),
            "capability message must start with ✦: {msg:?}"
        );
        let lower = msg.to_lowercase();
        assert!(
            !lower.contains("please"),
            "capability message must not contain 'please': {msg:?}"
        );
        assert!(
            !lower.contains("you need to"),
            "capability message must not contain 'you need to': {msg:?}"
        );
    }

    #[test]
    fn new_shell_reminder_copy_brand_voice() {
        let msg = Language::new_shell_reminder();
        assert!(msg.starts_with('✦'), "reminder must start with ✦: {msg:?}");
        let lower = msg.to_lowercase();
        assert!(
            !lower.contains("please"),
            "reminder must not contain 'please': {msg:?}"
        );
        assert!(
            !lower.contains("you need to"),
            "reminder must not contain 'you need to': {msg:?}"
        );
        let width = msg.chars().count();
        assert!(
            width <= 76,
            "reminder is {width} chars; must be ≤76 so 2-space indent fits 80 cols: {msg:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn new_shell_reminder_platform_aware_macos() {
        let msg = Language::new_shell_reminder();
        assert!(
            msg.contains("⌘N"),
            "macOS reminder must contain ⌘N: {msg:?}"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn new_shell_reminder_platform_aware_linux() {
        let msg = Language::new_shell_reminder();
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("terminal"),
            "non-macOS reminder must mention 'terminal': {msg:?}"
        );
        assert!(
            !msg.contains("⌘N"),
            "non-macOS reminder must not contain ⌘N: {msg:?}"
        );
    }

    #[test]
    fn new_shell_reminder_constants_differ() {
        assert_ne!(
            Language::NEW_SHELL_REMINDER_MACOS,
            Language::NEW_SHELL_REMINDER_LINUX,
            "platform reminder constants must differ"
        );
    }

    // ────────────────────────────────────────────────────────────
    // Phase 17 Plan 06 — D-09 consent prompt copy contract
    // ────────────────────────────────────────────────────────────

    /// Sanity check: every NVIM_* const is non-empty and the 3 option
    /// strings are distinct (so cliclack can render them as separate
    /// labels without collision).
    #[test]
    fn nvim_consent_constants_are_distinct_and_nonempty() {
        let all = [
            Language::NVIM_CONSENT_HEADER,
            Language::NVIM_CONSENT_OPTION_A,
            Language::NVIM_CONSENT_OPTION_B,
            Language::NVIM_CONSENT_OPTION_C,
            Language::NVIM_CONSENT_MARKER_COMMENT,
            Language::NVIM_MISSING_HINT,
            Language::NVIM_TOO_OLD_HINT,
        ];
        for s in &all {
            assert!(!s.is_empty(), "NVIM_* constant must not be empty");
        }
        assert_ne!(
            Language::NVIM_CONSENT_OPTION_A,
            Language::NVIM_CONSENT_OPTION_B
        );
        assert_ne!(
            Language::NVIM_CONSENT_OPTION_B,
            Language::NVIM_CONSENT_OPTION_C
        );
        assert_ne!(
            Language::NVIM_CONSENT_OPTION_A,
            Language::NVIM_CONSENT_OPTION_C
        );
    }

    /// Header must include the ✦ brand glyph + the word "Neovim" so
    /// users understand immediately which editor the prompt targets.
    #[test]
    fn nvim_consent_header_carries_brand_and_scope() {
        let header = Language::NVIM_CONSENT_HEADER;
        assert!(header.starts_with('✦'), "header must begin with ✦ glyph");
        assert!(
            header.contains("Neovim"),
            "header must name Neovim so the scope is unmistakable"
        );
    }

    /// Brand-voice contract (shared with the ls / reminder copy): no
    /// "please", no "you need to", no robotic tone. Applies to every
    /// piece of user-facing NVIM copy.
    #[test]
    fn nvim_copy_matches_brand_voice() {
        let surfaces = [
            Language::NVIM_CONSENT_HEADER,
            Language::NVIM_CONSENT_OPTION_A,
            Language::NVIM_CONSENT_OPTION_B,
            Language::NVIM_CONSENT_OPTION_C,
            Language::NVIM_MISSING_HINT,
            Language::NVIM_TOO_OLD_HINT,
        ];
        for msg in &surfaces {
            let lower = msg.to_lowercase();
            assert!(
                !lower.contains("please"),
                "nvim copy must not contain 'please': {msg:?}"
            );
            assert!(
                !lower.contains("you need to"),
                "nvim copy must not contain 'you need to': {msg:?}"
            );
        }
    }

    /// Marker comment is spliced above the `pcall(require, 'slate')`
    /// line in init.lua — must start with `--` so the resulting file
    /// remains syntactically valid Lua (Pitfall 4 contract).
    #[test]
    fn nvim_consent_marker_comment_is_lua_comment() {
        assert!(
            Language::NVIM_CONSENT_MARKER_COMMENT.starts_with("-- "),
            "marker comment must begin with `-- ` so init.lua stays valid Lua"
        );
    }

    /// Capability hints must name the fix (brew) so users have an
    /// actionable next step — matches the ls_capability_message shape.
    #[test]
    fn nvim_capability_hints_name_the_fix() {
        assert!(
            Language::NVIM_MISSING_HINT.contains("brew install neovim"),
            "missing-nvim hint must surface the brew install command"
        );
        assert!(
            Language::NVIM_TOO_OLD_HINT.contains("brew upgrade neovim"),
            "too-old-nvim hint must surface the brew upgrade command"
        );
        assert!(
            Language::NVIM_TOO_OLD_HINT.contains("0.8"),
            "too-old-nvim hint must name the minimum version (0.8)"
        );
    }
}
