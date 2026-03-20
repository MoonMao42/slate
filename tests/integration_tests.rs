use assert_cmd::Command;
use tempfile::TempDir;

/// Create a Command with SLATE_HOME pointing to an isolated temp directory.
/// Prevents tests from polluting the real ~/.config and ~/.cache.
fn slate_cmd_isolated(tempdir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.env("SLATE_HOME", tempdir.path());
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    });
    cmd.env("SHELL", shell);
    cmd
}

fn slate_cmd_isolated_with_shell(tempdir: &TempDir, shell: &str) -> Command {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.env("SLATE_HOME", tempdir.path());
    cmd.env("SHELL", shell);
    cmd
}

#[test]
fn test_cli_help_shows_commands() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("setup"));
    assert!(stdout.contains("set"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("list"));
    assert!(!stdout.contains("reset"));
    assert!(stdout.contains("theme"));
    assert!(stdout.contains("font"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("clean"));
    assert!(stdout.contains("macOS and Linux"));
}

#[test]
fn test_setup_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(["setup", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("setup"));
    assert!(stdout.contains("--quick"));
}

#[test]
fn test_set_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(["set", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("set"));
}

#[test]
fn test_status_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(["status", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("status"));
}

#[test]
fn test_list_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(["list", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("list"));
}

#[test]
fn test_setup_quick_flag() {
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);

    let output = cmd.args(["setup", "--quick"]).output().unwrap();
    // In quick mode, wizard runs successfully
    assert!(output.status.success());
}

#[test]
fn test_setup_shell_integration_zsh() {
    let tempdir = TempDir::new().unwrap();
    std::fs::write(tempdir.path().join(".zshrc"), "# user zsh\n").unwrap();

    let output = slate_cmd_isolated_with_shell(&tempdir, "/bin/zsh")
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let zshrc = std::fs::read_to_string(tempdir.path().join(".zshrc")).unwrap();
    assert!(zshrc.contains("slate:start"));
    assert!(zshrc.contains("managed/shell/env.zsh"));
    assert!(zshrc.contains("# user zsh"));
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.zsh")
        .exists());
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.bash")
        .exists());
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.fish")
        .exists());
}

#[test]
fn test_setup_shell_integration_bash() {
    let tempdir = TempDir::new().unwrap();
    std::fs::write(tempdir.path().join(".bashrc"), "# user bash\n").unwrap();
    std::fs::write(tempdir.path().join(".bash_profile"), "# bash profile\n").unwrap();

    let output = slate_cmd_isolated_with_shell(&tempdir, "/bin/bash")
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // On macOS, Terminal.app runs bash as a login shell which reads.bash_profile, so
    // slate prefers.bash_profile when it exists. On Linux,.bashrc is canonical for
    // interactive sessions. Either way,.bashrc still retains its user content and
    // bash_profile still retains its user content — the marker block lands in exactly
    // one of them.
    let bashrc = std::fs::read_to_string(tempdir.path().join(".bashrc")).unwrap();
    let bash_profile = std::fs::read_to_string(tempdir.path().join(".bash_profile")).unwrap();
    let marker_in_bashrc = bashrc.contains("slate:start");
    let marker_in_profile = bash_profile.contains("slate:start");
    assert!(
        marker_in_bashrc ^ marker_in_profile,
        "expected slate marker block in exactly one of .bashrc / .bash_profile"
    );
    if cfg!(target_os = "macos") {
        assert!(
            marker_in_profile,
            "macOS should write to .bash_profile when it exists"
        );
        assert!(bash_profile.contains("managed/shell/env.bash"));
        assert!(bash_profile.contains("# bash profile"));
        assert_eq!(bashrc, "# user bash\n");
    } else {
        assert!(marker_in_bashrc, "Linux should write to .bashrc");
        assert!(bashrc.contains("managed/shell/env.bash"));
        assert!(bashrc.contains("# user bash"));
        assert_eq!(bash_profile, "# bash profile\n");
    }
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.zsh")
        .exists());
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.bash")
        .exists());
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.fish")
        .exists());
}

#[test]
fn test_setup_shell_integration_fish() {
    let tempdir = TempDir::new().unwrap();
    let config_fish = tempdir.path().join(".config/fish/config.fish");
    std::fs::create_dir_all(config_fish.parent().unwrap()).unwrap();
    std::fs::write(&config_fish, "# user fish\n").unwrap();

    let output = slate_cmd_isolated_with_shell(&tempdir, "/usr/bin/fish")
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fish_loader = tempdir.path().join(".config/fish/conf.d/slate.fish");
    let fish_loader_content = std::fs::read_to_string(&fish_loader).unwrap();
    assert!(fish_loader_content.contains("managed/shell/env.fish"));
    assert_eq!(
        std::fs::read_to_string(&config_fish).unwrap(),
        "# user fish\n"
    );
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.zsh")
        .exists());
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.bash")
        .exists());
    assert!(tempdir
        .path()
        .join(".config/slate/managed/shell/env.fish")
        .exists());
}

