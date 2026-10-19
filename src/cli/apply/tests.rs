use super::*;
use crate::theme::catppuccin;
use tempfile::TempDir;

#[test]
fn activation_instructions_are_informational_not_reload_failures() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let mut report = ThemeApplyReport {
        results: ["opencode", "btop", "yazi", "zellij"]
            .into_iter()
            .map(|tool| ToolApplyResult {
                tool_name: tool.into(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: false,
            })
            .collect(),
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    };
    reload_theme_targets(&env, &ToolRegistry::default(), &mut report);
    assert_eq!(report.reload_warnings.len(), 4);
    assert!(report
        .reload_warnings
        .iter()
        .all(|notice| notice.informational));
    for notice in &report.reload_warnings {
        assert_eq!(notice.display_message(true), None);
        assert_eq!(notice.display_message(false), Some(notice.message.as_str()));
    }
    let failure = ReloadWarning {
        informational: false,
        tool_name: "tmux".into(),
        message: "Reload failed; inspect the session before retrying".into(),
    };
    assert_eq!(
        failure.display_message(true),
        Some(failure.message.as_str())
    );
    assert_eq!(
        failure.display_message(false),
        Some(failure.message.as_str())
    );
    assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
}

#[test]
fn theme_checkpoint_picker_includes_the_later_opacity_stage_and_its_absence() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let _config = ConfigManager::with_env(&env).unwrap();
    std::fs::write(env.managed_file("current-opacity"), b"solid\n").unwrap();
    let theme = catppuccin::catppuccin_mocha().unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .including_opacity()
        .apply_to_tools(&theme, &["ls_colors".into()])
        .unwrap();
    report.ensure_no_failures().unwrap();
    let id = report.restore_point_id.as_ref().unwrap();
    let point = crate::config::get_restore_point_with_env(&env, id).unwrap();
    for path in crate::opacity::managed_paths(&env) {
        assert!(point.entries.iter().any(|entry| entry.original_path == path
            && entry.original_state == crate::config::OriginalFileState::Absent));
    }
    apply_opacity(
        &env,
        OpacityPreset::Frosted,
        OpacityApplyOptions {
            persist_state: true,
            reload_terminals: false,
            snapshot_policy: SnapshotPolicy::Skip,
        },
    )
    .unwrap();
    assert_ne!(
        std::fs::read(env.managed_file("current-opacity")).unwrap(),
        b"solid\n"
    );
    assert!(crate::config::execute_restore_with_env(&env, id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        std::fs::read(env.managed_file("current-opacity")).unwrap(),
        b"solid\n"
    );
    assert!(crate::opacity::managed_paths(&env)
        .into_iter()
        .all(|path| !path.exists()));
}

fn managed_tool_dir(env: &SlateEnv, tool: &str) -> std::path::PathBuf {
    env.config_dir().join("managed").join(tool)
}

fn count_restore_points(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn test_report_counts_statuses() {
    let report = ThemeApplyReport {
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
        results: vec![
            ToolApplyResult {
                tool_name: "ghostty".to_string(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: false,
            },
            ToolApplyResult {
                tool_name: "alacritty".to_string(),
                status: ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig),
                requires_new_shell: false,
            },
            ToolApplyResult {
                tool_name: "starship".to_string(),
                status: ToolApplyStatus::Failed(crate::error::SlateError::Internal(
                    "boom".to_string(),
                )),
                requires_new_shell: false,
            },
        ],
    };

    assert_eq!(report.applied_count(), 1);
    assert_eq!(report.skipped_count(), 1);
    assert_eq!(report.failed_count(), 1);
    assert!(report.ghostty_applied());
}

#[test]
fn test_apply_report_skips_internal_ls_colors_adapter() {
    let result = ToolApplyResult {
        tool_name: "ls_colors".to_string(),
        status: ToolApplyStatus::Applied,
        requires_new_shell: true,
    };

    assert!(
        !should_log_apply_result(&result),
        "internal ls_colors layer should not print as a pseudo-tool in apply logs"
    );
}

#[test]
fn test_apply_opacity_without_persisting_state_skips_current_file() {
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());

    apply_opacity(
        &env,
        OpacityPreset::Frosted,
        OpacityApplyOptions {
            persist_state: false,
            reload_terminals: false,
            snapshot_policy: SnapshotPolicy::Skip,
        },
    )
    .unwrap();

    assert!(!env.managed_file("current-opacity").exists());
    assert!(managed_tool_dir(&env, "ghostty")
        .join("opacity.conf")
        .exists());
    assert!(managed_tool_dir(&env, "ghostty").join("blur.conf").exists());
    assert!(managed_tool_dir(&env, "alacritty")
        .join("opacity.toml")
        .exists());
    assert!(managed_tool_dir(&env, "kitty")
        .join("opacity.conf")
        .exists());
}

