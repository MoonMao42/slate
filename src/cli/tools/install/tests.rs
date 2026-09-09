//! Installer callbacks here are fixtures, never native package managers.
use super::*;
use crate::platform::packages::{PackageManagerBackend, ToolInstallRoute};
use std::{cell::Cell, fs};

fn context(package_manager: PackageManagerBackend) -> InstallContext {
    InstallContext {
        package_manager,
        supported_os: true,
    }
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    (home, env)
}

#[test]
fn install_review_covers_catalog_routes_without_initializing_any_files() {
    let (home, env) = fixture();
    assert_eq!(super::super::installable_tools().len(), 10);
    for id in super::super::installable_tools() {
        let plan = prepare_with(&env, id, context(PackageManagerBackend::Homebrew), false).unwrap();
        assert_eq!(plan.review.action, Action::InstallMissing);
        assert_eq!(
            plan.plan.as_ref().unwrap().tool(id).unwrap().metadata.id,
            id
        );
        assert!(render(&plan.review)
            .contains("Package managers may install or change its dependencies"));
        assert_eq!(plan.review.fallback.is_some(), id == "starship");
    }
    let local = prepare_with(
        &env,
        "starship",
        context(PackageManagerBackend::Unsupported),
        false,
    )
    .unwrap();
    assert_eq!(
        local.plan.unwrap().tool("starship").unwrap().route,
        ToolInstallRoute::UserLocalStarship
    );
    assert!(prepare_with(&env, "yazi", context(PackageManagerBackend::Apt), false).is_err());
    assert!(prepare_with(&env, "zellij", context(PackageManagerBackend::Apt), false).is_err());
    // ANSI-FIXTURE: raw input for escaping or width checks.
    for id in ["ghostty", "tmux", "nvim", "opencode", "BAD\x1b[31m\n"] {
        assert!(validate_name(id).is_err());
    }
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn install_already_detected_is_a_readonly_noop_even_without_an_install_route() {
    let (home, env) = fixture();
    let plan = prepare_with(
        &env,
        "yazi",
        context(PackageManagerBackend::Unsupported),
        true,
    )
    .unwrap();
    assert_eq!(plan.review.action, Action::AlreadyDetected);
    assert!(plan.review.route.is_none());
    assert!(execute_with(
        &env,
        &plan,
        || panic!("no inspection"),
        || panic!("no preflight"),
        |_, _| panic!("no installer")
    )
    .unwrap()
    .is_none());
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn install_confirmed_execution_requests_one_tool_and_keeps_existing_configuration() {
    let (_home, env) = fixture();
    fs::create_dir_all(env.config_dir()).unwrap();
    let files = ["current", "config.toml", "auto.toml", "font"];
    for file in files {
        fs::write(env.managed_file(file), format!("PRIVATE_{file}")).unwrap();
    }
    let plan = prepare_with(
        &env,
        "btop",
        context(PackageManagerBackend::Homebrew),
        false,
    )
    .unwrap();
    let calls = Cell::new(0);
    let method = execute_with(
        &env,
        &plan,
        || {
            prepare_with(
                &env,
                "btop",
                context(PackageManagerBackend::Homebrew),
                false,
            )
        },
        || Ok(()),
        |selected, selected_env| {
            assert_eq!(selected.metadata.id, "btop");
            assert_eq!(selected.route, ToolInstallRoute::Homebrew);
            assert_eq!(selected_env.home(), env.home());
            let other_env = selected_env.clone();
            assert!(
                std::thread::spawn(move || ConfigWriteGuard::acquire(&other_env).is_err())
                    .join()
                    .unwrap()
            );
            calls.set(calls.get() + 1);
            // Stand-in for installation: no native process, only a private marker.
            fs::create_dir_all(selected_env.user_local_bin()).unwrap();
            fs::write(
                selected_env.user_local_bin().join("btop"),
                "fixture executable",
            )
            .unwrap();
            Ok(ToolInstallMethod::Homebrew)
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(method, ToolInstallMethod::Homebrew);
    for file in files {
        assert_eq!(
            fs::read_to_string(env.managed_file(file)).unwrap(),
            format!("PRIVATE_{file}")
        );
    }
    assert!(!env.managed_file("managed/shell/env.zsh").exists());
    assert!(!env.xdg_config_home().join("btop/btop.conf").exists());
    assert!(!env.slate_cache_dir().join("backups").exists());
    drop(ConfigWriteGuard::acquire(&env).unwrap());
}

#[test]
fn install_changed_reviews_preflight_failure_and_pending_recovery_stop_before_installer() {
    for stage in [0, 1, 2, 3] {
        let (_home, env) = fixture();
        let plan = prepare_with(
            &env,
            "btop",
            context(PackageManagerBackend::Homebrew),
            false,
        )
        .unwrap();
        if stage == 3 {
            fs::create_dir_all(env.slate_cache_dir()).unwrap();
            fs::write(
                env.slate_cache_dir().join("preview-session.json"),
                "pending fixture",
            )
            .unwrap();
        }
        let observations = Cell::new(0);
        let result = execute_with(
            &env,
            &plan,
            || {
                let index = observations.get();
                observations.set(index + 1);
                prepare_with(
                    &env,
                    "btop",
                    context(if stage == 1 && index == 1 {
                        PackageManagerBackend::Apt
                    } else {
                        PackageManagerBackend::Homebrew
                    }),
                    stage == 0,
                )
            },
            || {
                if stage == 2 {
                    Err(SlateError::InvalidConfig("preflight fixture".into()))
                } else {
                    Ok(())
                }
            },
            |_, _| panic!("no installer may run"),
        );
        assert!(result.is_err());
        assert!(!env.config_dir().exists());
        if matches!(stage, 0 | 2) {
            assert!(!env.slate_cache_dir().exists());
        }
    }
}

#[test]
fn install_uncertain_result_is_preserved_without_a_retry_or_configuration_phase() {
    let (_home, env) = fixture();
    let plan = prepare_with(
        &env,
        "starship",
        context(PackageManagerBackend::Homebrew),
        false,
    )
    .unwrap();
    let calls = Cell::new(0);
    let result = execute_with(
        &env,
        &plan,
        || {
            prepare_with(
                &env,
                "starship",
                context(PackageManagerBackend::Homebrew),
                false,
            )
        },
        || Ok(()),
        |_, _| {
            calls.set(calls.get() + 1);
            Err(SlateError::HomebrewInstallUncertain("fixture".into()))
        },
    );
    assert!(matches!(
        result,
        Err(SlateError::HomebrewInstallUncertain(_))
    ));
    assert_eq!(calls.get(), 1);
    assert!(!env.config_dir().exists());
    assert!(!env.user_local_bin().exists());
}

#[test]
fn install_chinese_receipt_distinguishes_detection_from_activation() {
    use crate::config::ui_language::UiLanguage;
    use crate::detection::{ToolEvidence, ToolPresence};
    let render = |presence: &ToolPresence| {
        completion_in_language(
            "btop",
            &ToolInstallMethod::Homebrew,
            presence,
            UiLanguage::Chinese,
        )
    };
    let error = render(&ToolPresence::missing()).unwrap_err().to_string();
    assert!(error.contains("仍未检测到 btop") && error.contains("包变更未回滚"));
    let active = ToolPresence::in_path_with(ToolEvidence::Executable("/fixture/btop".into()));
    let text = render(&active).unwrap();
    assert!(text.contains("尚未配置主题、字体或启用 Shell/编辑器集成"));
    assert!(
        text.contains("slate tools info btop") && text.contains("slate tools sync btop --dry-run")
    );
    let fallback = ToolPresence::fallback_with(ToolEvidence::Executable(
        "/fixture/line\n\x1b[2J/btop".into(),
    ));
    let text = render(&fallback).unwrap();
    assert!(text.contains("不在当前 PATH") && text.contains("Slate 未更改 PATH"));
    assert!(!text.contains('\x1b') && !text.contains("line\n"));
    let config = ToolPresence::installed_with(ToolEvidence::Config("/fixture/config".into()));
    assert!(render(&config)
        .unwrap()
        .contains("尚未确认 PATH 中存在可执行文件"));
}

#[test]
fn install_receipt_requires_post_install_detection_and_keeps_review_paths_terminal_safe() {
    use crate::detection::{ToolEvidence, ToolPresence};
    assert!(completion(
        "btop",
        &ToolInstallMethod::Homebrew,
        &ToolPresence::missing()
    )
    .unwrap_err()
    .to_string()
    .contains("still not detected"));
    let active = ToolPresence::in_path_with(ToolEvidence::Executable("/fixture/btop".into()));
    assert!(completion("btop", &ToolInstallMethod::Homebrew, &active)
        .unwrap()
        .contains("Slate did not configure themes"));
    let fallback = ToolPresence::fallback_with(ToolEvidence::Executable(
        "/fixture/line\n\x1b[2J/btop".into(),
    ));
    let receipt = completion("btop", &ToolInstallMethod::Homebrew, &fallback).unwrap();
    assert!(receipt.contains("outside the current PATH"));
    assert!(receipt.contains("Slate did not change PATH"));
    assert!(!receipt.contains('\x1b'));
    assert!(!receipt.contains("line\n"));
    let config_only = ToolPresence::installed_with(ToolEvidence::Config("/fixture/config".into()));
    assert!(
        completion("btop", &ToolInstallMethod::Homebrew, &config_only)
            .unwrap()
            .contains("did not establish an executable in PATH")
    );
    let (_home, env) = fixture();
    // ANSI-FIXTURE: raw input for escaping or width checks.
    let env = SlateEnv::with_home(env.home().join("line\n\x1b[31m"));
    let plan = prepare_with(
        &env,
        "starship",
        context(PackageManagerBackend::Homebrew),
        false,
    )
    .unwrap();
    assert!(!render(&plan.review).contains('\x1b'));
    assert!(!env.home().exists());
}
