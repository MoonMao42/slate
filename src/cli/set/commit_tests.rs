use super::*;
use crate::config::{ConfigManager, OriginalFileState, RestoreFileResult, RestorePoint};
use std::{
    cell::Cell,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};

fn write(path: &Path, bytes: impl AsRef<[u8]>, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

// Exercise the real coordinator and checkpoint without native discovery: these
// two selected adapters are file-only. Auto appearance is tested separately;
// freezing its policy here keeps all test execution inside the injected home.
fn apply_selected(env: &SlateEnv, theme: &ThemeVariant) -> Result<ThemeApplyReport> {
    ThemeApplyCoordinator::new(env)
        .including_opacity()
        .deferring_terminal_reload()
        .preserving_auto_pair()
        .apply_to_tools(theme, &["alacritty".into(), "ls_colors".into()])
}

#[test]
fn picker_commit_reloads_only_after_both_stages_and_never_after_failure() {
    use std::cell::RefCell;
    for failure in ["none", "theme-error", "theme-report", "opacity"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let order = RefCell::new(Vec::new());
        let result = silent_commit_apply_with_effects(
            &env,
            "nord",
            OpacityPreset::Frosted,
            |env, theme| {
                order.borrow_mut().push("theme");
                if failure == "theme-error" {
                    return Err(SlateError::Internal("fixture theme failure".into()));
                }
                let mut report = apply_selected(env, theme)?;
                if failure == "theme-report" {
                    report.commit_failure = Some(apply::ThemeCommitFailure {
                        stage: apply::ThemeCommitStage::CurrentTheme,
                        error: SlateError::Internal("fixture commit failure".into()),
                    });
                }
                Ok(report)
            },
            |env, opacity, options| {
                order.borrow_mut().push("opacity");
                assert!(
                    !options.reload_terminals,
                    "opacity must not reload halfway through commit"
                );
                apply::apply_opacity(env, opacity, options)?;
                if failure == "opacity" {
                    return Err(SlateError::Internal("fixture opacity failure".into()));
                }
                Ok(())
            },
            |env, _report| {
                order.borrow_mut().push("reload");
                let config = ConfigManager::from_env_paths(env);
                assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
                assert_eq!(
                    config.get_current_opacity_preset().unwrap(),
                    OpacityPreset::Frosted
                );
                let lock = config::write_guard::open_lock(env, false).unwrap().unwrap();
                assert!(
                    !config::write_guard::try_lock(&lock).unwrap(),
                    "writer released before reload"
                );
            },
        );
        assert_eq!(result.is_ok(), failure == "none");
        let expected = match failure {
            "none" => vec!["theme", "opacity", "reload"],
            "opacity" => vec!["theme", "opacity"],
            _ => vec!["theme"],
        };
        assert_eq!(*order.borrow(), expected, "{failure}");
    }
}

fn latest_selection(env: &SlateEnv) -> RestorePoint {
    config::list_restore_points_with_env(env)
        .unwrap()
        .into_iter()
        .find(|p| p.theme_name == "pre-theme")
        .unwrap()
}

#[test]
fn picker_repeated_commits_recover_latest_selection_without_refreshing_failed_choice() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config
        .set_current_opacity_preset(OpacityPreset::Solid)
        .unwrap();
    let mut previous = ("nord", OpacityPreset::Solid);
    let refreshes = Cell::new(0);
    for (theme_id, opacity, fail) in [
        ("catppuccin-mocha", OpacityPreset::Frosted, false),
        ("catppuccin-latte", OpacityPreset::Solid, false),
        ("nord", OpacityPreset::Clear, true),
        ("nord", OpacityPreset::Clear, false),
        ("catppuccin-mocha", OpacityPreset::Frosted, false),
    ] {
        let before_refreshes = refreshes.get();
        let terminal_file = env.xdg_config_home().join("alacritty/alacritty.toml");
        let previous_bytes = fs::read(&terminal_file).ok();
        let result = silent_commit_apply_with_effects(
            &env,
            theme_id,
            opacity,
            apply_selected,
            |env, selected, options| {
                assert!(!options.reload_terminals);
                apply::apply_opacity(env, selected, options)?;
                if fail {
                    Err(SlateError::Internal(
                        "fixture interrupted opacity publication".into(),
                    ))
                } else {
                    Ok(())
                }
            },
            |env, _| {
                assert!(!fail, "failed choice must never reach final refresh");
                refreshes.set(refreshes.get() + 1);
                let saved = ConfigManager::from_env_paths(env);
                assert_eq!(
                    saved.get_current_theme().unwrap().as_deref(),
                    Some(theme_id)
                );
                assert_eq!(saved.get_current_opacity_preset().unwrap(), opacity);
            },
        );
        if fail {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("fixture interrupted opacity publication"));
            assert_eq!(refreshes.get(), before_refreshes);
            assert_eq!(fs::read(&terminal_file).ok(), previous_bytes);
        } else {
            result.unwrap();
            assert_eq!(refreshes.get(), before_refreshes + 1);
            previous = (theme_id, opacity);
        }
        assert_eq!(
            config.get_current_theme().unwrap().as_deref(),
            Some(previous.0)
        );
        assert_eq!(config.get_current_opacity_preset().unwrap(), previous.1);
    }
    assert_eq!(refreshes.get(), 4);
}