#[test]
fn test_apply_opacity_with_persisting_state_writes_current_file() {
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());

    apply_opacity(
        &env,
        OpacityPreset::Clear,
        OpacityApplyOptions {
            persist_state: true,
            reload_terminals: false,
            snapshot_policy: SnapshotPolicy::Skip,
        },
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
        "clear"
    );
}

#[test]
fn opacity_failure_does_not_advance_the_saved_preset() {
    use std::os::unix::fs::symlink;
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config
        .set_current_opacity_preset(OpacityPreset::Solid)
        .unwrap();
    let outside = td.path().join("private-original.conf");
    std::fs::write(&outside, "background_opacity 1\n").unwrap();
    let target = config.managed_dir("kitty").join("opacity.conf");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    symlink(&outside, &target).unwrap();
    assert!(apply_opacity(
        &env,
        OpacityPreset::Clear,
        OpacityApplyOptions {
            persist_state: true,
            reload_terminals: false,
            snapshot_policy: SnapshotPolicy::Skip,
        }
    )
    .is_err());
    assert_eq!(
        config.get_current_opacity_preset().unwrap(),
        OpacityPreset::Solid
    );
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "background_opacity 1\n"
    );
}

#[test]
fn opacity_late_failure_retains_checkpoint_and_never_reloads() {
    use std::os::unix::fs::symlink;
    for name in ["managed/kitty/opacity.conf", "current-opacity"] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        config
            .set_current_opacity_preset(OpacityPreset::Solid)
            .unwrap();
        let target = env.managed_file(name);
        let external = td.path().join("private-original");
        std::fs::write(&external, "solid").unwrap();
        let error = apply_opacity_with_effects(
            &env,
            OpacityPreset::Clear,
            OpacityApplyOptions {
                persist_state: true,
                reload_terminals: true,
                snapshot_policy: SnapshotPolicy::Create,
            },
            || {
                // A deterministic late edit after the real checkpoint, before
                // publication. Never race a background editor in this test.
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                if target.exists() {
                    std::fs::remove_file(&target).unwrap();
                }
                symlink(&external, &target).unwrap();
            },
            || panic!("failed opacity must not reload any terminal"),
        )
        .unwrap_err()
        .to_string();
        let points = crate::config::list_restore_points_with_env(&env).unwrap();
        assert_eq!(points.len(), 1);
        let point = &points[0];
        assert_eq!(point.theme_name, "pre-opacity");
        assert!(!point.reapplies_theme());
        assert!(
            error.contains(&format!("slate restore {} --dry-run", point.id)),
            "{error}"
        );
        assert!(
            error.contains(if name == "current-opacity" {
                "saved opacity"
            } else {
                "Kitty opacity"
            }),
            "{error}"
        );
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );
        assert_eq!(std::fs::read_to_string(&external).unwrap(), "solid");
        assert!(env.managed_file("managed/ghostty/opacity.conf").exists());
        std::fs::remove_file(&target).unwrap(); // Repair private link before restore.
        assert!(crate::config::execute_restore_with_env(&env, &point.id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );
        assert!(!env.managed_file("managed/ghostty/opacity.conf").exists());
    }
}

