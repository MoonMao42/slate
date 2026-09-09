use super::*;

#[test]
fn skipped_sync_guidance_reports_actual_reasons_and_retains_partial_success() {
    use crate::adapter::{SkipReason, ToolApplyResult};
    let mut report = ThemeApplyReport {
        results: vec![ToolApplyResult {
            tool_name: "btop".into(),
            status: ToolApplyStatus::Applied,
            requires_new_shell: false,
        }],
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    };
    assert!(skipped_sync_error(&report).is_none());
    for (id, reason) in [
        ("delta", SkipReason::MissingIntegrationConfig),
        ("tmux", SkipReason::NotInstalled),
        ("nvim", SkipReason::ThemeNotCommitted),
    ] {
        report.results.push(ToolApplyResult {
            tool_name: id.into(),
            status: ToolApplyStatus::Skipped(reason.clone()),
            requires_new_shell: false,
        });
        let error = skipped_sync_error(&report).unwrap().to_string();
        assert!(error.contains(&format!("{id}: {reason}")));
        assert!(error.contains(&format!("slate tools info {id}")));
        assert!(error.contains("successful syncs were kept"));
        assert!(!error.contains("btop:"));
        assert!(!error.contains("slate setup"));
        assert!(!error.contains("slate restore --list"));
    }
    report.restore_point_id = Some("fixture".into());
    assert!(skipped_sync_error(&report)
        .unwrap()
        .to_string()
        .contains("slate restore --list"));
    assert!(follow_up(&report).contains("Reopen btop"));
    let compact = menu_follow_up(&report);
    assert!(compact.contains("btop · 配置已保存"));
    assert!(compact.contains("退出时写回旧配色"));
    assert!(compact.contains("未验证实时外观"));
    assert!(compact.contains("slate restore 'fixture' --dry-run"));
    assert!(!compact.contains("delta · 配置已保存"));
    assert!(!compact.contains("tmux · 配置已保存"));
}

#[test]
fn opencode_restart_guidance_does_not_request_a_new_shell_or_change_startup() {
    use crate::adapter::{
        ApplyOutcome, OpencodeAdapter, ToolAdapter, ToolApplyResult, ToolRegistry,
    };
    let (_home, env) = fixture();
    let config = env.xdg_config_home().join("opencode/tui.json");
    seed(
        &config,
        "{\"theme\":\"personal\",\"keybinds\":{\"leader\":\"ctrl+a\"}}",
    );
    seed(&env.zshrc_path(), "# PRIVATE_STARTUP\n");
    let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
    for _ in 0..2 {
        let outcome = OpencodeAdapter.apply_theme_with_env(&theme, &env).unwrap();
        assert_eq!(outcome, ApplyOutcome::applied_no_shell());
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(config["theme"], "system");
        assert_eq!(config["keybinds"]["leader"], "ctrl+a");
        assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"# PRIVATE_STARTUP\n");
    }
    let mut report = ThemeApplyReport {
        results: vec![ToolApplyResult {
            tool_name: "opencode".into(),
            status: ToolApplyStatus::Applied,
            requires_new_shell: false,
        }],
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    };
    crate::cli::apply::reload_theme_targets(&env, &ToolRegistry::new(), &mut report);
    assert_eq!(report.reload_warnings.len(), 1);
    let message = &report.reload_warnings[0].message;
    assert!(message.contains("Reopen OpenCode"));
    assert!(message.contains("did not close or restart"));
    assert!(message.contains("new shell is not required"));
    assert!(!crate::adapter::registry::requires_new_shell(
        &report.results
    ));
    let text = follow_up(&report);
    assert!(text.contains("Reopen OpenCode"));
    assert!(!text.contains("Open a new shell"));
    report.results[0].status =
        ToolApplyStatus::Skipped(crate::adapter::SkipReason::MissingIntegrationConfig);
    report.reload_warnings.clear();
    crate::cli::apply::reload_theme_targets(&env, &ToolRegistry::new(), &mut report);
    assert!(report.reload_warnings.is_empty());
}

#[test]
fn tmux_follow_up_distinguishes_server_reload_from_file_recovery() {
    let mut report = ThemeApplyReport {
        results: vec![crate::adapter::ToolApplyResult {
            tool_name: "tmux".into(),
            status: ToolApplyStatus::Applied,
            requires_new_shell: false,
        }],
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    };
    let text = follow_up(&report);
    for boundary in [
        "captured TMUX socket",
        "default server outside tmux",
        "other servers are unchanged",
        "never starts a server",
        "partial application",
        "new session on an existing server does not reread",
        "File recovery does not restore colors in a running server",
        "slate tools info tmux",
        "live appearance is not verified",
    ] {
        assert!(text.contains(boundary), "missing guidance: {boundary}");
    }
    assert!(!text.contains("Open a new shell"));
    report.results[0].status = ToolApplyStatus::Skipped(crate::adapter::SkipReason::NotInstalled);
    assert!(follow_up(&report).is_empty());
}

