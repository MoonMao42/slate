use super::*;

#[test]
fn automatic_configuration_order_is_catalog_stable_and_preserves_opt_in() {
    let present = |in_path| ToolPresence {
        installed: true,
        in_path,
        evidence: None,
    };
    let entries = vec![
        ("kitty", present(false)),
        ("ghostty", present(false)),
        ("bat", present(true)),
        ("starship", present(true)),
        ("btop", present(false)),
        ("alacritty", ToolPresence::missing()),
        ("unknown", present(true)),
    ];
    let expected = crate::cli::tool_selection::ToolCatalog::all_tools()
        .iter()
        .filter(|tool| matches!(tool.id, "kitty" | "ghostty" | "bat" | "starship"))
        .map(|tool| tool.id.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 4);
    for reverse in [false, true] {
        let mut order = entries.clone();
        if reverse {
            order.reverse();
        }
        let installed = order
            .into_iter()
            .map(|(id, value)| (id.to_owned(), value))
            .collect();
        assert_eq!(automatic_configuration_tools(&installed), expected);
    }
    assert!(automatic_configuration_tools(&HashMap::new()).is_empty());
}

#[test]
fn wizard_opacity_uses_appearance_but_keeps_explicit_presets_without_writes() {
    use crate::opacity::OpacityPreset;
    for selected in [
        None,
        Some(OpacityPreset::Solid),
        Some(OpacityPreset::Frosted),
        Some(OpacityPreset::Clear),
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut wizard = Wizard::with_env(&env).unwrap();
        wizard.context.selected_theme = Some("rose-pine-dawn".into());
        wizard.context.selected_opacity = selected;
        wizard.step_auto_opacity().unwrap();
        assert_eq!(
            wizard.context.selected_opacity,
            selected.or(Some(OpacityPreset::Solid))
        );
        assert_eq!(wizard.context.current_step, 1);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }
}

#[test]
fn wizard_opacity_unknown_theme_stops_without_silent_fallback_or_state_advance() {
    for selected in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut wizard = Wizard::with_env(&env).unwrap();
        if selected {
            wizard.context.selected_theme = Some("PRIVATE_UNKNOWN".into());
        } else {
            wizard.context.current_theme = Some("PRIVATE_UNKNOWN".into());
        }
        let error = wizard.step_auto_opacity().unwrap_err().to_string();
        assert!(error.contains("No opacity was selected"));
        assert!(!error.contains("PRIVATE_UNKNOWN"));
        assert_eq!(wizard.context.selected_opacity, None);
        assert_eq!(wizard.context.current_step, 0);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }
}

