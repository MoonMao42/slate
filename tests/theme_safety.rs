use slate_cli::cli::theme_apply::ThemeApplyCoordinator;
use slate_cli::config::{
    execute_restore_with_env, get_restore_point_with_env, ConfigManager, OriginalFileState,
};
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[test]
fn partial_apply_preserves_global_state_and_exposes_recovery() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    let files = [
        (env.managed_file("current"), "nord"),
        (env.managed_file("current-opacity"), "solid"),
        (env.managed_file("auto.toml"), "dark_theme = 'nord'\n"),
        (
            env.config_dir().join("managed/shell/env.zsh"),
            "# original zsh\n",
        ),
        (
            env.config_dir().join("managed/shell/env.bash"),
            "# original bash\n",
        ),
        (
            env.config_dir().join("managed/shell/env.fish"),
            "# original fish\n",
        ),
        (
            env.slate_cache_dir().join("current_theme.lua"),
            "return 'nord'\n",
        ),
        (
            env.xdg_config_home().join("alacritty/alacritty.toml"),
            "[broken TOML\n",
        ),
    ];
    for (path, content) in &files {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let registry = ThemeRegistry::new().unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .apply_to_tools(
            registry.get("catppuccin-mocha").unwrap(),
            &["alacritty".into(), "ls_colors".into()],
        )
        .unwrap();
    assert_eq!(report.failed_count(), 1);
    let err = report.ensure_no_failures().unwrap_err().to_string();
    let id = report.restore_point_id.as_ref().unwrap();
    assert!(err.contains(&format!("slate restore {id} --dry-run")));
    let point = get_restore_point_with_env(&env, id).unwrap();
    assert_eq!(point.theme_name, "pre-theme");
    assert!(!point.reapplies_theme());
    assert!(point
        .entries
        .iter()
        .any(|entry| entry.original_state == OriginalFileState::Absent));
    for (path, content) in &files {
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            *content,
            "{} changed",
            path.display()
        );
    }
}

#[test]
fn first_apply_captures_a_file_only_operation_checkpoint_including_opencode() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
    let opencode = env.xdg_config_home().join("opencode/tui.jsonc");
    let originals = [
        (
            &alacritty,
            "# user settings\n[window]\npadding = { x = 9, y = 7 }\n",
        ),
        (
            &opencode,
            "// user settings\n{\"theme\": \"custom\", \"scroll_speed\": 7}\n",
        ),
    ];
    for (path, content) in &originals {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    // Installation evidence only: the adapter edits config, never launches this fixture.
    let executable = env.user_local_bin().join("opencode");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let registry = ThemeRegistry::new().unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .apply_to_tools(
            registry.get("catppuccin-mocha").unwrap(),
            &["alacritty".into(), "opencode".into()],
        )
        .unwrap();
    report.ensure_no_failures().unwrap();
    assert_eq!(report.applied_count(), 2);
    let id = report.restore_point_id.as_ref().unwrap();
    let point = get_restore_point_with_env(&env, id).unwrap();
    assert!(!point.is_baseline);
    assert_eq!(point.theme_name, "pre-theme");
    assert!(!point.reapplies_theme());
    for (path, content) in &originals {
        assert_ne!(fs::read_to_string(path).unwrap(), *content);
        let entry = point
            .entries
            .iter()
            .find(|entry| entry.original_path == **path)
            .unwrap();
        assert_eq!(
            fs::read_to_string(entry.backup_path.as_ref().unwrap()).unwrap(),
            *content
        );
    }
    assert!(execute_restore_with_env(&env, id)
        .unwrap()
        .is_fully_successful());
    for (path, content) in &originals {
        assert_eq!(fs::read_to_string(path).unwrap(), *content);
    }
    assert!(!env.managed_file("current").exists());
    assert!(!env.config_dir().join("managed/shell/env.zsh").exists());
}
