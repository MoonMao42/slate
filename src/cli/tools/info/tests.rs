use super::*;
use crate::platform::packages::PackageManagerBackend;

#[test]
fn configuration_only_tool_recommends_inspection_not_installation() {
    use crate::detection::{ToolEvidence, ToolPresence};
    for evidence in [
        ToolEvidence::Config("/fixture/starship.toml".into()),
        ToolEvidence::Plugin("/fixture/plugin".into()),
        ToolEvidence::AppBundle("/fixture/application".into()),
    ] {
        let mut item = tool("starship", Some(true));
        item.detection =
            inventory::Detection::from_presence(&ToolPresence::installed_with(evidence));
        assert_eq!(item.detection.as_ref().unwrap().executable_in_path, None);
        let info = describe(
            item,
            Some("nord".into()),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        assert_eq!(info.recommended_action.action, "check");
        assert_eq!(
            info.recommended_action.command.as_deref(),
            Some("slate doctor starship")
        );
        assert_ne!(info.tool.status_label(), "in PATH");
        assert!(info.sync_review_available);
        let page = render_menu_details(&info);
        assert!(page.contains("此路径不能证明命令可运行或配色已启用"));
        assert!(page.contains("建议下一步：slate doctor starship"));
        assert!(page.contains("完整说明：slate tools info starship"));
        assert!(!page.contains("仅确认 PATH 中存在命令"));
        assert!(!page.contains("Next steps:"));
    }
}

#[test]
fn discovery_purposes_are_short_and_cover_all_adapters_without_replacing_details() {
    for id in super::super::supported_tools() {
        let short = menu_purpose(id);
        assert_ne!(short, "工具配色", "missing purpose for {id}");
        assert!(short.chars().count() <= 24, "{id}: {short}");
        assert!(!short.chars().any(char::is_control));
        let info = describe(
            tool(id, Some(true)),
            Some("nord".into()),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        let details = render(&info);
        assert!(details.contains(purpose(id)));
        assert!(
            details.contains(inventory::hint(id)),
            "lost sync advice for {id}"
        );
    }
}

fn context(package_manager: PackageManagerBackend) -> InstallContext {
    InstallContext {
        package_manager,
        supported_os: true,
    }
}

fn tool(id: &'static str, available: Option<bool>) -> inventory::Tool {
    inventory::Tool {
        id,
        label: id,
        available,
        detection: None,
        hint: inventory::hint(id),
    }
}

#[test]
fn ghostty_menu_is_concise_without_disguising_real_warnings() {
    use crate::detection::{ToolEvidence, ToolPresence};
    let mut item = tool("ghostty", Some(true));
    item.detection = inventory::Detection::from_presence(&ToolPresence::installed_with(
        ToolEvidence::AppBundle("/Applications/Ghostty.app".into()),
    ));
    let mut info = describe(
        item,
        Some("catppuccin-frappe".into()),
        None,
        context(PackageManagerBackend::Homebrew),
        true,
    );
    let menu = render_menu(&info);
    assert!(menu.lines().count() <= 5, "{menu}");
    assert!(menu.contains("找到应用"));
    assert!(menu.contains("未验证配色生效"));
    assert!(menu.contains("Slate 已保存主题：Catppuccin Frappé"));
    assert!(menu.contains("同步可能重载终端窗口"));
    for hidden in [
        "Installation:",
        "Detected via",
        "executable-in-PATH",
        "Next steps:",
        "slate setup",
        "/Applications",
    ] {
        assert!(!menu.contains(hidden), "unrequested diagnostics: {hidden}");
    }
    assert!(render(&info).contains("Detected via app_bundle"));
    info.warning = Some("Saved theme is unreadable; inspect before replacing it.");
    assert!(render_menu(&info).contains("Saved theme is unreadable"));
}

#[test]
fn tool_menu_distinguishes_missing_unknown_and_unreadable_theme() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let missing = inspect(&env, "ghostty").unwrap();
    assert!(render_menu(&missing).contains("Slate 已保存主题：未选择"));
    std::fs::create_dir_all(env.config_dir()).unwrap();
    let current = env.managed_file("current");
    std::fs::write(&current, "unknown-theme\n").unwrap();
    let unknown = inspect(&env, "ghostty").unwrap();
    assert!(render_menu(&unknown).contains("Slate 已保存主题：无法识别"));
    assert!(unknown.theme_selection_available);
    assert!(!unknown.sync_review_available);
    std::fs::remove_file(&current).unwrap();
    std::fs::create_dir(&current).unwrap();
    let unreadable = inspect(&env, "ghostty").unwrap();
    let menu = render_menu(&unreadable);
    assert!(menu.contains("Slate 已保存主题：需检查"));
    assert!(menu.contains("已保存主题无法读取"));
    assert!(!unreadable.theme_selection_available);
    assert!(!unreadable.sync_review_available);
    let json = serde_json::to_value(&unreadable).unwrap();
    assert!(json.get("menu_notice").is_none());
    assert_eq!(json["warning"], unreadable.warning.unwrap());
    assert!(current.is_dir());
    assert!(!env.slate_cache_dir().exists());
}

#[test]
fn menu_theme_names_are_readable_without_changing_diagnostic_ids() {
    for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
        let info = describe(
            tool("btop", Some(true)),
            Some(theme.id.clone()),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        assert!(render_menu(&info).contains(&format!("Slate 已保存主题：{}", theme.name)));
        assert!(render(&info).contains(&format!("Saved theme: {}", theme.id)));
    }
    let info = describe(
        tool("btop", Some(true)),
        Some("unknown\n\x1b[2J".into()),
        None,
        context(PackageManagerBackend::Homebrew),
        true,
    );
    let menu = render_menu(&info);
    assert!(!menu.contains('\x1b'));
    assert!(!menu.contains("unknown\n"));
}

#[test]
fn tool_status_labels_preserve_missing_and_unknown_states() {
    use crate::detection::{ToolEvidence, ToolPresence};
    for (evidence, expected, menu_status) in [
        (
            ToolEvidence::Executable("/fixture/tool".into()),
            "outside PATH",
            "找到程序，但不在 PATH 中",
        ),
        (
            ToolEvidence::AppBundle("/fixture/app".into()),
            "app found",
            "找到应用",
        ),
        (
            ToolEvidence::Config("/fixture/config".into()),
            "config found",
            "仅找到配置",
        ),
        (
            ToolEvidence::Plugin("/fixture/plugin".into()),
            "plugin found",
            "找到插件",
        ),
    ] {
        let mut item = tool("starship", Some(true));
        item.detection =
            inventory::Detection::from_presence(&ToolPresence::installed_with(evidence));
        assert_eq!(item.status_label(), expected);
        assert_eq!(item.menu_status_label(), menu_status);
        let mut info = describe(
            item,
            Some("nord".into()),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        assert!(render(&info).contains(&format!("Availability: {expected}")));
        assert!(render_menu(&info).contains(menu_status));
        assert!(render_menu(&info).contains("未验证配色生效"));
        info.tool.available = Some(false);
        assert_eq!(info.tool.status_label(), "not detected");
        assert!(render_menu(&info).contains("未检测到"));
        info.tool.available = None;
        assert_eq!(info.tool.status_label(), "unknown");
        assert!(render_menu(&info).contains("检测状态未知"));
    }
    let mut active = tool("starship", Some(true));
    assert_eq!(active.status_label(), "detected");
    active.detection = inventory::Detection::from_presence(&ToolPresence::in_path_with(
        ToolEvidence::Executable("/fixture/tool".into()),
    ));
    assert_eq!(active.status_label(), "in PATH");
}

#[test]
fn tool_details_distinguish_detection_evidence_from_shell_command_availability() {
    use crate::detection::{ToolEvidence, ToolPresence};
    let path = std::path::PathBuf::from("/fixture/personal\n\x1b[2J");
    for (evidence, kind, in_path) in [
        (
            ToolEvidence::Executable(path.clone()),
            "executable",
            Some(false),
        ),
        (ToolEvidence::AppBundle(path.clone()), "app_bundle", None),
        (ToolEvidence::Config(path.clone()), "configuration", None),
        (ToolEvidence::Plugin(path.clone()), "plugin", None),
    ] {
        let mut item = tool("starship", Some(true));
        item.detection =
            inventory::Detection::from_presence(&ToolPresence::installed_with(evidence));
        let info = describe(
            item,
            Some("nord".into()),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        let text = render(&info);
        assert!(text.contains(&format!("Detected via {kind}:")));
        assert!(!text.contains('\x1b'));
        assert!(!text.contains("personal\n"));
        let menu = render_menu_details(&info);
        assert!(!menu.contains('\x1b'));
        assert!(!menu.contains("personal\n"));
        assert!(menu.contains(if in_path.is_some() {
            "仅在备用位置找到程序"
        } else {
            "此路径不能证明命令可运行"
        }));
        let detected = info.tool.detection.as_ref().unwrap();
        assert_eq!(detected.executable_in_path, in_path);
        assert!(text.contains(if in_path.is_some() {
            "fallback location"
        } else {
            "not executable-in-PATH evidence"
        }));
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["tool"]["detection"]["kind"], kind);
    }
    let active = inventory::Detection::from_presence(&ToolPresence::in_path_with(
        ToolEvidence::Executable(path),
    ))
    .unwrap();
    assert_eq!(active.executable_in_path, Some(true));
    assert!(inventory::Detection::from_presence(&ToolPresence::missing()).is_none());
}

#[test]
fn discovery_covers_every_adapter_without_claiming_activation_or_installability() {
    for id in supported_tools() {
        validate_id(id).unwrap();
        assert_ne!(purpose(id), "Theme adapter", "{id}");
        let info = describe(
            tool(id, Some(true)),
            Some("nord".into()),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        assert!(info.sync_review_available);
        assert_eq!(
            info.installation.guided_install,
            ToolCatalog::get_tool(id).is_some_and(|tool| tool.installable)
        );
        let output = render(&info);
        assert!(output.contains("not proof of configuration or live theme activation"));
        assert!(output.contains(&format!("slate tools sync {id} --dry-run")));
    }
    // ANSI-FIXTURE: raw input for escaping or width checks.
    for id in ["ls_colors", "nerd-font", "BAD\x1b[31m", ""] {
        assert!(validate_id(id).is_err());
    }
}

#[test]
fn discovery_distinguishes_missing_theme_tool_and_existing_integration() {
    for availability in [None, Some(false), Some(true)] {
        for theme in [None, Some("nord".into())] {
            let info = describe(
                tool("btop", availability),
                theme.clone(),
                None,
                context(PackageManagerBackend::Homebrew),
                true,
            );
            assert_eq!(
                info.sync_review_available,
                availability == Some(true) && theme.is_some()
            );
            let output = render(&info);
            assert_eq!(
                output.contains("slate tools sync btop --dry-run"),
                info.sync_review_available
            );
            assert_eq!(output.contains("Save a recognized theme"), theme.is_none());
            assert_eq!(
                output.contains("This tool is not detected"),
                availability == Some(false)
            );
        }
    }
    let info = describe(
        tool("starship", Some(true)),
        Some("nord".into()),
        None,
        context(PackageManagerBackend::Homebrew),
        true,
    );
    assert!(render(&info).contains("Syncing colors alone does not activate shell hooks"));
}

#[test]
fn installation_guidance_reuses_setup_routes_and_respects_detect_only_tools() {
    assert_eq!(
        installation("btop", context(PackageManagerBackend::Apt)).route,
        "apt"
    );
    assert!(installation("yazi", context(PackageManagerBackend::Apt))
        .description
        .contains("no apt mapping"));
    assert_eq!(
        installation("yazi", context(PackageManagerBackend::Homebrew)).route,
        "homebrew"
    );
    assert_eq!(
        installation("starship", context(PackageManagerBackend::Unsupported)).route,
        "user-local-starship"
    );
    for id in ["nvim", "opencode", "ghostty", "kitty", "alacritty", "tmux"] {
        let advice = installation(id, context(PackageManagerBackend::Homebrew));
        assert!(!advice.guided_install);
        assert_eq!(advice.route, "manual");
    }
}

#[test]
fn recommended_tool_action_tracks_prerequisites_and_existing_menu_choices() {
    for (id, available, theme, expected) in [
        ("btop", None, Some("nord"), "refresh"),
        ("btop", Some(false), Some("nord"), "install"),
        ("nvim", Some(false), Some("nord"), "refresh"),
        ("btop", Some(true), None, "theme"),
        ("lazygit", Some(true), Some("nord"), "check"),
        ("delta", Some(true), Some("nord"), "preview"),
    ] {
        let info = describe(
            tool(id, available),
            theme.map(str::to_owned),
            None,
            context(PackageManagerBackend::Homebrew),
            true,
        );
        assert_eq!(info.recommended_action.action, expected);
        assert!(render(&info).contains(&format!("Recommended: {}", info.recommended_action.label)));
        if expected == "install" || expected == "preview" {
            assert!(info
                .recommended_action
                .command
                .as_ref()
                .unwrap()
                .ends_with("--dry-run"));
        }
        if expected == "theme" {
            assert!(info.recommended_action.reason.contains("detected adapters"));
            assert!(info.recommended_action.reason.contains("confirmation"));
        }
    }
    let info = describe(
        tool("yazi", Some(false)),
        None,
        None,
        context(PackageManagerBackend::Apt),
        true,
    );
    assert_eq!(info.recommended_action.action, "refresh");
    let unsafe_theme = describe(
        tool("btop", Some(true)),
        None,
        None,
        context(PackageManagerBackend::Homebrew),
        false,
    );
    assert_eq!(unsafe_theme.recommended_action.action, "refresh");
    assert!(!unsafe_theme.theme_selection_available);
}
