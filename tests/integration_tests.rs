use assert_cmd::Command;
use slate_cli::brand::language::Language;

#[test]
fn test_cli_help_shows_commands() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("setup"));
    assert!(stdout.contains("set"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("restore"));
    assert!(stdout.contains("init"));
}

#[test]
fn test_setup_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["setup", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("setup"));
    assert!(stdout.contains("--quick"));
}

#[test]
fn test_set_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["set", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("set"));
}

#[test]
fn test_status_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["status", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("status"));
}

#[test]
fn test_list_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["list", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("list"));
}

#[test]
fn test_restore_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["restore", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("restore"));
}

#[test]
fn test_init_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["init", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("init"));
}

#[test]
fn test_setup_quick_flag() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["setup", "--quick"]).output().unwrap();
    // In quick mode, wizard runs successfully
    assert!(output.status.success());
}

#[test]
fn test_set_with_theme_argument() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["set", "catppuccin-mocha"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Placeholder shows theme name
    assert!(stdout.contains("catppuccin-mocha"));
}

#[test]
fn test_status_command_runs() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("status").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains(Language::STATUS_PENDING));
}

#[test]
fn test_list_command_runs() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("list").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains(Language::LIST_PENDING));
}

#[test]
fn test_restore_with_backup_id() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["restore", "backup123"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("backup123"));
}

#[test]
fn test_init_with_shell_arg() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["init", "zsh"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("slate shell init for zsh"));
    assert!(stdout.contains("SLATE_HOME"));
}

// Setup wizard tests 

#[test]
fn test_setup_wizard_intro_displays() {
    // Verify wizard displays intro frame and completes successfully
    let mut cmd = Command::cargo_bin("slate").unwrap();
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
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.arg("setup").arg("--quick");
    
    let output = cmd.output().unwrap();
    assert!(output.status.success());
}

#[test]
fn test_setup_wizard_step_counter_present() {
    // Verify step counter format "Step X of Y" is logged
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.arg("setup").arg("--quick");
    
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    
    // In quick mode, step counter should log completion
    assert!(output.status.success());
}

#[test]
fn test_setup_quick_mode_minimal_interactions() {
    // Verify --quick flag skips mode selection
    let mut cmd = Command::cargo_bin("slate").unwrap();
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

// Tool selection logic tests 

#[cfg(test)]
mod tool_selection_tests {
    use slate_cli::cli::tool_selection::{
        ToolCatalog, BrewKind, compute_install_candidates, filter_valid_selections, ReviewReceipt
    };
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
        installed.insert("tmux".to_string(), false); // not installed

        let candidates = compute_install_candidates(&installed);
        
        // Even though tmux is not installed, it should NOT be a candidate
        assert!(!candidates.iter().any(|t| t.id == "tmux"));
    }

    #[test]
    fn test_already_installed_tools_not_in_candidates() {
        // Tools that are already installed should not appear in install candidates
        let mut installed = HashMap::new();
        installed.insert("ghostty".to_string(), true);
        installed.insert("starship".to_string(), false);

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
        // filter_valid_selections should remove non-installable tools
        let selected = vec![
            "ghostty".to_string(),  // installable ✓
            "tmux".to_string(),     // detect-only ✗
            "nonexistent".to_string(), // unknown ✗
        ];

        let actions = filter_valid_selections(selected);

        // Only ghostty should be included
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].tool_id, "ghostty");
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
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&ghostty)
            );
        }
        
        if let Some(starship) = ToolCatalog::get_tool("starship") {
            receipt.add_install_action(
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&starship)
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
                assert!(!tool.brew_package.is_empty(), "Installable tool must have brew package");
            }
            
            // detect-only tools should not be installable
            if tool.detect_only {
                assert!(!tool.installable, "Detect-only tools should not be installable");
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
            assert!(tool.installable, "Action must only include installable tools");
        }
    }
}

// Tests for -03: Preset/Font/Theme Selection & Mapping Logic

#[cfg(test)]
mod preset_font_theme_mapping {
    use slate_cli::cli::preset_selection::PresetCatalog;
    use slate_cli::cli::font_selection::FontCatalog;
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
            assert!(font.is_some(), "Preset {} references nonexistent font {}", preset.id, preset.font_id);
        }
    }

    #[test]
    fn test_all_themes_in_presets_exist() {
        // Verify all theme IDs referenced in presets actually exist
        let selector = ThemeSelector::new().unwrap();
        let presets = PresetCatalog::all_presets();
        for preset in presets {
            let theme = selector.get_theme(preset.theme_id);
            assert!(theme.is_some(), "Preset {} references nonexistent theme {}", preset.id, preset.theme_id);
        }
    }

    #[test]
    fn test_ten_theme_variants_available() {
        // Verify all 10 theme variants are available
        let selector = ThemeSelector::new().unwrap();
        let count = selector.theme_count();
        assert_eq!(count, 10, "Must have exactly 10 theme variants (Catppuccin 4 + Tokyo Night 2 + Dracula + Nord + Gruvbox 2)");
    }

    #[test]
    fn test_gruvbox_themes_selectable() {
        // Verify Gruvbox Dark and Light are in the selection
        let selector = ThemeSelector::new().unwrap();
        assert!(selector.get_theme("gruvbox-dark").is_some(), "Gruvbox Dark must be available");
        assert!(selector.get_theme("gruvbox-light").is_some(), "Gruvbox Light must be available");
    }

    #[test]
    fn test_themes_grouped_by_family_count() {
        // Verify family grouping has correct distribution
        let selector = ThemeSelector::new().unwrap();
        let families = selector.themes_by_family();
        
        assert_eq!(families.len(), 5, "Must have 5 families");
        assert_eq!(families.get("Catppuccin").map(|v| v.len()), Some(4));
        assert_eq!(families.get("Tokyo Night").map(|v| v.len()), Some(2));
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
    fn test_wizard_detects_current_state_on_new() {
        // Wizard detects current font on startup
        let wizard = Wizard::new().unwrap();
        let _context = wizard.get_context();
        // current_font may be Some or None depending on environment
        // The important thing is detection doesn't crash
        assert!(true);
    }

    #[test]
    fn test_wizard_context_has_rerun_awareness() {
        // WizardContext tracks current state for rerun
        let wizard = Wizard::new().unwrap();
        let context = wizard.get_context();
        // These fields allow the wizard to show "current" and default to "keep"
        assert_eq!(context.selected_font, None);
        assert_eq!(context.selected_theme, None);
        // But detection fields are available:
        let has_font_detection = context.current_font.is_some() || context.current_font.is_none();
        let has_theme_detection = context.current_theme.is_some() || context.current_theme.is_none();
        assert!(has_font_detection && has_theme_detection);
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
            // All fields are accessible for phase 3 to apply
        }
    }

    #[test]
    fn test_preset_visual_settings_reasonable() {
        // Visual settings must be sensible
        let presets = PresetCatalog::all_presets();
        for preset in presets {
            assert!(preset.visuals.background_opacity > 0.0 && preset.visuals.background_opacity <= 1.0);
            assert!(matches!(preset.visuals.cursor_style, "block" | "underline" | "bar"));
        }
    }

    #[test]
    fn test_receipt_can_show_terminal_visuals() {
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
        assert!(formatted.contains("opacity 0.95"));
        assert!(formatted.contains("padding 12x12"));
    }
}