#[test]
fn opacity_reload_uses_captured_session_and_preview_never_saves_state() {
    for (session, requested, expected) in [
        ("isolated", true, false),
        ("remote", true, false),
        ("local", false, false),
        ("local", true, true),
    ] {
        let td = TempDir::new().unwrap();
        let env = if session == "isolated" {
            SlateEnv::with_home(td.path().to_owned())
        } else {
            SlateEnv::from_vars(|key| match key {
                "HOME" => Some(td.path().as_os_str().to_owned()),
                "SSH_CONNECTION" if session == "remote" => Some("captured remote session".into()),
                _ => None,
            })
            .unwrap()
        };
        let config = ConfigManager::with_env(&env).unwrap();
        config
            .set_current_opacity_preset(OpacityPreset::Solid)
            .unwrap();
        let reloaded = std::cell::Cell::new(false);
        apply_opacity_with_effects(
            &env,
            OpacityPreset::Clear,
            OpacityApplyOptions {
                persist_state: false,
                reload_terminals: requested,
                snapshot_policy: SnapshotPolicy::Create,
            },
            || {},
            || reloaded.set(true),
        )
        .unwrap();
        assert_eq!(reloaded.get(), expected, "{session}");
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );
        assert!(crate::config::list_restore_points_with_env(&env)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn opacity_noop_keeps_requested_reload_without_checkpoint_or_writes() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::from_vars(|key| (key == "HOME").then(|| td.path().as_os_str().to_owned()))
        .unwrap();
    let options = OpacityApplyOptions {
        persist_state: true,
        reload_terminals: true,
        snapshot_policy: SnapshotPolicy::Create,
    };
    apply_opacity_with_effects(&env, OpacityPreset::Frosted, options, || {}, || {}).unwrap();
    let reloaded = std::cell::Cell::new(false);
    apply_opacity_with_effects(
        &env,
        OpacityPreset::Frosted,
        options,
        || panic!("no-op must not enter checkpoint/write effects"),
        || reloaded.set(true),
    )
    .unwrap();
    assert!(reloaded.get());
    assert_eq!(
        crate::config::list_restore_points_with_env(&env)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_theme_apply_with_skip_snapshot_keeps_restore_directory_empty() {
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("catppuccin-mocha").unwrap();

    let theme = catppuccin::catppuccin_latte().unwrap();
    let coordinator = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip);

    coordinator.apply(&theme).unwrap();
    coordinator.apply(&theme).unwrap();

    assert_eq!(
        count_restore_points(&env.slate_cache_dir().join("backups")),
        0
    );
}

#[test]
fn test_theme_apply_with_create_snapshot_records_previous_theme() {
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("catppuccin-mocha").unwrap();

    let theme = catppuccin::catppuccin_latte().unwrap();
    ThemeApplyCoordinator::new(&env).apply(&theme).unwrap();

    assert_eq!(
        count_restore_points(&env.slate_cache_dir().join("backups")),
        1
    );
}

#[test]
fn slate_theme_set_writes_nvim_state_file_on_successful_apply() {
    // Contract: after ThemeApplyCoordinator::apply succeeds (at least one
    // adapter returns Applied), the shared coordinator writes the nvim
    // state file at `<home>/.cache/slate/current_theme.lua` containing the
    // applied variant id. The state file is what triggers the nvim loader's
    // vim.uv.fs_event watcher in any running nvim.
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let theme = catppuccin::catppuccin_mocha().unwrap();

    let report = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip)
        .apply_to_tools(&theme, &["ls_colors".into()])
        .unwrap();
    report.ensure_no_failures().unwrap();
    assert!(
        report.applied_count() >= 1,
        "precondition: at least one adapter must apply (ls_colors is always-on)"
    );

    let state = tempdir.path().join(".cache/slate/current_theme.lua");
    assert!(
        state.is_file(),
        "shared coordinator must write nvim state file at {:?}",
        state
    );
    let got = std::fs::read_to_string(&state).unwrap();
    assert!(
        got.contains(&theme.id),
        "state file must contain applied variant id {:?}, got {:?}",
        theme.id,
        got
    );
}

#[test]
fn slate_theme_set_no_state_file_when_no_adapter_applied() {
    // Contract: if applied_count == 0 (e.g., only a non-existent tool was
    // targeted), the coordinator must NOT write an orphan state file. This
    // prevents running nvim instances from hot-reloading to a theme that
    // nothing else applied.
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let theme = catppuccin::catppuccin_mocha().unwrap();

    let coordinator = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip);
    let unreachable_target = vec!["definitely-not-a-real-tool".to_string()];
    let report = coordinator
        .apply_to_tools(&theme, &unreachable_target)
        .unwrap();
    assert_eq!(report.applied_count(), 0);

    let state = tempdir.path().join(".cache/slate/current_theme.lua");
    assert!(
        !state.exists(),
        "no orphan state file must be written when no adapter applied; found {:?}",
        state
    );
}