fn assert_original_files(point: &RestorePoint) {
    for entry in &point.entries {
        match entry.original_state {
            OriginalFileState::Absent => assert_eq!(
                fs::symlink_metadata(&entry.original_path)
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound,
                "{}",
                entry.original_path.display()
            ),
            OriginalFileState::Present => {
                assert_eq!(
                    fs::read(&entry.original_path).unwrap(),
                    fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
                    "{}",
                    entry.original_path.display()
                );
                assert_eq!(
                    fs::metadata(&entry.original_path)
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    entry.unix_mode.unwrap(),
                    "{}",
                    entry.original_path.display()
                );
            }
        }
    }
}

#[test]
fn picker_commit_recovers_exact_bytes_modes_pairs_and_absence_after_opacity_failure() {
    for case in [
        "before-opacity-writes",
        "after-opacity-writes",
        "initially-unset",
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        let seeded = case != "initially-unset";
        if seeded {
            write(&env.managed_file("current"), b"nord\n", 0o640);
            write(&env.managed_file("current-opacity"), b"solid\n", 0o600);
            write(&env.managed_file("auto.toml"), b"# keep exact comments\ndark_theme = 'nord'\nlight_theme = 'catppuccin-latte'\nextra = 7\n", 0o640);
            write(
                &env.managed_file("managed/shell/env.zsh"),
                b"# custom shell bytes \xff\n",
                0o600,
            );
            write(
                &env.managed_file("managed/alacritty/colors.toml"),
                b"# custom colors \xff\n",
                0o640,
            );
            write(
                &env.managed_file("managed/kitty/opacity.conf"),
                b"# custom opacity file\n",
                0o640,
            );
            write(
                &env.slate_cache_dir().join("current_theme.lua"),
                b"-- custom old editor bytes\n",
                0o600,
            );
        }
        let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
        write(
            &alacritty,
            b"# user formatting\n[window]\npadding = {x = 7, y = 9}\n",
            0o640,
        );
        let external_cache = td.path().join("external-cache");
        write(&external_cache, b"original cache\n", 0o600);
        let theme_calls = Cell::new(0);
        let opacity_calls = Cell::new(0);
        let error = silent_commit_apply_with(
            &env,
            "catppuccin-mocha",
            OpacityPreset::Frosted,
            |env, theme| {
                theme_calls.set(theme_calls.get() + 1);
                let report = apply_selected(env, theme)?;
                // Model the successful manual preference update using its real
                // writer, without reading the host desktop appearance.
                ConfigManager::from_env_paths(env).write_auto_config(Some(&theme.id), None)?;
                Ok(report)
            },
            |env, opacity, options| {
                opacity_calls.set(opacity_calls.get() + 1);
                let lock_probe = config::write_guard::open_lock(env, false).unwrap().unwrap();
                assert!(!config::write_guard::try_lock(&lock_probe).unwrap());
                assert!(options.persist_state && !options.reload_terminals);
                assert_eq!(options.snapshot_policy, SnapshotPolicy::Skip);
                assert_eq!(
                    config.get_current_theme().unwrap().as_deref(),
                    Some("catppuccin-mocha")
                );
                write(&external_cache, b"cache side effect remains\n", 0o600);
                if case != "before-opacity-writes" {
                    apply::apply_opacity(
                        env,
                        opacity,
                        OpacityApplyOptions {
                            reload_terminals: false,
                            ..options
                        },
                    )?;
                    assert_eq!(
                        config.get_current_opacity_preset().unwrap(),
                        OpacityPreset::Frosted
                    );
                }
                Err(SlateError::Internal("injected opacity failure".into()))
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("injected opacity failure"), "{error}");
        assert!(
            error.contains("Pre-selection file bytes, permissions and prior absence were restored"),
            "{error}"
        );
        assert!(error.contains("External caches and live application state were not rolled back"));
        assert_eq!(theme_calls.get(), 1, "recovery must never rerun the theme");
        assert_eq!(opacity_calls.get(), 1, "recovery must never rerun opacity");
        let point = latest_selection(&env);
        assert_original_files(&point);
        assert!(error.contains(&format!("slate restore {} --dry-run", point.id)));
        assert_eq!(
            fs::read(&external_cache).unwrap(),
            b"cache side effect remains\n"
        );
        let points = config::list_restore_points_with_env(&env).unwrap();
        assert_eq!(
            points.len(),
            2,
            "one selection point plus one undo checkpoint"
        );
        let undo = points.iter().find(|p| p.id != point.id).unwrap();
        assert!(!undo.reapplies_theme());
        assert!(error.contains(&format!("slate restore {} --dry-run", undo.id)));
        // The saved pre-recovery state can itself be inspected and recovered.
        assert!(config::execute_restore_with_env(&env, &undo.id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(
            config.get_current_theme().unwrap().as_deref(),
            Some("catppuccin-mocha")
        );
        if case != "before-opacity-writes" {
            assert_eq!(
                config.get_current_opacity_preset().unwrap(),
                OpacityPreset::Frosted
            );
        }
    }
}

#[test]
fn picker_commit_success_keeps_selection_and_creates_no_recovery_undo() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let opacity_calls = Cell::new(0);
    silent_commit_apply_with(
        &env,
        "nord",
        OpacityPreset::Frosted,
        apply_selected,
        |env, opacity, options| {
            opacity_calls.set(opacity_calls.get() + 1);
            apply::apply_opacity(
                env,
                opacity,
                OpacityApplyOptions {
                    reload_terminals: false,
                    ..options
                },
            )
        },
    )
    .unwrap();
    assert_eq!(opacity_calls.get(), 1);
    let config = ConfigManager::from_env_paths(&env);
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    assert_eq!(
        config.get_current_opacity_preset().unwrap(),
        OpacityPreset::Frosted
    );
    assert_eq!(config::list_restore_points_with_env(&env).unwrap().len(), 1);
}

#[test]
fn theme_safety_picker_rejects_partial_apply_before_opacity_or_success() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config
        .set_current_opacity_preset(OpacityPreset::Solid)
        .unwrap();
    let path = env.xdg_config_home().join("alacritty/alacritty.toml");
    write(&path, "[invalid TOML\n", 0o600);
    let err = silent_commit_apply_with(
        &env,
        "catppuccin-mocha",
        OpacityPreset::Frosted,
        apply_selected,
        |_, _, _| panic!("opacity must not run after theme failure"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("alacritty"));
    assert!(err.to_string().contains("slate restore"));
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    assert_eq!(
        config.get_current_opacity_preset().unwrap(),
        OpacityPreset::Solid
    );
    assert_eq!(fs::read_to_string(path).unwrap(), "[invalid TOML\n");
    assert_eq!(config::list_restore_points_with_env(&env).unwrap().len(), 1);
}

#[test]
fn picker_commit_blocked_recovery_retains_both_errors_and_does_not_regenerate() {
    for fault in ["unsafe-target", "missing-backup"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_current_theme("nord").unwrap();
        let unrelated = td.path().join("untouched-link-target");
        write(&unrelated, b"untouched\n", 0o600);
        let calls = Cell::new(0);
        let point_id = std::cell::RefCell::new(String::new());
        let error = silent_commit_apply_with(
            &env,
            "catppuccin-mocha",
            OpacityPreset::Frosted,
            |env, theme| {
                calls.set(calls.get() + 1);
                let report = apply_selected(env, theme)?;
                *point_id.borrow_mut() = report.restore_point_id.clone().unwrap();
                Ok(report)
            },
            |env, _, _| {
                if fault == "unsafe-target" {
                    let target = env.managed_file("managed/ghostty/blur.conf");
                    fs::create_dir_all(target.parent().unwrap()).unwrap();
                    symlink(&unrelated, &target).unwrap();
                } else {
                    let point = latest_selection(env);
                    let entry = point
                        .entries
                        .iter()
                        .find(|e| e.original_path == env.managed_file("current"))
                        .unwrap();
                    fs::remove_file(entry.backup_path.as_ref().unwrap()).unwrap();
                    // Private fixture only.
                }
                Err(SlateError::Internal("injected opacity failure".into()))
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("injected opacity failure"));
        assert!(
            error.contains("Automatic file recovery could not complete"),
            "{error}"
        );
        assert!(error.contains("no theme was regenerated as a fallback"));
        assert!(!error
            .contains("Pre-selection file bytes, permissions and prior absence were restored"));
        assert_eq!(calls.get(), 1);
        assert_eq!(
            config.get_current_theme().unwrap().as_deref(),
            Some("catppuccin-mocha")
        );
        assert_eq!(fs::read(&unrelated).unwrap(), b"untouched\n");
        let inventory = config::inspect_restore_points_with_env(&env).unwrap();
        assert_eq!(
            inventory.points.len() + inventory.issues.len(),
            1,
            "blocked restore must not save undo or start writes"
        );
        if fault == "missing-backup" {
            assert_eq!(
                inventory.issues[0].id.as_deref(),
                Some(point_id.borrow().as_str())
            );
        } else {
            assert_eq!(inventory.points[0].id, *point_id.borrow());
        }
        assert!(error.contains(&format!("slate restore {} --dry-run", point_id.borrow())));
    }
}

#[test]
fn picker_commit_recovery_rejects_missing_and_wrong_kind_checkpoints() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    assert!(recover_selection_files(&env, None)
        .unwrap_err()
        .to_string()
        .contains("no pre-theme recovery point"));
    let point = config::snapshot_current_state_with_env(&env, "nord").unwrap();
    config.set_current_theme("catppuccin-mocha").unwrap();
    let error = recover_selection_files(&env, Some(&point.id))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("not a file-only pre-theme checkpoint"),
        "{error}"
    );
    assert_eq!(
        config.get_current_theme().unwrap().as_deref(),
        Some("catppuccin-mocha")
    );
    assert_eq!(config::list_restore_points_with_env(&env).unwrap().len(), 1);
}

#[test]
fn picker_commit_partial_or_empty_recovery_receipt_never_claims_success() {
    for results in [
        vec![],
        vec![RestoreFileResult {
            tool_key: "fixture".into(),
            display_tool: "fixture".into(),
            original_path: "/private-fixture/current".into(),
            success: false,
            error: Some("injected restore failure".into()),
        }],
    ] {
        let partial = !results.is_empty();
        let error = opacity_failure(
            SlateError::Internal("original opacity failure".into()),
            Some("selection-point"),
            Ok(RestoreReceipt {
                restore_point_id: "selection-point".into(),
                pre_restore_point_id: "undo-point".into(),
                theme_name: "pre-theme".into(),
                results,
            }),
        )
        .to_string();
        assert!(error.contains("original opacity failure"));
        assert!(
            error.contains("Automatic file recovery was incomplete"),
            "{error}"
        );
        assert!(!error
            .contains("Pre-selection file bytes, permissions and prior absence were restored"));
        assert!(error.contains("slate restore selection-point --dry-run"));
        assert!(error.contains("slate restore undo-point --dry-run"));
        if partial {
            assert!(error.contains("injected restore failure"));
        }
    }
}
