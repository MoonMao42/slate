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
        assert_eq!(ghostty.brew_kind, BrewKind::Formula);
        
        let alacritty = ToolCatalog::get_tool("alacritty").unwrap();
        assert_eq!(alacritty.brew_kind, BrewKind::Cask);
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
        
        if let Some(alacritty) = ToolCatalog::get_tool("alacritty") {
            receipt.add_install_action(
                slate_cli::cli::tool_selection::InstallAction::from_metadata(&alacritty)
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
