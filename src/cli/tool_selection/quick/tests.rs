use super::*;
use crate::{
    cli::tool_selection::{compute_install_candidates_for_platform, validate_install_routes},
    platform::packages::{InstallContext, PackageManagerBackend},
};

fn present(in_path: bool) -> ToolPresence {
    ToolPresence {
        installed: true,
        in_path,
        evidence: None,
    }
}

#[test]
fn quick_shell_core_tools_and_install_routes_match_the_target_shell() {
    let context = InstallContext {
        package_manager: PackageManagerBackend::Unsupported,
        supported_os: true,
    };
    for shell in [ShellBackend::Bash, ShellBackend::Fish, ShellBackend::Zsh] {
        let (selected, configure) = plan(&HashMap::new(), None, shell).unwrap();
        let expected = if shell == ShellBackend::Zsh {
            vec!["starship", "zsh-syntax-highlighting"]
        } else {
            vec!["starship"]
        };
        assert_eq!(selected, expected);
        assert_eq!(configure, selected);
        assert_eq!(
            validate_install_routes(&selected, context).is_ok(),
            shell != ShellBackend::Zsh
        );
        for id in core_tools(shell) {
            assert!(ToolCatalog::get_tool(id).unwrap().installable);
        }
    }
    assert!(plan(&HashMap::new(), None, ShellBackend::Unsupported)
        .unwrap_err()
        .to_string()
        .contains("SHELL"));
}

#[test]
fn quick_shell_existing_tools_keep_tiers_and_deterministic_configuration_order() {
    let entries = [
        ("starship", present(false)),
        ("zsh-syntax-highlighting", present(true)),
        ("ghostty", present(false)),
        ("bat", present(true)),
        ("delta", present(true)),
        ("zellij", present(true)),
        ("lazygit", present(false)),
    ];
    let installed = entries
        .iter()
        .map(|(id, p)| (id.to_string(), p.clone()))
        .collect();
    let reversed = entries
        .iter()
        .rev()
        .map(|(id, p)| (id.to_string(), p.clone()))
        .collect();
    for shell in [ShellBackend::Bash, ShellBackend::Fish, ShellBackend::Zsh] {
        let (selected, configure) = plan(&installed, Some("ghostty"), shell).unwrap();
        assert!(selected.is_empty());
        assert_eq!(
            plan(&reversed, Some("ghostty"), shell).unwrap().1,
            configure
        );
        for id in ["starship", "ghostty", "bat", "delta", "zellij"] {
            assert!(configure.iter().any(|tool| tool == id));
        }
        assert!(
            !configure.contains(&"lazygit".into()),
            "unselected fallback tools stay opt-in"
        );
        assert_eq!(
            configure.contains(&"zsh-syntax-highlighting".into()),
            shell == ShellBackend::Zsh
        );
        let mut unique = configure.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(configure.len(), unique.len());
    }
}

#[test]
fn quick_shell_defaults_do_not_remove_manual_zsh_installation() {
    let apt = InstallContext {
        package_manager: PackageManagerBackend::Apt,
        supported_os: true,
    };
    let choices = compute_install_candidates_for_platform(&HashMap::new(), apt);
    assert!(choices
        .iter()
        .any(|tool| tool.id == "zsh-syntax-highlighting"));
    validate_install_routes(&["zsh-syntax-highlighting".into()], apt).unwrap();
}