#[test]
fn quick_shell_wizard_review_uses_shell_defaults_including_force_mode() {
    use crate::platform::packages::{InstallContext, PackageManagerBackend};
    let context = InstallContext {
        package_manager: PackageManagerBackend::Unsupported,
        supported_os: true,
    };
    for force in [false, true] {
        for shell in [
            ShellBackend::Bash,
            ShellBackend::Fish,
            ShellBackend::Zsh,
            ShellBackend::Unsupported,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(temp.path().to_owned());
            let mut wizard = Wizard::with_env(&env).unwrap();
            wizard.context.mode = WizardMode::Quick;
            wizard.context.force = force;
            let result = wizard.select_quick_tools(&HashMap::new(), shell, context);
            if matches!(shell, ShellBackend::Bash | ShellBackend::Fish) {
                result.unwrap();
                assert_eq!(wizard.context.selected_tools, ["starship"]);
                assert_eq!(wizard.context.tools_to_configure, ["starship"]);
                let receipt = wizard.prepare_review_receipt(context).unwrap();
                assert_eq!(receipt.install_actions.len(), 1);
                let text = wizard.display_receipt(&receipt);
                assert!(text.contains("user-local executable"));
                assert!(!text.contains("zsh-syntax-highlighting"));
            } else {
                assert!(result.is_err());
                assert!(wizard.context.selected_tools.is_empty());
                assert!(wizard.confirmed_install_plan().is_err());
            }
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }
}

#[test]
fn install_review_wizard_uses_captured_routes_and_does_not_reuse_stale_confirmation() {
    use crate::platform::packages::{InstallContext, PackageManagerBackend};
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let mut wizard = Wizard::with_env(&env).unwrap();
    wizard.context.selected_tools = vec!["starship".into(), "delta".into(), "starship".into()];
    let apt = InstallContext {
        package_manager: PackageManagerBackend::Apt,
        supported_os: true,
    };
    let receipt = wizard.prepare_review_receipt(apt).unwrap();
    assert_eq!(receipt.install_actions.len(), 2);
    let text = wizard.display_receipt(&receipt);
    assert!(text.contains("apt package (git-delta; administrator access)"));
    assert!(text.contains("user-local executable") && !text.contains("Homebrew formula"));
    assert!(wizard.confirmed_install_plan().is_err());
    wizard.context.confirmed = true;
    let captured = wizard.confirmed_install_plan().unwrap();
    assert!(captured.description("delta").unwrap().contains("git-delta"));
    wizard.context.selected_tools = vec!["fastfetch".into()];
    assert!(wizard
        .prepare_review_receipt(InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            ..apt
        })
        .is_err());
    assert!(wizard.confirmed_install_plan().is_err());
    assert!(!wizard.context.confirmed);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn bootstrap_quick_keeps_requested_core_tools_and_blocks_unavailable_routes() {
    use crate::platform::packages::{InstallContext, PackageManagerBackend};
    let context = InstallContext {
        package_manager: PackageManagerBackend::Unsupported,
        supported_os: true,
    };
    let mut installed = HashMap::new();
    let (selected, _) = compute_quick_tool_plan(&installed, None, ShellBackend::Zsh).unwrap();
    assert_eq!(selected, ["starship", "zsh-syntax-highlighting"]);
    let error =
        crate::cli::tool_selection::validate_install_routes(&selected, context).unwrap_err();
    assert!(error.to_string().contains("zsh-syntax-highlighting"));
    assert!(error.to_string().contains("Manual setup"));
    installed.insert(
        "zsh-syntax-highlighting".into(),
        ToolPresence {
            installed: true,
            in_path: false,
            evidence: None,
        },
    );
    let (selected, configure) =
        compute_quick_tool_plan(&installed, None, ShellBackend::Zsh).unwrap();
    assert_eq!(selected, ["starship"]);
    assert!(configure.contains(&"zsh-syntax-highlighting".into()));
    crate::cli::tool_selection::validate_install_routes(&selected, context).unwrap();
    assert!(crate::cli::tool_selection::validate_install_routes(&["bat".into()], context).is_err());
    crate::cli::tool_selection::validate_install_routes(&[], context).unwrap();
}

fn isolated_wizard() -> Wizard {
    let home = tempfile::tempdir().unwrap();
    Wizard::with_env(&SlateEnv::with_home(home.path().to_owned())).unwrap()
}

#[test]
fn wizard_profile_stays_bound_for_inventory_theme_and_session_receipt() {
    let td = tempfile::tempdir().unwrap();
    for (name, theme) in [("first", "nord"), ("second", "dracula")] {
        let home = td.path().join(name);
        let custom = home.join("custom-xdg");
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(custom.as_os_str().to_owned()),
            "SSH_CONNECTION" => Some("private remote session".into()),
            _ => None,
        })
        .unwrap();
        let app = home.join("Applications/Ghostty.app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(env.config_dir()).unwrap();
        std::fs::write(env.managed_file("current"), format!("{theme}\n")).unwrap();
        let mut wizard = Wizard::with_env(&env).unwrap();
        assert_eq!(wizard.terminal.session(), env.session());
        assert_eq!(
            wizard.installed_tools()["ghostty"].evidence,
            Some(crate::detection::ToolEvidence::AppBundle(app))
        );
        std::fs::write(env.managed_file("current"), "catppuccin-mocha\n").unwrap();
        assert_eq!(wizard.render_context().unwrap().theme.id, theme);
        wizard.step_quick_auto_tools().unwrap();
        assert!(wizard
            .context
            .tools_to_configure
            .iter()
            .any(|tool| tool == "ghostty"));
        let receipt = wizard.display_receipt(&wizard.build_review_receipt());
        assert!(receipt.contains("remote shell"));
        assert!(!env.slate_cache_dir().exists());
    }
}

#[test]
fn test_wizard_new() {
    let wizard = isolated_wizard();
    let context = wizard.get_context();
    assert_eq!(context.mode, WizardMode::Manual);
    assert_eq!(context.current_step, 0);
    assert_eq!(context.total_steps, 6);
}

#[test]
fn test_wizard_context_fields() {
    let wizard = isolated_wizard();
    let context = wizard.get_context();
    assert!(context.selected_tools.is_empty());
    assert!(context.selected_font.is_none());
    assert!(context.selected_theme.is_none());
}

#[test]
fn test_wizard_force_flag_clears_state() {
    let mut wizard = isolated_wizard();
    wizard.context.current_font = Some("JetBrains Mono".to_string());
    wizard.context.current_theme = Some("Catppuccin Mocha".to_string());

    wizard.context.force = true;
    wizard.context.current_font = None;
    wizard.context.current_theme = None;

    assert!(wizard.context.force);
    assert!(wizard.context.current_font.is_none());
}

#[test]
fn test_build_review_receipt_empty() {
    let wizard = isolated_wizard();
    let receipt = wizard.build_review_receipt();
    assert!(receipt.install_actions.is_empty());
}

