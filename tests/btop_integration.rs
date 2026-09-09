//! Every write and native terminal belongs to a disposable profile.
use slate_cli::{
    adapter::{BtopAdapter, ToolAdapter, ToolApplyStatus},
    cli::theme_apply::ThemeApplyCoordinator,
    config::{
        execute_restore_with_env, get_restore_point_with_env, list_restore_points_with_env,
        OriginalFileState,
    },
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[path = "btop_integration/native.rs"]
mod native;
#[path = "support/tree.rs"]
mod tree_snapshot;

fn seed(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(10));
    command
}

#[test]
fn btop_repeat_apply_preserves_file_identity_permissions_and_personal_settings() {
    use std::os::unix::fs::MetadataExt;
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = BtopAdapter::config_path(&env);
    let asset = BtopAdapter::theme_path(&env);
    seed(
        &config,
        "# personal layout\ncolor_theme=custom\ntheme_background=false\nupdate_ms=1500\n",
    );
    let identity = |path: &Path| {
        let metadata = fs::metadata(path).unwrap();
        (
            metadata.dev(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.mode(),
        )
    };
    let registry = ThemeRegistry::new().unwrap();
    for theme in registry.all() {
        BtopAdapter.apply_theme_with_env(theme, &env).unwrap();
        // Non-default permissions must survive a no-op, not be normalized by
        // an unnecessary atomic replacement. No timestamps are used as sleeps.
        fs::set_permissions(&asset, fs::Permissions::from_mode(0o640)).unwrap();
        let before = tree_snapshot::tree(home.path());
        let identities = (identity(&config), identity(&asset));
        for _ in 0..2 {
            BtopAdapter.apply_theme_with_env(theme, &env).unwrap();
            assert_eq!(
                (identity(&config), identity(&asset)),
                identities,
                "{}",
                theme.id
            );
            assert_eq!(tree_snapshot::tree(home.path()), before, "{}", theme.id);
        }
        let text = fs::read_to_string(&config).unwrap();
        assert!(text.contains("# personal layout\n"));
        assert!(text.contains("theme_background=false\n"));
        assert!(text.contains("update_ms=1500\n"));
    }
}

#[test]
fn btop_coordinated_apply_has_exact_recovery_and_restart_guidance() {
    for existing in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let fake = env.user_local_bin().join("btop");
        seed(&fake, "#!/bin/sh\nexit 99\n");
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        let config = BtopAdapter::config_path(&env);
        let asset = BtopAdapter::theme_path(&env);
        if existing {
            seed(
                &config,
                "# personal\ncolor_theme=custom\ntheme_background=false\n",
            );
        }
        let before = fs::read(&config).ok();
        let registry = ThemeRegistry::new().unwrap();
        let report = ThemeApplyCoordinator::new(&env)
            .apply_to_tools(registry.get("nord").unwrap(), &["btop".into()])
            .unwrap();
        report.ensure_no_failures().unwrap();
        assert!(report
            .results
            .iter()
            .any(|r| r.tool_name == "btop" && matches!(r.status, ToolApplyStatus::Applied)));
        assert!(report
            .reload_warnings
            .iter()
            .any(|w| w.tool_name == "btop" && w.message.contains("reopen btop")));
        let id = report.restore_point_id.unwrap();
        let checkpoint = get_restore_point_with_env(&env, &id).unwrap();
        let entry = checkpoint
            .entries
            .iter()
            .find(|e| e.original_path == config)
            .unwrap();
        assert_eq!(
            entry.original_state,
            if existing {
                OriginalFileState::Present
            } else {
                OriginalFileState::Absent
            }
        );
        assert!(checkpoint
            .entries
            .iter()
            .any(|e| e.original_path == asset && e.original_state == OriginalFileState::Absent));
        assert!(!checkpoint.reapplies_theme());
        assert!(execute_restore_with_env(&env, &id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(fs::read(&config).ok(), before);
        assert!(!asset.exists());
        if existing {
            assert_eq!(
                fs::metadata(&config).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }
}

#[test]
fn btop_clean_preview_execution_and_undo_agree_and_preserve_other_themes() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = BtopAdapter::config_path(&env);
    let asset = BtopAdapter::theme_path(&env);
    let personal = asset.parent().unwrap().join("personal.theme");
    seed(
        &config,
        "# personal\ncolor_theme=custom\ntheme_background=false\n",
    );
    seed(&personal, "# keep me\n");
    BtopAdapter
        .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
        .unwrap();
    let applied = (fs::read(&config).unwrap(), fs::read(&asset).unwrap());
    let before = tree_snapshot::tree(home.path());
    let output = command(home.path())
        .args(["clean", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&output).unwrap();
    for (path, action) in [(&config, "rewrite"), (&asset, "remove")] {
        assert!(preview["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == path.to_str().unwrap() && e["action"] == action));
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
    command(home.path())
        .args(["--quiet", "clean"])
        .assert()
        .success();
    assert!(!asset.exists());
    assert_eq!(fs::read(&personal).unwrap(), b"# keep me\n");
    assert_eq!(
        fs::read(&config).unwrap(),
        b"# personal\ncolor_theme=\"Default\"\ntheme_background=false\n"
    );
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        (fs::read(&config).unwrap(), fs::read(&asset).unwrap()),
        applied
    );
    // A same-named personal file without the ownership header is never removed.
    seed(&asset, "# user replaced this asset\n");
    command(home.path())
        .args(["--quiet", "clean"])
        .assert()
        .success();
    assert_eq!(fs::read(&asset).unwrap(), b"# user replaced this asset\n");
}
