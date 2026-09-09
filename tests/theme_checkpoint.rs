//! Exact ordinary-theme checkpoints; selected adapters do not launch native tools.
use slate_cli::{
    cli::theme_apply::ThemeApplyCoordinator,
    config::{
        execute_restore_with_env, get_restore_point_with_env, ConfigManager, OriginalFileState,
    },
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn theme_checkpoint_restores_generated_bytes_permissions_and_absence() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    write(&env.managed_file("current"), b"nord\n", 0o640);
    let entry = env.xdg_config_home().join("alacritty/alacritty.toml");
    write(
        &entry,
        b"# keep exact user config\n[window]\npadding = {x = 7, y = 9}\n",
        0o640,
    );
    let colors = env.managed_file("managed/alacritty/colors.toml");
    let original = b"# manually tuned old generated colors\n# raw \xff\n";
    write(&colors, original, 0o600);
    let opacity = env.managed_file("managed/alacritty/opacity.toml");
    // An unrelated file must not become part of a selected-tools checkpoint.
    let unrelated = env.zshrc_path();
    write(&unrelated, b"private shell config\n", 0o600);
    let registry = ThemeRegistry::new().unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .apply_to_tools(
            registry.get("catppuccin-mocha").unwrap(),
            &["alacritty".into(), "ls_colors".into()],
        )
        .unwrap();
    report.ensure_no_failures().unwrap();
    let point =
        get_restore_point_with_env(&env, report.restore_point_id.as_ref().unwrap()).unwrap();
    let saved = point
        .entries
        .iter()
        .find(|e| e.original_path == colors)
        .expect("ordinary theme checkpoint must include generated colors");
    assert_eq!(
        fs::read(saved.backup_path.as_ref().unwrap()).unwrap(),
        original
    );
    assert_eq!(saved.unix_mode, Some(0o600));
    assert!(point
        .entries
        .iter()
        .any(|e| e.original_path == opacity && e.original_state == OriginalFileState::Absent));
    assert!(!point.entries.iter().any(|e| e.original_path == unrelated));
    assert!(
        !point.reapplies_theme(),
        "exact bytes must not be overwritten by regeneration"
    );
    assert_eq!(point.theme_name, "pre-theme");
    assert!(!point.is_baseline);
    let output = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("HOME", td.path())
        .env("SLATE_HOME", td.path())
        .env("PATH", td.path().join("no-native-path"))
        .args(["restore", &point.id, "--dry-run", "--json"])
        .timeout(std::time::Duration::from_secs(7))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(plan["may_regenerate_theme_files"], false);
    assert_ne!(fs::read(&colors).unwrap(), original);
    assert!(opacity.is_file());
    let receipt = execute_restore_with_env(&env, &point.id).unwrap();
    assert!(receipt.is_fully_successful());
    assert_eq!(fs::read(&colors).unwrap(), original);
    assert_eq!(
        fs::metadata(&colors).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!opacity.exists());
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    assert!(!env.slate_cache_dir().join("current_theme.lua").exists());
    // Undo is also exact: it restores the state immediately before restore.
    assert!(
        execute_restore_with_env(&env, &receipt.pre_restore_point_id)
            .unwrap()
            .is_fully_successful()
    );
    assert_ne!(fs::read(&colors).unwrap(), original);
    assert!(opacity.exists());
    assert_eq!(
        config.get_current_theme().unwrap().as_deref(),
        Some("catppuccin-mocha")
    );
}

#[test]
fn theme_checkpoint_failure_stops_before_adapter_or_shared_writes() {
    use slate_cli::config::list_restore_points_with_env;
    for kind in ["oversized", "fifo", "dangling-link"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let _config = ConfigManager::with_env(&env).unwrap();
        write(&env.managed_file("current"), b"nord\n", 0o640);
        let old_shell = env.managed_file("managed/shell/env.zsh");
        write(&old_shell, b"original shell bytes\n", 0o640);
        let blocked = env.managed_file("managed/shell/env.bash");
        match kind {
            "oversized" => fs::File::create(&blocked)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            "fifo" => {
                let path = std::ffi::CString::new(blocked.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            _ => {
                std::os::unix::fs::symlink(td.path().join("missing-link-target"), &blocked).unwrap()
            }
        }
        let registry = ThemeRegistry::new().unwrap();
        let result = ThemeApplyCoordinator::new(&env).apply_to_tools(
            registry.get("catppuccin-mocha").unwrap(),
            &["ls_colors".into()],
        );
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains(if kind == "oversized" {
                "checkpoint limit"
            } else if kind == "fifo" {
                "regular file"
            } else {
                "final symlink"
            }),
            "{error}"
        );
        assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
        assert_eq!(fs::read(&old_shell).unwrap(), b"original shell bytes\n");
        assert!(!env.managed_file("managed/starship/plain.toml").exists());
        assert!(!env.slate_cache_dir().join("current_theme.lua").exists());
        assert!(!td.path().join("missing-link-target").exists());
        assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    }
}