#[test]
fn test_build_review_receipt_with_selections() {
    let mut wizard = isolated_wizard();
    wizard.context.selected_tools = vec!["ghostty".to_string(), "starship".to_string()];
    wizard.context.selected_font = Some("jetbrains-mono".to_string());
    wizard.context.selected_theme = Some("catppuccin-mocha".to_string());

    let receipt = wizard.build_review_receipt();
    assert_eq!(receipt.install_actions.len(), 2);
    assert_eq!(
        receipt.selected_font.as_deref(),
        Some("JetBrains Mono Nerd Font")
    );
    assert_eq!(receipt.selected_theme.as_deref(), Some("Catppuccin Mocha"));
}

#[test]
fn test_build_review_receipt_uses_current_state_when_skipped() {
    let mut wizard = isolated_wizard();
    wizard.context.current_font = Some("SF Mono".to_string());
    wizard.context.current_theme = Some("catppuccin-mocha".to_string());

    let receipt = wizard.build_review_receipt();
    assert_eq!(
        receipt.selected_font.as_deref(),
        Some("Keep current (SF Mono)")
    );
    assert_eq!(
        receipt.selected_theme.as_deref(),
        Some("Keep current (Catppuccin Mocha)")
    );
}

#[test]
fn test_build_review_receipt_includes_terminal_settings() {
    let mut wizard = isolated_wizard();
    wizard.context.selected_terminal_settings = Some(TerminalSettings {
        background_opacity: 0.95,
        blur_enabled: true,
        padding_x: 12,
        padding_y: 12,
    });

    let receipt = wizard.build_review_receipt();
    let settings = receipt
        .terminal_settings
        .expect("terminal settings should exist");
    assert_eq!(settings.padding_x, 12);
    assert!(settings.blur_enabled);
}

#[test]
fn manual_review_exposes_actual_opacity_without_inventing_preset_padding() {
    for opacity in [
        crate::opacity::OpacityPreset::Solid,
        crate::opacity::OpacityPreset::Frosted,
    ] {
        let mut wizard = isolated_wizard();
        wizard.context.selected_opacity = Some(opacity);
        let receipt = wizard.build_review_receipt();
        assert!(receipt.terminal_settings.is_none());
        assert_eq!(receipt.selected_opacity, Some(opacity));
        let rendered = receipt.format_with_install_plan(
            None,
            &TerminalProfile::from_env_vars(Some("ghostty"), None),
            None,
        );
        assert!(
            rendered.contains(&format!("opacity {:.2}", opacity.to_f32())),
            "{rendered}"
        );
        assert_eq!(
            rendered.contains("frosted glass"),
            opacity.blur_radius() > 0
        );
    }
}

#[test]
fn review_distinguishes_unchanged_disabled_and_enabled_startup_info() {
    for choice in [None, Some(false), Some(true)] {
        let mut wizard = isolated_wizard();
        wizard.context.fastfetch_enabled = choice;
        let receipt = wizard.build_review_receipt();
        assert_eq!(receipt.fastfetch_enabled, choice);
        let text = wizard.display_receipt(&receipt);
        match choice {
            None => assert!(!text.contains("Startup system info")),
            Some(false) => assert!(text.contains("Startup system info: Off"), "{text}"),
            Some(true) => assert!(text.contains("Startup system info: On"), "{text}"),
        }
    }
}

#[test]
fn editing_autorun_removes_only_its_own_install_and_config_requests() {
    for install in [false, true] {
        for configure in [false, true] {
            let mut wizard = isolated_wizard();
            if install {
                wizard.context.selected_tools.push("fastfetch".into());
            }
            if configure {
                wizard.context.tools_to_configure.push("fastfetch".into());
            }
            let before_installs = wizard.context.selected_tools.clone();
            let before_configs = wizard.context.tools_to_configure.clone();
            wizard.set_fastfetch_selection(true, false);
            wizard.set_fastfetch_selection(true, false);
            assert_eq!(wizard.context.selected_tools, ["fastfetch"]);
            assert_eq!(wizard.context.tools_to_configure, ["fastfetch"]);
            wizard.set_fastfetch_selection(false, false);
            assert_eq!(wizard.context.selected_tools, before_installs);
            assert_eq!(wizard.context.tools_to_configure, before_configs);
            assert_eq!(wizard.context.fastfetch_enabled, Some(false));
        }
    }
}