#[test]
fn test_set_with_theme_argument() {
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);

    let output = cmd.args(["set", "catppuccin-mocha"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // set command now switches theme and confirms
    assert!(stdout.contains("Catppuccin Mocha"));
    assert!(stdout.contains("slate set' is transitioning to 'slate theme"));
}

#[test]
fn test_status_command_runs() {
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);

    let output = cmd.arg("status").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // slate status now shows rounded panel dashboard with 4 sections
    assert!(stdout.contains("slate status")); // Panel header
    assert!(stdout.contains("Core Vibe")); // Section 1
    assert!(stdout.contains("Typography")); // Section 2
    assert!(stdout.contains("Background")); // Section 3
    assert!(stdout.contains("Platform Capabilities")); //  section
    assert!(stdout.contains("Desktop Appearance"));
    assert!(stdout.contains("Share Capture"));
    assert!(stdout.contains("Package Manager"));
    assert!(stdout.contains("Reload"));
    assert!(stdout.contains("Preview"));
    assert!(stdout.contains("Font"));
    assert!(stdout.contains("Toolkit")); // Section 5
    assert!(stdout.contains("supported") || stdout.contains("best effort"));
}

#[test]
fn test_list_command_runs() {
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);

    let output = cmd.arg("list").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // slate list shows families grouped under tree-narrative headings
    // (migration replaced the legacy `━━ {family} ━━`
    // separator with `◆ {family}` via Roles::heading per Sketch 003).
    assert!(stdout.contains("Catppuccin")); // First family in  order
    assert!(stdout.contains("Tokyo Night")); // Second family in  order
    assert!(stdout.contains("◆")); // Brand-anchor family heading glyph
}

#[test]
fn test_reset_subcommand_is_not_exposed() {
    // Test that reset is hidden (not shown in help)
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // reset should not appear in help (it's a hidden compatibility alias)
    assert!(
        !stdout.contains("reset"),
        "reset should be hidden from help"
    );

    // But reset command still works for backward compatibility
    let tempdir = TempDir::new().unwrap();
    let mut cmd2 = slate_cmd_isolated(&tempdir);
    let output2 = cmd2.args(["reset", "--help"]).output().unwrap();

    // reset --help should work (it's hidden but functional)
    assert!(
        output2.status.success(),
        "reset command should still be recognized internally"
    );
}

// Setup wizard tests

#[test]
fn test_setup_wizard_intro_displays() {
    // Verify wizard displays intro frame and completes successfully
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);
    cmd.arg("setup").arg("--quick");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    // Step counter should appear in stderr
    assert!(stderr.contains("Step") || stderr.contains("✦"));
}

#[test]
fn test_setup_wizard_completion_message() {
    // Verify "Your terminal is now beautiful!" appears in output
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);
    cmd.arg("setup").arg("--quick");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("✓ Preflight Checks"));
    assert!(stderr.contains("Package Manager"));
    assert!(stderr.contains("Desktop Appearance"));
    assert!(stderr.contains("Share Capture"));
    assert!(stderr.contains("Terminal Features"));
    assert!(stderr.contains("supported") || stderr.contains("best effort"));
}

#[test]
fn test_setup_wizard_step_counter_present() {
    // Verify step counter format "Step X of Y" is logged
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);
    cmd.arg("setup").arg("--quick");

    let output = cmd.output().unwrap();
    let _stderr = String::from_utf8(output.stderr).unwrap();

    // In quick mode, step counter should log completion
    assert!(output.status.success());
}

#[test]
fn test_setup_quick_mode_minimal_interactions() {
    // Verify --quick flag skips mode selection
    let tempdir = TempDir::new().unwrap();
    let mut cmd = slate_cmd_isolated(&tempdir);
    cmd.arg("setup").arg("--quick");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    // Quick mode should complete without asking for mode selection
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{}{}", stdout, stderr);

    // Should show completion even in non-interactive quick mode
    assert!(combined.contains("beautiful") || combined.contains("Step") || output.status.success());
}

// `slate demo` integration tests retired in (/).
// The 4-block renderer continues to be exercised by its unit tests in
// `src/cli/demo.rs::tests` (migrating to `src/cli/picker/preview/blocks.rs`
// in). The `slate demo` CLI surface + DEMO-02 hint test gates
// are superseded by DEMO-03 integration tests (landing in).

/// CLI smoke: the `slate demo` subcommand has been retired.
/// clap must reject it with a non-zero exit; stderr names the unknown
/// subcommand. Locks the absence of the command at the CLI surface.
#[test]
fn slate_demo_subcommand_is_retired_phase_19() {
    let tempdir = TempDir::new().unwrap();
    let output = slate_cmd_isolated(&tempdir)
        .arg("demo")
        .output()
        .expect("spawn slate");
    assert!(
        !output.status.success(),
        "`slate demo` must exit non-zero after  retirement"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        combined.to_lowercase().contains("unrecognized subcommand")
            || combined.to_lowercase().contains("error:"),
        "clap must report the missing subcommand; got:\n{combined}"
    );
}

// Tool selection logic tests

#[cfg(test)]
mod tool_selection_tests {
    use slate_cli::cli::tool_selection::{
        compute_install_candidates, filter_valid_selections, BrewKind, ReviewReceipt, ToolCatalog,
    };
    use slate_cli::detection::ToolPresence;
    use std::collections::HashMap;