#[test]
fn missing_tool_guidance_stays_scoped_for_every_adapter() {
    for id in supported_tools() {
        let message = missing_tool(id).to_string();
        assert!(message.contains(&format!("slate tools info {id}")));
        assert!(message.contains("Sync never installs software"));
        assert!(message.contains("No tools were changed"));
        assert!(!message.contains("slate setup"));
        // Some adapters have no guided installation route.
        assert!(!message.contains("slate tools install"));
        super::super::info::validate_id(id).unwrap();
    }
}

#[test]
fn sync_follow_up_only_describes_applied_tools_and_does_not_claim_activation() {
    use crate::adapter::{SkipReason, ToolApplyResult};
    let mut report = ThemeApplyReport {
        results: vec![
            ToolApplyResult {
                tool_name: "eza".into(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: true,
            },
            ToolApplyResult {
                tool_name: "btop".into(),
                status: ToolApplyStatus::Failed(SlateError::InvalidConfig("failure".into())),
                requires_new_shell: false,
            },
            ToolApplyResult {
                tool_name: "yazi".into(),
                status: ToolApplyStatus::Skipped(SkipReason::NotInstalled),
                requires_new_shell: false,
            },
        ],
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    };
    let text = follow_up(&report);
    assert!(text.contains("live appearance is not verified"));
    assert!(text.contains("slate doctor eza"));
    assert!(text.contains("Sync did not regenerate shell startup"));
    assert!(!text.contains("btop"));
    assert!(!text.contains("yazi"));
    assert!(!text.contains("Synced"));
    report.results.remove(0);
    assert!(follow_up(&report).is_empty());
}
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

fn seed(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(&env.managed_file("current"), "nord\n");
    let binary = env.user_local_bin().join("btop");
    seed(&binary, "#!/bin/sh\nexit 91\n");
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    (home, env)
}

#[test]
fn partial_tool_sync_keeps_success_and_restores_without_creating_missing_gitconfig() {
    let (_home, env) = fixture();
    let delta = env.user_local_bin().join("delta");
    seed(&delta, "#!/bin/sh\nexit 91\n");
    fs::set_permissions(&delta, fs::Permissions::from_mode(0o755)).unwrap();
    let btop = crate::adapter::BtopAdapter::config_path(&env);
    let personal = "color_theme=personal\ntheme_background=false\n";
    seed(&btop, personal);
    fs::set_permissions(&btop, fs::Permissions::from_mode(0o640)).unwrap();
    let delta_palette = env.managed_file("managed/delta/colors");
    seed(&delta_palette, "# existing personal palette\n");
    let delta_before = fs::metadata(&delta_palette).unwrap();
    seed(&env.zshrc_path(), "# private startup\n");
    let plan = prepare(&env, &["btop".into(), "delta".into()]).unwrap();
    let report = execute(&env, &plan).unwrap();
    report.ensure_no_failures().unwrap();
    assert_eq!(report.applied_count(), 1);
    assert_eq!(report.skipped_count(), 1);
    assert!(report
        .results
        .iter()
        .any(|result| result.tool_name == "delta"
            && matches!(
                result.status,
                ToolApplyStatus::Skipped(crate::adapter::SkipReason::MissingIntegrationConfig)
            )));
    assert!(crate::adapter::BtopAdapter::theme_path(&env).is_file());
    assert_ne!(fs::read_to_string(&btop).unwrap(), personal);
    assert!(!env.home().join(".gitconfig").exists());
    assert_eq!(
        fs::read_to_string(&delta_palette).unwrap(),
        "# existing personal palette\n"
    );
    assert_eq!(
        fs::metadata(&delta_palette).unwrap().ino(),
        delta_before.ino()
    );
    let message = skipped_sync_error(&report).unwrap().to_string();
    assert!(message.contains("delta: missing integration config"));
    assert!(message.contains("slate restore --list"));
    assert!(follow_up(&report).contains("Reopen btop"));
    let id = report.restore_point_id.unwrap();
    assert!(crate::config::execute_restore_with_env(&env, &id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read_to_string(&btop).unwrap(), personal);
    assert_eq!(fs::metadata(&btop).unwrap().mode() & 0o777, 0o640);
    assert!(!crate::adapter::BtopAdapter::theme_path(&env).exists());
    assert!(!env.home().join(".gitconfig").exists());
    assert_eq!(
        fs::read_to_string(&delta_palette).unwrap(),
        "# existing personal palette\n"
    );
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"# private startup\n");
}

#[test]
fn tool_sync_scope_is_exact_and_restorable_without_global_publication() {
    let (_home, env) = fixture();
    let config = crate::adapter::BtopAdapter::config_path(&env);
    seed(&config, "color_theme=personal\ntheme_background=false\n");
    let mut sentinels = Vec::new();
    for path in crate::adapter::write_paths::shared_theme_paths(&env)
        .into_iter()
        .chain([
            env.managed_file("managed/ghostty/theme.conf"),
            env.zshrc_path(),
        ])
    {
        if path != env.managed_file("current") {
            seed(&path, "private sentinel\n");
        }
        sentinels.push((
            path.clone(),
            fs::read(&path).unwrap(),
            fs::metadata(&path).unwrap().ino(),
        ));
    }
    let names = ["btop".into(), "btop".into()];
    let plan = prepare(&env, &names).unwrap();
    assert_eq!(plan.tools, ["btop"]);
    assert_eq!(plan.configuration_paths.len(), 2);
    let report = execute(&env, &plan).unwrap();
    report.ensure_no_failures().unwrap();
    assert_eq!(report.applied_count(), 1);
    for (path, bytes, inode) in sentinels {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
        assert_eq!(fs::metadata(path).unwrap().ino(), inode);
    }
    let id = report.restore_point_id.unwrap();
    let point = crate::config::get_restore_point_with_env(&env, &id).unwrap();
    assert_eq!(point.entries.len(), 2);
    assert!(!point.reapplies_theme());
    assert!(crate::config::execute_restore_with_env(&env, &id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        fs::read(config).unwrap(),
        b"color_theme=personal\ntheme_background=false\n"
    );
    assert!(!crate::adapter::BtopAdapter::theme_path(&env).exists());
}

#[test]
fn tool_sync_invalidates_review_when_theme_changes_before_lock_or_writes() {
    let (_home, env) = fixture();
    let plan = prepare(&env, &["btop".into()]).unwrap();
    seed(&env.managed_file("current"), "catppuccin-latte");
    assert!(execute(&env, &plan)
        .unwrap_err()
        .to_string()
        .contains("changed after review"));
    assert!(!env.slate_cache_dir().exists());
    assert!(!crate::adapter::BtopAdapter::theme_path(&env).exists());
}

#[test]
fn tool_sync_review_rejects_configuration_edits_without_exposing_contents() {
    for change in [
        "content",
        "permissions",
        "replacement",
        "removed",
        "created",
    ] {
        let (_home, env) = fixture();
        let path = crate::adapter::BtopAdapter::config_path(&env);
        if change != "created" {
            seed(&path, "# PRIVATE_REVIEW_BYTES\ncolor_theme=personal\n");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let plan = prepare(&env, &["btop".into()]).unwrap();
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("PRIVATE_REVIEW_BYTES"));
        assert!(!format!("{plan:?}").contains("PRIVATE_REVIEW_BYTES"));
        match change {
            "content" => seed(&path, "# PRIVATE_LATER_EDIT\ncolor_theme=personal\n"),
            "permissions" => fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap(),
            "replacement" => {
                let replacement = path.with_extension("replacement");
                fs::write(&replacement, fs::read(&path).unwrap()).unwrap();
                fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
                fs::rename(replacement, &path).unwrap();
            }
            "removed" => fs::remove_file(&path).unwrap(),
            "created" => seed(&path, "# PRIVATE_NEW_FILE\n"),
            _ => unreachable!(),
        }
        let before = fs::read(&path).ok();
        let error = execute(&env, &plan).unwrap_err().to_string();
        assert!(error.contains("changed after review"), "{change}: {error}");
        assert!(!error.contains("PRIVATE_"));
        assert_eq!(fs::read(&path).ok(), before);
        assert!(!env.slate_cache_dir().exists());
        assert!(!crate::adapter::BtopAdapter::theme_path(&env).exists());
    }
}

#[test]
fn tool_sync_capture_error_names_the_target_and_reason_without_raw_controls() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("config-\u{1b}[2J\n");
    let file = fs::File::create(&path).unwrap();
    file.set_len(crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1)
        .unwrap();
    let error = capture_files(std::slice::from_ref(&path))
        .unwrap_err()
        .to_string();
    assert!(error.contains("config-\\u{1b}[2J\\n"), "{error}");
    assert!(error.contains("file size limit exceeded"));
    assert!(error.contains("Nothing was changed; file contents omitted"));
    assert!(!error.contains('\u{1b}'));
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1
    );
    let directory = home.path().join("directory.conf");
    fs::create_dir(&directory).unwrap();
    let error = capture_files(&[directory]).unwrap_err().to_string();
    assert!(error.contains("directory.conf"));
    assert!(error.contains("expected a regular file"));
}

