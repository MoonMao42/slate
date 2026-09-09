//! Captured policy/rendering only: no installers, ambient profile writes or network.
use super::*;
use crate::{
    cli::tool_selection::{InstallAction, ReviewReceipt},
    detection::TerminalProfile,
    platform::packages::PackageManagerBackend,
};

fn context(backend: PackageManagerBackend) -> InstallContext {
    InstallContext {
        package_manager: backend,
        supported_os: true,
    }
}
fn receipt(ids: &[&str]) -> ReviewReceipt {
    let mut receipt = ReviewReceipt::new();
    for id in ids {
        receipt.add_install_action(InstallAction::from_metadata(
            &ToolCatalog::get_tool(id).unwrap(),
        ));
    }
    receipt
}

#[test]
fn install_review_chinese_routes_keep_commands_permissions_and_destinations() {
    use crate::config::ui_language::UiLanguage::{Chinese, English};
    let env = SlateEnv::with_home(PathBuf::from("/private/profile\n\x1bname"));
    let ids = ["starship".into(), "delta".into()];
    let apt = InstallPlan::capture(&ids, &env, context(PackageManagerBackend::Apt)).unwrap();
    let delta = apt.description_in("delta", Chinese).unwrap();
    assert!(delta.contains("git-delta") && delta.contains("管理员权限"));
    let local = apt.description_in("starship", Chinese).unwrap();
    assert!(local.contains(".local/bin/starship") && local.contains("curl"));
    assert!(!local.contains('\x1b') && !local.contains('\n'));
    assert_eq!(
        apt.description_in("delta", English),
        apt.description("delta")
    );
    let brew = InstallPlan::capture(&ids, &env, context(PackageManagerBackend::Homebrew)).unwrap();
    let fallback = brew.fallback_description_in(Chinese).unwrap();
    assert!(
        fallback.contains("安装结果不明确时会停止") && fallback.contains(".local/bin/starship")
    );
    assert_eq!(
        brew.description_in("delta", Chinese),
        brew.description("delta")
    );
    assert!(apt.fallback_description_in(Chinese).is_none());
}

#[test]
fn install_review_renders_real_backend_package_and_escaped_local_destination() {
    let env = SlateEnv::with_home(PathBuf::from("/private/profile\n\u{1b}name"));
    let terminal = TerminalProfile::from_env_vars(None, None);
    let ids = ["starship".into(), "delta".into()];
    let apt = InstallPlan::capture(&ids, &env, context(PackageManagerBackend::Apt)).unwrap();
    let text =
        receipt(&["starship", "delta"]).format_with_install_plan(None, &terminal, Some(&apt));
    assert!(text.contains("apt package (git-delta; administrator access)"));
    assert!(
        text.contains("Backups cover configuration files, not packages or installed executables")
    );
    assert!(text.contains("user-local executable") && text.contains(".local/bin/starship"));
    assert!(text.contains("profile\\n\\u{1b}name"));
    assert!(!text.contains('\u{1b}') && !text.contains("Homebrew") && !text.contains("formula"));
    assert!(apt.fallback_description().is_none());
    let brew = InstallPlan::capture(&ids, &env, context(PackageManagerBackend::Homebrew)).unwrap();
    let text =
        receipt(&["starship", "delta"]).format_with_install_plan(None, &terminal, Some(&brew));
    assert!(text.contains("Homebrew formula (delta)"));
    assert!(text.contains("permission/missing-brew failures may fall back"));
    assert!(text.contains("unconfirmed installer outcomes stop setup"));
    let local = InstallPlan::capture(
        &["starship".into()],
        &env,
        context(PackageManagerBackend::Unsupported),
    )
    .unwrap();
    assert_eq!(local.description("starship"), apt.description("starship"));
    assert!(local.description("delta").is_none());
}

#[test]
fn install_review_rejects_invalid_routes_and_deduplicates_exact_catalog_actions() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    for id in ["unknown\u{1b}", "ghostty", "tmux"] {
        let error =
            InstallPlan::capture(&[id.into()], &env, context(PackageManagerBackend::Homebrew))
                .unwrap_err();
        assert!(!error.to_string().contains('\u{1b}'));
    }
    assert!(InstallPlan::capture(
        &["fastfetch".into()],
        &env,
        context(PackageManagerBackend::Unsupported)
    )
    .is_err());
    let plan = InstallPlan::capture(
        &["bat".into(), "bat".into(), "delta".into()],
        &env,
        context(PackageManagerBackend::Apt),
    )
    .unwrap();
    assert_eq!(
        plan.tools
            .iter()
            .map(|tool| tool.metadata.id)
            .collect::<Vec<_>>(),
        ["bat", "delta"]
    );
    plan.verify_selection(
        &[
            ToolCatalog::get_tool("bat").unwrap(),
            ToolCatalog::get_tool("delta").unwrap(),
        ],
        &env,
    )
    .unwrap();
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn install_review_checks_selection_profile_and_route_without_silent_substitution() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let brew = context(PackageManagerBackend::Homebrew);
    let apt = context(PackageManagerBackend::Apt);
    let missing = context(PackageManagerBackend::Unsupported);
    let plan = InstallPlan::capture(&["starship".into(), "delta".into()], &env, brew).unwrap();
    plan.verify_context(brew).unwrap();
    for current in [
        apt,
        missing,
        InstallContext {
            supported_os: false,
            ..brew
        },
    ] {
        assert!(plan
            .verify_context(current)
            .unwrap_err()
            .to_string()
            .contains("plan changed"));
    }
    assert!(plan
        .verify_selection(&[ToolCatalog::get_tool("delta").unwrap()], &env)
        .is_err());
    assert!(plan
        .verify_selection(
            &[
                ToolCatalog::get_tool("delta").unwrap(),
                ToolCatalog::get_tool("starship").unwrap()
            ],
            &env
        )
        .is_err());
    let selected = [
        ToolCatalog::get_tool("starship").unwrap(),
        ToolCatalog::get_tool("delta").unwrap(),
    ];
    assert!(plan
        .verify_selection(&selected, &SlateEnv::with_home(temp.path().join("other")))
        .is_err());
    let mut changed_package = selected;
    changed_package[1].brew_package = "unreviewed-package";
    assert!(plan.verify_selection(&changed_package, &env).is_err());
    // An unrelated package-manager change need not invalidate an unchanged
    // user-local route; switching that route to Homebrew does require review.
    let local = InstallPlan::capture(&["starship".into()], &env, missing).unwrap();
    local.verify_context(apt).unwrap();
    assert!(local.verify_context(brew).is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