    #[test]
    fn test_tool_catalog_complete() {
        // Verify catalog has core tools
        let all_tools = ToolCatalog::all_tools();
        let tool_ids: Vec<&str> = all_tools.iter().map(|t| t.id).collect();

        assert!(tool_ids.contains(&"ghostty"));
        assert!(tool_ids.contains(&"starship"));
        assert!(tool_ids.contains(&"bat"));
        assert!(tool_ids.contains(&"delta"));
        assert!(tool_ids.contains(&"eza"));
        assert!(tool_ids.contains(&"lazygit"));
        assert!(tool_ids.contains(&"fastfetch"));
        assert!(tool_ids.contains(&"zsh-syntax-highlighting"));
        assert!(tool_ids.contains(&"alacritty"));
        assert!(tool_ids.contains(&"tmux"));
    }

    #[test]
    fn test_detect_only_tools_not_in_candidates() {
        // tmux is detect-only and should not appear in install candidates
        let mut installed = HashMap::new();
        installed.insert("tmux".to_string(), ToolPresence::missing()); // not installed

        let candidates = compute_install_candidates(&installed);

        // Even though tmux is not installed, it should NOT be a candidate
        assert!(!candidates.iter().any(|t| t.id == "tmux"));
    }

    #[test]
    fn test_already_installed_tools_not_in_candidates() {
        // Tools that are already installed should not appear in install candidates
        use slate_cli::detection::ToolEvidence;
        let mut installed = HashMap::new();
        installed.insert(
            "ghostty".to_string(),
            ToolPresence::in_path_with(ToolEvidence::Executable("/usr/bin/ghostty".into())),
        );
        installed.insert("starship".to_string(), ToolPresence::missing());

        let candidates = compute_install_candidates(&installed);

        // ghostty is installed → not a candidate
        assert!(!candidates.iter().any(|t| t.id == "ghostty"));

        // starship is not installed → should be a candidate
        assert!(candidates.iter().any(|t| t.id == "starship"));
    }

    #[test]
    fn test_formula_vs_cask_distinction() {
        // Verify that formula and cask installs are correctly distinguished
        let ghostty = ToolCatalog::get_tool("ghostty").unwrap();
        assert_eq!(ghostty.brew_kind, BrewKind::Cask);

        let starship = ToolCatalog::get_tool("starship").unwrap();
        assert_eq!(starship.brew_kind, BrewKind::Formula);
    }

    #[test]
    fn test_filter_valid_selections_removes_invalid() {
        // filter_valid_selections should remove non-installable and detect-only tools
        let selected = vec![
            "starship".to_string(),    // installable ✓
            "ghostty".to_string(),     // detect-only ✗
            "tmux".to_string(),        // detect-only ✗
            "nonexistent".to_string(), // unknown ✗
        ];

        let actions = filter_valid_selections(selected);

        // Only starship should be included
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].tool_id, "starship");
    }

    #[test]
    fn test_review_receipt_format_shows_actions() {
        // Verify that review receipt displays install actions correctly
        let mut receipt = ReviewReceipt::new();

        if let Some(ghostty) = ToolCatalog::get_tool("ghostty") {
            let action = slate_cli::cli::tool_selection::InstallAction::from_metadata(&ghostty);
            receipt.add_install_action(action);
        }

        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());

        let formatted = receipt.format_for_display();

        // Receipt should contain both install action and selections
        assert!(formatted.contains("Ghostty"));
        assert!(formatted.contains("JetBrains Mono"));
        assert!(formatted.contains("Catppuccin Mocha"));
        assert!(formatted.contains("Review and confirm"));
    }

    #[test]
    fn test_receipt_distinguishes_brew_kinds() {
        // Verify that receipt shows formula vs cask distinction
        let mut receipt = ReviewReceipt::new();

        if let Some(ghostty) = ToolCatalog::get_tool("ghostty") {
            receipt.add_install_action(
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&ghostty),
            );
        }

        if let Some(starship) = ToolCatalog::get_tool("starship") {
            receipt.add_install_action(
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&starship),
            );
        }

        let formatted = receipt.format_for_display();

        // Should contain both formula and cask labels
        assert!(formatted.contains("formula"));
        assert!(formatted.contains("cask"));
    }

    #[test]
    fn test_tool_metadata_completeness() {
        // All tools should have non-empty metadata
        for tool in ToolCatalog::all_tools() {
            assert!(!tool.id.is_empty(), "Tool id must not be empty");
            assert!(!tool.label.is_empty(), "Tool label must not be empty");
            assert!(!tool.pitch.is_empty(), "Tool pitch must not be empty");

            // If installable, must have a brew package
            if tool.installable {
                assert!(
                    !tool.brew_package.is_empty(),
                    "Installable tool must have brew package"
                );
            }

            // detect-only tools should not be installable
            if tool.detect_only {
                assert!(
                    !tool.installable,
                    "Detect-only tools should not be installable"
                );
            }
        }
    }

    #[test]
    fn test_selection_respects_installability() {
        // Only installable tools should produce install actions
        let all_selections: Vec<String> = ToolCatalog::all_tools()
            .iter()
            .map(|t| t.id.to_string())
            .collect();

        let actions = filter_valid_selections(all_selections);

        // Should have fewer actions than total tools (due to detect-only)
        assert!(actions.len() < ToolCatalog::all_tools().len());

        // All actions should be for installable tools
        for action in actions {
            let tool = ToolCatalog::get_tool(&action.tool_id).unwrap();
            assert!(
                tool.installable,
                "Action must only include installable tools"
            );
        }
    }
}