#[test]
fn tool_sync_menu_english_preserves_review_and_recovery_details() {
    std::thread::spawn(|| {
        let (_home, env) = fixture();
        crate::config::ui_language::save(&env, crate::config::ui_language::UiLanguage::English)
            .unwrap();
        crate::cli::ui_language::load_saved_ui_language(&env).unwrap();
        let plan = prepare(&env, &["btop".into()]).unwrap();
        let compact = render_menu(&plan);
        for text in [
            "Sync Preview",
            "not a line-by-line diff",
            "no installs",
            "shell startup files",
            "recovery point",
            "application reloads",
            "not rolled back automatically",
            "new review",
            "slate tools sync btop --dry-run",
        ] {
            assert!(compact.contains(text), "missing {text}: {compact}");
        }
        for path in &plan.configuration_paths {
            assert!(compact.contains(&path.display().to_string()));
        }
        let report = ThemeApplyReport {
            results: vec![crate::adapter::ToolApplyResult {
                tool_name: "btop".into(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: false,
            }],
            commit_failure: None,
            reload_warnings: vec![],
            restore_point_id: Some("fixture".into()),
        };
        let receipt = menu_follow_up(&report);
        for text in [
            "Configuration saved",
            "Reopen btop",
            "live appearance was not verified",
            "slate restore 'fixture' --dry-run",
            "files only, not running state",
        ] {
            assert!(receipt.contains(text), "missing {text}: {receipt}");
        }
    })
    .join()
    .unwrap();
}

#[test]
fn tool_sync_menu_review_keeps_targets_and_consent_limits() {
    let (_home, env) = fixture();
    let plan = prepare(&env, &["btop".into()]).unwrap();
    let compact = render_menu(&plan);
    for path in &plan.configuration_paths {
        assert!(compact.contains(&path.display().to_string()));
    }
    for detail in [
        "不是逐行差异",
        "不安装软件",
        "Shell 启动文件",
        "创建恢复点",
        "可能生成缓存或重载应用",
        "不会自动回滚",
        "需重新审阅",
        "slate tools sync btop --dry-run",
        inventory::menu_sync_hint("btop"),
    ] {
        assert!(compact.contains(detail), "missing {detail}: {compact}");
    }
    assert!(!compact.contains(plan.notes[0]));
    assert!(render(&plan).contains(plan.notes[0]));
    assert!(render(&plan).contains(inventory::hint("btop")));
    assert!(!compact.contains(inventory::hint("btop")));
    assert!(!env.slate_cache_dir().exists());
}

#[test]
fn tool_sync_review_remains_available_during_recovery_but_apply_is_blocked() {
    let (_home, env) = fixture();
    seed(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_INVALID_RECORD",
    );
    let plan = prepare(&env, &["btop".into()]).unwrap();
    assert!(matches!(
        execute(&env, &plan),
        Err(SlateError::PreviewRecoveryPending)
    ));
    assert!(!crate::adapter::BtopAdapter::theme_path(&env).exists());
    assert!(!env.slate_cache_dir().join("backups").exists());
}

#[test]
fn tool_sync_invalidates_review_when_parent_directory_is_redirected() {
    use std::os::unix::fs::symlink;
    let (home, env) = fixture();
    let first = home.path().join("first-btop");
    let second = home.path().join("second-btop");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let link = env.xdg_config_home().join("btop");
    symlink(&first, &link).unwrap();
    let plan = prepare(&env, &["btop".into()]).unwrap();
    fs::remove_file(&link).unwrap();
    symlink(&second, &link).unwrap();
    assert!(execute(&env, &plan)
        .unwrap_err()
        .to_string()
        .contains("changed after review"));
    assert_eq!(fs::read_dir(first).unwrap().count(), 0);
    assert_eq!(fs::read_dir(second).unwrap().count(), 0);
    assert!(!env.slate_cache_dir().exists());
}
