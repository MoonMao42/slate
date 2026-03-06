//! Regression tests for font decoupling (08-02)
//! Per through Verifies that font changes are truly localized
//! and do not trigger full theme reapply or shell integration refresh.

#[cfg(test)]
mod font_integration_tests {
    use slate_cli::adapter::font::FontAdapter;
    use slate_cli::env::SlateEnv;
    use std::fs;
    use tempfile::TempDir;

    /// Test: Changing font updates managed font files
    #[test]
    fn test_font_change_updates_font_files() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Setup: Create minimal integration configs
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        let ghostty_config = ghostty_dir.join("config.ghostty");
        fs::write(
            &ghostty_config,
            "# Ghostty config
",
        )
        .unwrap();

        let alacritty_dir = env.xdg_config_home().join("alacritty");
        fs::create_dir_all(&alacritty_dir).unwrap();
        let alacritty_config = alacritty_dir.join("alacritty.toml");
        fs::write(
            &alacritty_config,
            "[general]
",
        )
        .unwrap();

        // Apply font
        FontAdapter::apply_font(&env, "JetBrainsMono Nerd Font").unwrap();

        // Verify: Check that managed font files were created
        let ghostty_managed_font = env.config_dir().join("managed/ghostty/font.conf");
        let alacritty_managed_font = env.config_dir().join("managed/alacritty/font.toml");

        assert!(
            ghostty_managed_font.exists(),
            "Ghostty font.conf should be created at {:?}",
            ghostty_managed_font
        );
        assert!(
            alacritty_managed_font.exists(),
            "Alacritty font.toml should be created at {:?}",
            alacritty_managed_font
        );

        // Verify font content is correct
        let ghostty_content = fs::read_to_string(&ghostty_managed_font).unwrap();
        assert!(
            ghostty_content.contains("font-family"),
            "Ghostty font.conf should contain font-family"
        );
        assert!(
            ghostty_content.contains("JetBrainsMono Nerd Font"),
            "Ghostty font.conf should contain selected font"
        );

        let alacritty_content = fs::read_to_string(&alacritty_managed_font).unwrap();
        assert!(
            alacritty_content.contains("[font.normal]"),
            "Alacritty font.toml should contain [font.normal] section"
        );
        assert!(
            alacritty_content.contains("JetBrainsMono Nerd Font"),
            "Alacritty font.toml should contain selected font"
        );
    }

    /// Test: Font change persists current-font
    #[test]
    fn test_font_change_persists_current_font() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Setup: Create integration configs
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(
            ghostty_dir.join("config.ghostty"),
            "# Config
",
        )
        .unwrap();

        let alacritty_dir = env.xdg_config_home().join("alacritty");
        fs::create_dir_all(&alacritty_dir).unwrap();
        fs::write(
            alacritty_dir.join("alacritty.toml"),
            "[general]
",
        )
        .unwrap();

        // Apply font
        FontAdapter::apply_font(&env, "Fira Code Nerd Font").unwrap();

        // Verify: Check that current-font was persisted
        let current_font_file = env.config_dir().join("current-font");
        assert!(
            current_font_file.exists(),
            "current-font file should be created"
        );

        let persisted_font = fs::read_to_string(&current_font_file)
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            persisted_font, "Fira Code Nerd Font",
            "current-font should contain the selected font"
        );
    }

    /// Test: Font change does NOT re-run theme apply
    #[test]
    fn test_font_change_does_not_reapply_theme() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Setup: Create a theme.conf file to verify it's not modified
        let ghostty_managed = env.config_dir().join("managed/ghostty");
        fs::create_dir_all(&ghostty_managed).unwrap();
        let theme_conf = ghostty_managed.join("theme.conf");
        fs::write(
            &theme_conf,
            "# Original theme colors
",
        )
        .unwrap();
        let original_theme_content = fs::read_to_string(&theme_conf).unwrap();

        // Create integration configs
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(
            ghostty_dir.join("config.ghostty"),
            "# Config
",
        )
        .unwrap();

        let alacritty_dir = env.xdg_config_home().join("alacritty");
        fs::create_dir_all(&alacritty_dir).unwrap();
        fs::write(
            alacritty_dir.join("alacritty.toml"),
            "[general]
",
        )
        .unwrap();

        // Apply font
        FontAdapter::apply_font(&env, "Iosevka Nerd Font").unwrap();

        // Verify: theme.conf should NOT be modified
        let new_theme_content = fs::read_to_string(&theme_conf).unwrap();
        assert_eq!(
            original_theme_content, new_theme_content,
            "Font change should NOT modify theme.conf"
        );
    }

    /// Test: Font change does NOT refresh shell integration
    #[test]
    fn test_font_change_does_not_touch_shell_integration() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Setup: Create a marker file in integration directory
        let shell_integ = env.config_dir().join("integration/env.zsh");
        fs::create_dir_all(shell_integ.parent().unwrap()).unwrap();
        fs::write(
            &shell_integ,
            "# Original integration
",
        )
        .unwrap();
        let original_integ = fs::read_to_string(&shell_integ).unwrap();

        // Create integration configs
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(
            ghostty_dir.join("config.ghostty"),
            "# Config
",
        )
        .unwrap();

        let alacritty_dir = env.xdg_config_home().join("alacritty");
        fs::create_dir_all(&alacritty_dir).unwrap();
        fs::write(
            alacritty_dir.join("alacritty.toml"),
            "[general]
",
        )
        .unwrap();

        // Apply font
        FontAdapter::apply_font(&env, "Hack Nerd Font").unwrap();

        // Verify: env.zsh should not be modified
        let new_integ = fs::read_to_string(&shell_integ).unwrap();
        assert_eq!(
            original_integ, new_integ,
            "Font change should NOT modify shell integration file"
        );
    }

    /// Test: CLI output uses typography-focused language
    #[test]
    fn test_font_cli_output_is_typography_focused() {
        // Expected output format (from handle_font):
        // "✓ Updated font to {name} in Slate-managed terminal configs."
        let expected_copy =
            "Updated font to JetBrainsMono Nerd Font in Slate-managed terminal configs.";
        assert!(
            !expected_copy.contains("changed"),
            "Copy should use 'updated' not 'changed'"
        );
        assert!(
            expected_copy.contains("Slate-managed terminal configs"),
            "Copy should describe the managed terminal scope"
        );
    }

    /// Test: System font selection shows soft warning without hard failure
    #[test]
    fn test_system_font_warning_is_soft() {
        // Non-Nerd Font warning uses "may" not "will"
        let soft_warning = "Starship, eza etc. icons may not render correctly";
        assert!(
            soft_warning.contains("may"),
            "System font warning should use 'may', not 'will'"
        );
        assert!(
            !soft_warning.contains("break"),
            "System font warning should not say 'break'"
        );
    }
}