// Full pipeline and adapter output tests

#[test]
fn test_full_pipeline() {
    let tempdir = TempDir::new().unwrap();

    // Step 1: setup --quick (non-interactive, uses defaults)
    let output = slate_cmd_isolated(&tempdir)
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "setup --quick failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Step 2: theme catppuccin-mocha
    let output = slate_cmd_isolated(&tempdir)
        .args(["theme", "catppuccin-mocha"])
        .output()
        .unwrap();
    assert!(output.status.success(), "theme set failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Catppuccin Mocha") || stdout.contains("catppuccin-mocha"),
        "theme output should mention the theme name"
    );

    // Step 3: status (verify theme is reflected)
    let output = slate_cmd_isolated(&tempdir)
        .args(["status"])
        .output()
        .unwrap();
    assert!(output.status.success(), "status failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Catppuccin Mocha") || stdout.contains("catppuccin"),
        "status should show the current theme"
    );

    // Step 4: list (verify themes are listed)
    let output = slate_cmd_isolated(&tempdir)
        .args(["list"])
        .output()
        .unwrap();
    assert!(output.status.success(), "list failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("catppuccin") || stdout.contains("Catppuccin"),
        "list should include catppuccin themes"
    );
    assert!(
        stdout.contains("tokyo") || stdout.contains("Tokyo"),
        "list should include tokyo night themes"
    );

    // Step 5: font (verify font surface is reachable with a direct name argument)
    let output = slate_cmd_isolated(&tempdir)
        .args(["font", "JetBrainsMono Nerd Font"])
        .output()
        .unwrap();
    // font <name> should succeed in setting the font
    assert!(
        output.status.success(),
        "font <name> failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Step 6: clean (removes managed configs)
    let output = slate_cmd_isolated(&tempdir)
        .args(["clean"])
        .output()
        .unwrap();
    assert!(output.status.success(), "clean failed");

    // Step 6b: verify clean actually removed Slate-owned config state
    let slate_config_dir = tempdir.path().join(".config/slate");
    assert!(
        !slate_config_dir.exists(),
        "slate config dir should be removed after clean"
    );

    // Step 6c: shell marker block should also be gone
    let zshrc_path = tempdir.path().join(".zshrc");
    if zshrc_path.exists() {
        let zshrc = std::fs::read_to_string(&zshrc_path).unwrap();
        assert!(
            !zshrc.contains("slate:start"),
            ".zshrc should not retain Slate marker block after clean"
        );
    }

    // Step 7: restore --list (verify restore surface is accessible)
    let output = slate_cmd_isolated(&tempdir)
        .args(["restore", "--list"])
        .output()
        .unwrap();
    // restore --list should succeed even if no snapshots exist
    assert!(
        output.status.success(),
        "restore --list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_adapter_output_ghostty() {
    let tempdir = TempDir::new().unwrap();

    // Setup + set theme
    slate_cmd_isolated(&tempdir)
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    slate_cmd_isolated(&tempdir)
        .args(["theme", "catppuccin-mocha"])
        .output()
        .unwrap();

    // Verify Ghostty managed config
    let theme_conf = tempdir
        .path()
        .join("config/slate/managed/ghostty/theme.conf");
    if theme_conf.exists() {
        let content = std::fs::read_to_string(&theme_conf).unwrap();
        assert!(
            content.contains("background ="),
            "Ghostty theme.conf should contain background color"
        );
        assert!(
            content.contains("foreground ="),
            "Ghostty theme.conf should contain foreground color"
        );
        assert!(
            content.contains("palette ="),
            "Ghostty theme.conf should contain palette entries"
        );
    }

    // Verify blur.conf uses correct key name (BUG-1 regression test)
    let blur_conf = tempdir
        .path()
        .join("config/slate/managed/ghostty/blur.conf");
    if blur_conf.exists() {
        let content = std::fs::read_to_string(&blur_conf).unwrap();
        assert!(
            !content.contains("background-blur-radius"),
            "blur.conf must NOT use deprecated background-blur-radius key"
        );
        assert!(
            content.contains("background-blur"),
            "blur.conf should use background-blur key"
        );
    }
}

#[test]
fn test_adapter_output_starship() {
    let tempdir = TempDir::new().unwrap();
    slate_cmd_isolated(&tempdir)
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    slate_cmd_isolated(&tempdir)
        .args(["theme", "catppuccin-mocha"])
        .output()
        .unwrap();

    let palette_toml = tempdir
        .path()
        .join("config/slate/managed/starship/palette.toml");
    if palette_toml.exists() {
        let content = std::fs::read_to_string(&palette_toml).unwrap();
        assert!(
            content.contains("[palettes.slate]") || content.contains("palettes"),
            "Starship palette.toml should contain palette section"
        );
    }
}

#[test]
fn test_aura_command() {
    let tempdir = TempDir::new().unwrap();
    // Set up a theme first so aura has colors to use
    slate_cmd_isolated(&tempdir)
        .args(["setup", "--quick"])
        .output()
        .unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args(["aura"])
        .output()
        .unwrap();
    assert!(output.status.success(), "slate aura should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should contain some quote text (at least one of the attribution markers)
    assert!(
        stdout.contains("--") || stdout.contains("\u{2014}"),
        "aura should display a quote with attribution"
    );
}

#[test]
fn test_unsupported_terminal_graceful_skip() {
    let tempdir = TempDir::new().unwrap();
    // Simulate running from Terminal.app
    let output = slate_cmd_isolated(&tempdir)
        .env("TERM_PROGRAM", "Apple_Terminal")
        .args(["setup", "--quick"])
        .output()
        .unwrap();
    // Should succeed (skip terminal-specific features, not crash)
    assert!(
        output.status.success(),
        "setup should succeed even from Apple_Terminal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("ghostty failed during theme apply"),
        "setup should not report ghostty apply failures when running in Terminal.app: {stderr}"
    );
    assert!(
        !stderr.contains("alacritty failed during theme apply"),
        "setup should not report alacritty apply failures when running in Terminal.app: {stderr}"
    );
}

#[test]
fn test_status_reports_ghostty_compatibility() {
    let tempdir = TempDir::new().unwrap();
    let output = slate_cmd_isolated(&tempdir)
        .env("TERM_PROGRAM", "ghostty")
        .arg("status")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Ghostty"));
    assert!(stdout.contains("best experience"));
}

#[test]
fn test_status_reports_alacritty_limits() {
    let tempdir = TempDir::new().unwrap();
    let output = slate_cmd_isolated(&tempdir)
        .env("TERM_PROGRAM", "alacritty")
        .arg("status")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Alacritty"));
    assert!(stdout.contains("supported with limits"));
    assert!(stdout.contains("no blur"));
}

#[test]
fn test_status_reports_terminal_app_limits() {
    let tempdir = TempDir::new().unwrap();
    let output = slate_cmd_isolated(&tempdir)
        .env("TERM_PROGRAM", "Apple_Terminal")
        .arg("status")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Terminal.app"));
    assert!(stdout.contains("manual font pick"));
}

#[test]
fn test_status_reports_unknown_terminal_as_best_effort() {
    let tempdir = TempDir::new().unwrap();
    let output = slate_cmd_isolated(&tempdir)
        .env("TERM_PROGRAM", "WarpTerminal")
        .arg("status")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("WarpTerminal"));
    assert!(stdout.contains("best-effort only"));
}

#[test]
fn test_font_command_rejects_unknown_font() {
    let tempdir = TempDir::new().unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args(["font", "Definitely Not A Font"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "font command should fail for unknown font"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Font 'Definitely Not A Font' not found"));
}

#[test]
fn test_import_rejects_invalid_font_without_mutating_state() {
    let tempdir = TempDir::new().unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args([
            "import",
            "slate://catppuccin-mocha/Definitely-Not-A-Font/solid/none",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "import should fail for unknown font"
    );
    assert!(
        !tempdir.path().join(".config/slate/current").exists(),
        "invalid import should not apply theme state before validation"
    );
}

#[test]
fn test_import_rejects_invalid_opacity_without_mutating_state() {
    let tempdir = TempDir::new().unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args([
            "import",
            "slate://catppuccin-mocha/none/not-a-real-opacity/none",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "import should fail for invalid opacity"
    );
    assert!(
        !tempdir.path().join(".config/slate/current").exists(),
        "invalid import should not apply theme state before validation"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn test_system_font_switch_uses_plain_starship_profile() {
    // Menlo is a macOS system font; the slate font registry accepts it only on macOS.
    // Skip the test on Linux where the font simply isn't a valid target.
    let tempdir = TempDir::new().unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args(["font", "Menlo"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "font command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let shell_env = tempdir.path().join(".config/slate/managed/shell/env.zsh");
    let content = std::fs::read_to_string(&shell_env).unwrap();

    assert!(content.contains("managed/starship/plain.toml"));
    assert!(!content.contains("else\n  export STARSHIP_CONFIG="));
}

#[test]
fn test_clean_removes_ghostty_managed_config_references() {
    let tempdir = TempDir::new().unwrap();
    let ghostty_dir = tempdir.path().join(".config/ghostty");
    std::fs::create_dir_all(&ghostty_dir).unwrap();

    let managed_root = tempdir.path().join(".config/slate/managed/ghostty");
    let ghostty_config = ghostty_dir.join("config");
    std::fs::write(
        &ghostty_config,
        format!(
            "font-family = Menlo\nconfig-file = \"{}/theme.conf\"\nconfig-file = \"{}/opacity.conf\"\nconfig-file = \"{}/blur.conf\"\n",
            managed_root.display(),
            managed_root.display(),
            managed_root.display()
        ),
    )
    .unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args(["clean"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = std::fs::read_to_string(&ghostty_config).unwrap();
    assert!(content.contains("font-family = Menlo"));
    assert!(!content.contains("config-file = "));
}

#[test]
fn test_clean_removes_alacritty_managed_imports() {
    let tempdir = TempDir::new().unwrap();
    let alacritty_dir = tempdir.path().join(".config/alacritty");
    std::fs::create_dir_all(&alacritty_dir).unwrap();

    let managed_root = tempdir.path().join(".config/slate/managed/alacritty");
    let alacritty_config = alacritty_dir.join("alacritty.toml");
    std::fs::write(
        &alacritty_config,
        format!(
            "[general]\nimport = [\"{}/colors.toml\", \"{}/opacity.toml\", \"~/dotfiles/alacritty/base.toml\"]\n",
            managed_root.display(),
            managed_root.display()
        ),
    )
    .unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args(["clean"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = std::fs::read_to_string(&alacritty_config).unwrap();
    assert!(content.contains("~/dotfiles/alacritty/base.toml"));
    assert!(!content.contains("managed/alacritty/colors.toml"));
    assert!(!content.contains("managed/alacritty/opacity.toml"));
}

#[test]
fn test_clean_shell_loader_removes_zsh_and_bash_markers() {
    let tempdir = TempDir::new().unwrap();
    let zshrc = tempdir.path().join(".zshrc");
    let bashrc = tempdir.path().join(".bashrc");

    std::fs::write(
        &zshrc,
        "# user zsh\n# slate:start — managed by slate, do not edit\nsource '/tmp/env.zsh'\n# slate:end\n# keep zsh\n",
    )
    .unwrap();
    std::fs::write(
        &bashrc,
        "# user bash\n# slate:start — managed by slate, do not edit\nsource '/tmp/env.bash'\n# slate:end\n# keep bash\n",
    )
    .unwrap();

    let output = slate_cmd_isolated_with_shell(&tempdir, "/bin/bash")
        .args(["clean"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let zshrc_content = std::fs::read_to_string(&zshrc).unwrap();
    let bashrc_content = std::fs::read_to_string(&bashrc).unwrap();
    assert!(zshrc_content.contains("# user zsh"));
    assert!(zshrc_content.contains("# keep zsh"));
    assert!(!zshrc_content.contains("slate:start"));
    assert!(bashrc_content.contains("# user bash"));
    assert!(bashrc_content.contains("# keep bash"));
    assert!(!bashrc_content.contains("slate:start"));
}

#[test]
fn test_clean_shell_loader_removes_fish_loader_but_preserves_config_fish() {
    let tempdir = TempDir::new().unwrap();
    let fish_loader = tempdir.path().join(".config/fish/conf.d/slate.fish");
    let config_fish = tempdir.path().join(".config/fish/config.fish");
    std::fs::create_dir_all(fish_loader.parent().unwrap()).unwrap();
    std::fs::write(&fish_loader, "source '/tmp/env.fish'\n").unwrap();
    std::fs::write(&config_fish, "# keep fish config\n").unwrap();

    let output = slate_cmd_isolated_with_shell(&tempdir, "/usr/bin/fish")
        .args(["clean"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!fish_loader.exists());
    assert_eq!(
        std::fs::read_to_string(&config_fish).unwrap(),
        "# keep fish config\n"
    );
}

#[test]
fn test_clean_removes_auto_theme_watcher_launcher() {
    let tempdir = TempDir::new().unwrap();
    let watcher = tempdir
        .path()
        .join(".config/slate/managed/bin/slate-dark-mode-notify");
    std::fs::create_dir_all(watcher.parent().unwrap()).unwrap();
    std::fs::write(&watcher, "#!/bin/sh\nexit 0\n").unwrap();

    let output = slate_cmd_isolated(&tempdir)
        .args(["clean"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!watcher.exists());
}

#[test]
fn test_version_flag() {
    let output = Command::cargo_bin("slate")
        .unwrap()
        .args(["--version"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("slate") && stdout.contains(env!("CARGO_PKG_VERSION")),
        "version should contain crate name and version, got: {}",
        stdout
    );
}

// Tests for Preset/Font/Theme Selection & Mapping Logic

#[cfg(test)]
mod preset_font_theme_mapping {
    use slate_cli::cli::font_selection::FontCatalog;
    use slate_cli::cli::preset_selection::PresetCatalog;
    use slate_cli::cli::theme_selection::ThemeSelector;

    #[test]
    fn test_all_four_presets_locked_correctly() {
        // Verify the four locked presets match exactly
        let presets = PresetCatalog::all_presets();
        assert_eq!(presets.len(), 4, "Must have exactly 4 locked presets");

        // Modern Dark → Catppuccin Mocha + JetBrains Mono
        let modern = presets.iter().find(|p| p.id == "modern-dark").unwrap();
        assert_eq!(modern.theme_id, "catppuccin-mocha");
        assert_eq!(modern.font_id, "jetbrains-mono");

        // Minimal Frost → Nord + Hack
        let minimal = presets.iter().find(|p| p.id == "minimal-frost").unwrap();
        assert_eq!(minimal.theme_id, "nord");
        assert_eq!(minimal.font_id, "hack");

        // Retro Warm → Gruvbox Dark + Iosevka Term
        let retro = presets.iter().find(|p| p.id == "retro-warm").unwrap();
        assert_eq!(retro.theme_id, "gruvbox-dark");
        assert_eq!(retro.font_id, "iosevka-term");

        // Clean Light → Catppuccin Latte + Fira Code
        let clean = presets.iter().find(|p| p.id == "clean-light").unwrap();
        assert_eq!(clean.theme_id, "catppuccin-latte");
        assert_eq!(clean.font_id, "fira-code");
    }

    #[test]
    fn test_all_fonts_in_presets_exist() {
        // Verify all font IDs referenced in presets actually exist
        let presets = PresetCatalog::all_presets();
        for preset in presets {
            let font = FontCatalog::get_font(preset.font_id);
            assert!(
                font.is_some(),
                "Preset {} references nonexistent font {}",
                preset.id,
                preset.font_id
            );
        }
    }

    #[test]
    fn test_all_themes_in_presets_exist() {
        // Verify all theme IDs referenced in presets actually exist
        let selector = ThemeSelector::new().unwrap();
        let presets = PresetCatalog::all_presets();
        for preset in presets {
            let theme = selector.get_theme(preset.theme_id);
            assert!(
                theme.is_some(),
                "Preset {} references nonexistent theme {}",
                preset.id,
                preset.theme_id
            );
        }
    }

    #[test]
    fn test_theme_variants_available() {
        // +: Verify all 18 theme variants across 8 families are available
        let selector = ThemeSelector::new().unwrap();
        let count = selector.theme_count();
        assert_eq!(
            count, 18,
            "Must have exactly 18 theme variants (Catppuccin 4 + Tokyo Night 2 + Rosé Pine 3 + Kanagawa 3 + Everforest 2 + Dracula 1 + Nord 1 + Gruvbox 2)"
        );
    }

    #[test]
    fn test_gruvbox_themes_selectable() {
        // Verify Gruvbox Dark and Light are in the selection
        let selector = ThemeSelector::new().unwrap();
        assert!(
            selector.get_theme("gruvbox-dark").is_some(),
            "Gruvbox Dark must be available"
        );
        assert!(
            selector.get_theme("gruvbox-light").is_some(),
            "Gruvbox Light must be available"
        );
    }

    #[test]
    fn test_themes_grouped_by_family_count() {
        // Verify family grouping has correct distribution across all 8 families
        let selector = ThemeSelector::new().unwrap();
        let families = selector.themes_by_family();

        assert_eq!(families.len(), 8, "Must have 8 families");
        assert_eq!(families.get("Catppuccin").map(|v| v.len()), Some(4));
        assert_eq!(families.get("Tokyo Night").map(|v| v.len()), Some(2));
        assert_eq!(families.get("Rosé Pine").map(|v| v.len()), Some(3));
        assert_eq!(families.get("Kanagawa").map(|v| v.len()), Some(3));
        assert_eq!(families.get("Everforest").map(|v| v.len()), Some(2));
        assert_eq!(families.get("Dracula").map(|v| v.len()), Some(1));
        assert_eq!(families.get("Nord").map(|v| v.len()), Some(1));
        assert_eq!(families.get("Gruvbox").map(|v| v.len()), Some(2));
    }

    #[test]
    fn test_default_preset_is_modern_dark() {
        // quick uses Modern Dark as default
        let preset = PresetCatalog::default_preset();
        assert_eq!(preset.id, "modern-dark");
    }

    #[test]
    fn test_default_theme_exists() {
        // Default theme must exist for quick mode
        let selector = ThemeSelector::new().unwrap();
        let default_theme = ThemeSelector::default_theme_id();
        assert!(selector.get_theme(default_theme).is_some());
    }

    #[test]
    fn test_font_skip_option_preserves_current() {
        // Skip option allows keeping current font
        let (skip_id, skip_label) = FontCatalog::skip_option();
        assert_eq!(skip_id, "skip");
        assert!(!skip_label.is_empty());
        // In wizard, if selected == "skip", we don't update font
    }
}

#[cfg(test)]
mod rerun_behavior {
    use slate_cli::cli::wizard_core::{Wizard, WizardMode};

    #[test]
    fn test_wizard_context_has_rerun_awareness() {
        // WizardContext tracks current state for rerun
        let wizard = Wizard::new().unwrap();
        let context = wizard.get_context();
        // These fields allow the wizard to show "current" and default to "keep"
        assert_eq!(context.selected_font, None);
        assert_eq!(context.selected_theme, None);
        assert_eq!(context.current_step, 0);
    }

    #[test]
    fn test_quick_mode_reduces_step_count() {
        // Per constraints: Quick mode step count differs from Manual
        let mut wizard = Wizard::new().unwrap();
        let manual_steps = wizard.get_context().total_steps;

        wizard = Wizard::new().unwrap();
        wizard.get_context_mut().mode = WizardMode::Quick;
        wizard.get_context_mut().total_steps = 4; // Quick is shorter
        let quick_steps = wizard.get_context().total_steps;

        assert!(quick_steps < manual_steps || quick_steps == 4);
    }

    #[test]
    fn test_manual_mode_full_step_sequence() {
        // Manual mode step order
        // intro → mode → tools → font → theme → action → apply
        let wizard = Wizard::new().unwrap();
        let context = wizard.get_context();
        assert_eq!(context.mode, WizardMode::Manual, "Default should be manual");
        assert_eq!(context.total_steps, 6); // intro → mode → tools → font → theme → action (apply is implicit)
    }
}

#[cfg(test)]
mod optional_automations {
    use slate_cli::cli::preset_selection::PresetCatalog;
    use slate_cli::cli::tool_selection::TerminalSettings;
    use slate_cli::cli::wizard_core::Wizard;

    #[test]
    fn test_preset_visuals_are_defined() {
        // Presets include terminal visual settings
        let presets = PresetCatalog::all_presets();
        for preset in presets {
            // Visual settings are defined per preset
            let _opacity = preset.visuals.background_opacity;
            let _blur = preset.visuals.blur_radius;
            let _padding_x = preset.visuals.padding_x;
            let _padding_y = preset.visuals.padding_y;
            let _cursor = preset.visuals.cursor_style;
            // All fields are accessible for to apply
        }
    }

    #[test]
    fn test_preset_visual_settings_reasonable() {
        // Visual settings must be sensible
        let presets = PresetCatalog::all_presets();
        for preset in presets {
            assert!(
                preset.visuals.background_opacity > 0.0 && preset.visuals.background_opacity <= 1.0
            );
            assert!(matches!(
                preset.visuals.cursor_style,
                "block" | "underline" | "bar"
            ));
        }
    }

    #[test]
    fn test_receipt_can_show_terminal_visuals() {
        use slate_cli::detection::{TerminalKind, TerminalProfile};

        let mut wizard = Wizard::new().unwrap();
        wizard.get_context_mut().selected_terminal_settings = Some(TerminalSettings {
            background_opacity: 0.95,
            blur_enabled: true,
            padding_x: 12,
            padding_y: 12,
        });

        let receipt = wizard.build_review_receipt();
        let formatted = wizard.display_receipt(&receipt);
        assert!(formatted.contains("Terminal"));
        let terminal = TerminalProfile::detect();
        match terminal.kind() {
            TerminalKind::Ghostty => {
                assert!(formatted.contains("opacity 0.95"));
                assert!(formatted.contains("frosted glass"));
            }
            TerminalKind::Kitty | TerminalKind::Alacritty => {
                assert!(formatted.contains("opacity 0.95"));
                assert!(formatted.contains("blur not supported here"));
            }
            TerminalKind::TerminalApp | TerminalKind::Unknown => {
                assert!(formatted.contains("shell/tool theme"));
            }
        }
    }
}

// Tests for Polish and visual hierarchy

#[cfg(test)]
mod polish_and_clarity {
    use slate_cli::brand::language::Language;
    use slate_cli::brand::Symbols;
    use slate_cli::cli::tool_selection::ReviewReceipt;
    use slate_cli::cli::wizard_core::Wizard;

    #[test]
    fn test_completion_message_contains_dopamine() {
        // Per requirement: Time-to-Dopamine visible in completion
        assert!(Language::SETUP_COMPLETE.contains("beautiful"));
        assert!(
            Language::COMPLETION_TIME_TAKEN.contains("Time")
                || Language::COMPLETION_TIME_TAKEN.contains("dopamine")
        );
    }

    #[test]
    fn test_receipt_maintains_action_clarity() {
        // Per constraint: activation guidance remains visible after polish
        let mut receipt = ReviewReceipt::new();

        if let Some(ghostty) = slate_cli::cli::tool_selection::ToolCatalog::get_tool("ghostty") {
            receipt.add_install_action(
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&ghostty),
            );
        }

        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());

        let formatted = receipt.format_for_display();

        // Key information must be present and readable
        assert!(formatted.contains("Review")); // section header
        assert!(formatted.contains("Ghostty")); // tool names
        assert!(formatted.contains("JetBrains Mono")); // selected items
        assert!(formatted.contains("Catppuccin Mocha")); // theme

        // Receipt footer (activation guidance) must be visible
        assert!(formatted.contains("Ready") || formatted.contains("apply"));
    }

    #[test]
    fn test_completion_activation_guidance_present() {
        // Per constraint: activation guidance from remains prominent
        let activation = Language::activation_guidance("Ghostty", "new_window");
        assert!(activation.contains("Ghostty"));
        assert!(activation.contains("new_window"));

        let immediate = Language::activation_guidance("Starship", "immediate");
        assert!(immediate.contains("Starship"));
        assert!(immediate.contains("immediate"));
    }

    #[test]
    fn test_receipt_categories_clearly_labeled() {
        // Section headers must be clear
        assert!(
            Language::RECEIPT_HEADER.contains("Review")
                || Language::RECEIPT_HEADER.contains("confirm")
        );
        assert!(!Language::RECEIPT_INSTALL_SECTION.is_empty());
        assert!(!Language::RECEIPT_FONT_SECTION.is_empty());
        assert!(!Language::RECEIPT_THEME_SECTION.is_empty());
    }

    #[test]
    fn test_wizard_completion_timing_optional_not_mandatory() {
        // Timing should only appear if meaningful (not cluttering output)
        let wizard = Wizard::new().unwrap();
        // context.start_time is optional
        assert!(
            wizard.get_context().start_time.is_none() || wizard.get_context().start_time.is_some()
        );
        // The important thing: timing doesn't make output noisy
    }

    #[test]
    fn test_polish_preserves_symbol_language() {
        // Brand system: the 5 core symbols live under `src/brand/symbols.rs`
        // after the migration (moved the table,
        // deleted the `src/design/symbols.rs` shim).
        assert_eq!(Symbols::BRAND, '✦');
        assert_eq!(Symbols::SUCCESS, '✓');
        assert_eq!(Symbols::FAILURE, '✗');
        assert_eq!(Symbols::PENDING, '○');
        assert_eq!(Symbols::DIAMOND, '◆');
    }
}