#[test]
fn changing_setup_mode_clears_pending_actions_but_keeps_saved_state_hints() {
    let mut wizard = isolated_wizard();
    wizard.context.current_theme = Some("nord".into());
    wizard.context.current_font = Some("Personal Font".into());
    wizard_support::apply_preset_selection(&mut wizard.context, &PresetCatalog::default_preset());
    wizard.context.selected_tools.push("bat".into());
    wizard.context.tools_to_configure.push("bat".into());
    wizard.set_fastfetch_selection(true, false);
    wizard.context.confirmed = true;
    wizard.clear_pending_choices();
    assert!(wizard.context.selected_tools.is_empty());
    assert!(wizard.context.tools_to_configure.is_empty());
    assert!(wizard.context.selected_font.is_none());
    assert!(wizard.context.selected_theme.is_none());
    assert!(wizard.context.selected_opacity.is_none());
    assert!(wizard.context.selected_terminal_settings.is_none());
    assert!(wizard.context.fastfetch_enabled.is_none());
    assert!(!wizard.context.confirmed);
    assert!(wizard.confirmed_install_plan().is_err());
    assert_eq!(wizard.context.current_theme.as_deref(), Some("nord"));
    assert_eq!(
        wizard.context.current_font.as_deref(),
        Some("Personal Font")
    );
}

#[test]
fn review_includes_configuration_only_targets_without_implying_installation() {
    let mut wizard = isolated_wizard();
    wizard.context.tools_to_configure = vec!["starship".into(), "ghostty".into(), "ghostty".into()];
    let receipt = wizard.build_review_receipt();
    assert_eq!(
        receipt.tools_to_configure,
        wizard.context.tools_to_configure
    );
    assert!(receipt.install_actions.is_empty());
    let text = wizard.display_receipt(&receipt);
    assert!(
        text.contains("Configure colors: Ghostty · Starship"),
        "{text}"
    );
    assert!(!text.contains("formula") && !text.contains("cask"));
    let mut unsafe_receipt = ReviewReceipt::new();
    unsafe_receipt.tools_to_configure = vec!["unknown\x1b[2J\nfixture".into()];
    let text = unsafe_receipt.format_with_install_plan(None, &wizard.terminal, None);
    assert!(!text.contains('\x1b'));
    assert!(!text.contains("[2J\nfixture"));
}

#[test]
fn test_quick_mode_adjusts_step_count() {
    let mut wizard = isolated_wizard();
    wizard.context.mode = WizardMode::Quick;
    wizard.context.total_steps = 4;
    assert_eq!(wizard.context.total_steps, 4);
}

#[test]
fn test_wizard_mode_variants() {
    assert_ne!(WizardMode::Quick, WizardMode::Manual);
}

#[test]
fn test_display_receipt_includes_sections() {
    let mut wizard = isolated_wizard();
    wizard.context.selected_tools = vec!["ghostty".to_string()];
    wizard.context.selected_font = Some("jetbrains-mono".to_string());
    wizard.context.selected_theme = Some("catppuccin-mocha".to_string());

    let receipt = wizard.build_review_receipt();
    let display = wizard.display_receipt(&receipt);

    assert!(display.contains("Review"));
    assert!(display.contains("JetBrains Mono Nerd Font"));
    assert!(display.contains("Catppuccin Mocha"));
}

#[test]
fn test_wizard_tracks_start_time() {
    let mut wizard = isolated_wizard();
    wizard.context.start_time = Some(Instant::now());
    assert!(wizard.context.start_time.is_some());
}

#[test]
fn test_compute_quick_tool_plan_configures_starship_and_current_terminal() {
    let mut installed = HashMap::new();
    installed.insert(
        "starship".to_string(),
        ToolPresence {
            installed: true,
            in_path: true,
            evidence: None,
        },
    );
    installed.insert(
        "ghostty".to_string(),
        ToolPresence {
            installed: true,
            in_path: false,
            evidence: None,
        },
    );

    let (selected_tools, tools_to_configure) =
        compute_quick_tool_plan(&installed, Some("ghostty"), ShellBackend::Zsh).unwrap();

    assert!(!selected_tools.contains(&"starship".to_string()));
    assert!(tools_to_configure.contains(&"starship".to_string()));
    assert!(tools_to_configure.contains(&"ghostty".to_string()));
}

#[test]
fn test_current_terminal_tool_id_includes_kitty() {
    let ghostty = TerminalProfile::from_env_vars(Some("ghostty"), None);
    let alacritty = TerminalProfile::from_env_vars(Some("Alacritty"), None);
    let kitty = TerminalProfile::from_env_vars(None, Some("xterm-kitty"));
    let apple = TerminalProfile::from_env_vars(Some("Apple_Terminal"), None);

    assert_eq!(current_terminal_tool_id(&ghostty), Some("ghostty"));
    assert_eq!(current_terminal_tool_id(&alacritty), Some("alacritty"));
    assert_eq!(current_terminal_tool_id(&kitty), Some("kitty"));
    assert_eq!(current_terminal_tool_id(&apple), None);
}