/// contract: `preview_theme(&env, ...)` must route through
/// `ToolRegistry::apply_theme_to_tools_with_env` so the 3 preview-path
/// adapters (Ghostty, Alacritty, Kitty) honor the injected env. With a
/// tempdir-backed env, managed writes must land inside the tempdir and
/// the host's real `~/.config/slate/managed/*` must NOT be touched. This
/// is the end-to-end proof that the `silent_preview_apply(&env, ...)`
/// signature is no longer a lie.
#[test]
fn preview_theme_routes_injected_env_all_the_way_to_adapters() {
    use std::io::Write;

    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());

    // Pre-create Ghostty integration config inside tempdir so the apply
    // path doesn't short-circuit with MissingIntegrationConfig. This is
    // the "live preview on Ghostty" scenario — picker navigation with a
    // real Ghostty session running.
    let ghostty_integration = tempdir.path().join(".config/ghostty/config.ghostty");
    std::fs::create_dir_all(ghostty_integration.parent().unwrap()).unwrap();
    let mut file = std::fs::File::create(&ghostty_integration).unwrap();
    writeln!(file, "# managed by slate").unwrap();
    drop(file);

    let theme = catppuccin::catppuccin_mocha().unwrap();

    // Route through the actual `preview_theme` entry point — this is
    // what `silent_preview_apply` calls.
    preview_theme(&env, &theme, OpacityPreset::Solid).unwrap();

    // Managed ghostty/theme.conf MUST live inside the tempdir.
    let managed_ghostty_theme = tempdir
        .path()
        .join(".config/slate/managed/ghostty/theme.conf");
    assert!(
        managed_ghostty_theme.exists(),
        "preview_theme must write managed ghostty theme.conf inside tempdir at {:?}",
        managed_ghostty_theme
    );

    // Integration config inside tempdir must reference the tempdir managed path.
    let integration_content = std::fs::read_to_string(&ghostty_integration).unwrap();
    assert!(
        integration_content.contains(&managed_ghostty_theme.display().to_string()),
        "ghostty integration config must include the tempdir-scoped managed theme.conf, got:\n{}",
        integration_content
    );
}

#[test]
fn test_apply_does_not_commit_current_when_no_adapter_applied() {
    // Targeting a non-existent adapter produces an empty result list, i.e. applied_count==0.
    // The guard must keep the previous current_theme untouched so `slate status` does not
    // advertise a theme that nothing actually applied.
    let tempdir = TempDir::new().unwrap();
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("catppuccin-mocha").unwrap();

    let theme = catppuccin::catppuccin_latte().unwrap();
    let coordinator = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip);
    let unreachable_target = vec!["definitely-not-a-real-tool".to_string()];
    let report = coordinator
        .apply_to_tools(&theme, &unreachable_target)
        .unwrap();

    assert_eq!(report.applied_count(), 0);
    assert_eq!(
        config.get_current_theme().unwrap().unwrap_or_default(),
        "catppuccin-mocha",
        "current theme should be preserved when no adapter applied the new theme"
    );
}