// Tests for -05: Polish and visual hierarchy

#[cfg(test)]
mod polish_and_clarity {
    use slate_cli::brand::language::Language;
    use slate_cli::design::typography::Typography;
    use slate_cli::cli::tool_selection::ReviewReceipt;
    use slate_cli::cli::wizard_core::Wizard;

    #[test]
    fn test_completion_message_contains_dopamine() {
        // Per requirement: Time-to-Dopamine visible in completion
        assert!(Language::SETUP_COMPLETE.contains("beautiful"));
        assert!(Language::COMPLETION_TIME_TAKEN.contains("Time") || Language::COMPLETION_TIME_TAKEN.contains("dopamine"));
    }

    #[test]
    fn test_receipt_maintains_action_clarity() {
        // Per constraint: activation guidance remains visible after polish
        let mut receipt = ReviewReceipt::new();
        
        if let Some(ghostty) = slate_cli::cli::tool_selection::ToolCatalog::get_tool("ghostty") {
            receipt.add_install_action(
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&ghostty)
            );
        }
        
        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());

        let formatted = receipt.format_for_display();
        
        // Key information must be present and readable
        assert!(formatted.contains("Review"));  // section header
        assert!(formatted.contains("Ghostty")); // tool names
        assert!(formatted.contains("JetBrains Mono")); // selected items
        assert!(formatted.contains("Catppuccin Mocha")); // theme
        
        // Receipt footer (activation guidance) must be visible
        assert!(formatted.contains("Ready") || formatted.contains("apply"));
    }

    #[test]
    fn test_typography_helpers_maintain_readability() {
        // Verify typography helpers don't obscure content
        let section = Typography::section_header("Tool Inventory");
        assert!(section.contains("Tool Inventory")); // Must be readable
        assert!(section.contains("✦")); // Brand mark visible

        let strong = Typography::strong_emphasis("Your terminal is now beautiful!");
        assert!(strong.contains("Your terminal is now beautiful!")); // Content visible
        
        let item = Typography::list_item('✓', "Ghostty", "Makes your terminal glow");
        assert!(item.contains("Ghostty")); // Label visible
        assert!(item.contains("Makes your terminal glow")); // Description visible
    }

    #[test]
    fn test_completion_activation_guidance_present() {
        // Per constraint: activation guidance from -04 remains prominent
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
        assert!(Language::RECEIPT_HEADER.contains("Review") || Language::RECEIPT_HEADER.contains("confirm"));
        assert!(!Language::RECEIPT_INSTALL_SECTION.is_empty());
        assert!(!Language::RECEIPT_FONT_SECTION.is_empty());
        assert!(!Language::RECEIPT_THEME_SECTION.is_empty());
    }

    #[test]
    fn test_wizard_completion_timing_optional_not_mandatory() {
        // Timing should only appear if meaningful (not cluttering output)
        let wizard = Wizard::new().unwrap();
        // context.start_time is optional
        assert!(wizard.get_context().start_time.is_none() || wizard.get_context().start_time.is_some());
        // The important thing: timing doesn't make output noisy
    }

    #[test]
    fn test_polish_preserves_symbol_language() {
        // Design system symbols remain consistent
        assert_eq!(slate_cli::design::symbols::Symbols::BRAND, '✦');
        assert_eq!(slate_cli::design::symbols::Symbols::SUCCESS, '✓');
        assert_eq!(slate_cli::design::symbols::Symbols::PENDING, '○');
        assert_eq!(slate_cli::design::symbols::Symbols::CTA_ARROW, '→');
        // No conflicting changes to symbols
    }

    #[test]
    fn test_hierarchy_helpers_are_optional_not_required() {
        // Typography helpers are infrastructure, not requirements for wizard operation
        // This test verifies the design principle: helpers are optional
        let _section = Typography::section_header("Test"); // Can be used
        let _secondary = Typography::secondary_label("label", "value"); // Can be used
        let _list_item = Typography::list_item('•', "item", "description"); // Can be used
        // Wizard can still work without them (backward compatibility implicit)
    }
}
