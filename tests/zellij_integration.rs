//! End-to-end selection, cleanup and recovery in disposable profiles only.
use slate_cli::{
    adapter::{ToolAdapter, ZellijAdapter},
    config::{
        begin_restore_point_baseline_with_env, execute_restore_with_env,
        get_restore_point_with_env, list_restore_points_with_env,
    },
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[path = "zellij_integration/native.rs"]
mod native;
#[path = "support/tree.rs"]
mod tree_snapshot;

fn seed(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn command(env: &SlateEnv, isolated: bool) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("HOME", env.home())
        .env("PATH", env.user_local_bin())
        .env("NO_COLOR", "1")
        .current_dir(env.home())
        .timeout(Duration::from_secs(8));
    if isolated {
        cmd.env("SLATE_HOME", env.home());
    }
    cmd
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env.managed_file("current"), "nord\n");
    let binary = env.user_local_bin().join("zellij");
    seed(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    (home, env)
}

#[test]
fn zellij_selected_sync_reviews_two_files_and_restores_bytes_mode_and_absence() {
    let (_home, env) = fixture();
    let [config, asset] = ZellijAdapter::paths(&env).unwrap();
    let personal = "// keep\ntheme r#\"personal\"#\nmouse_mode false\nkeybinds { normal { bind \"Ctrl a\" { SwitchToMode \"tmux\"; }; }; }\n";
    seed(&config, personal);
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();
    let layout = config.parent().unwrap().join("layouts/personal.kdl");
    seed(&layout, "PRIVATE_LAYOUT");
    let before = tree_snapshot::tree(env.home());
    let output = command(&env, true)
        .args(["tools", "sync", "zellij", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let review: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let paths = review["configuration_paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);
    for path in [&config, &asset] {
        assert!(paths.iter().any(|p| p == path.to_str().unwrap()));
    }
    command(&env, true)
        .args(["tools", "sync", "zellij"])
        .assert()
        .failure();
    assert_eq!(tree_snapshot::tree(env.home()), before);
    let output = command(&env, true)
        .args(["tools", "sync", "zellij", "--yes"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&output).contains("no session was queried or commanded"));
    let saved = fs::read_to_string(&config).unwrap();
    for key in ["theme", "theme_dark", "theme_light"] {
        assert!(saved.contains(&format!("{key} \"slate-sync\"")));
    }
    assert!(saved.contains("bind \"Ctrl a\""));
    assert_eq!(fs::read(layout).unwrap(), b"PRIVATE_LAYOUT");
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    assert!(!env.home().join("UNEXPECTED_PROCESS").exists());
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    let point = get_restore_point_with_env(&env, &point.id).unwrap();
    assert_eq!(point.entries.len(), 2);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(&config).unwrap(), personal.as_bytes());
    assert_eq!(
        fs::metadata(config).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(!asset.exists());
}

#[test]
fn zellij_clean_preview_and_undo_preserve_personal_choices_and_unowned_assets() {
    let (_home, env) = fixture();
    ZellijAdapter
        .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
        .unwrap();
    let [config, asset] = ZellijAdapter::paths(&env).unwrap();
    // A later personal choice must not be replaced by clean.
    let selected = fs::read_to_string(&config)
        .unwrap()
        .replace("theme_light \"slate-sync\"", "theme_light \"personal\"");
    seed(&config, selected);
    let personal = asset.parent().unwrap().join("personal.kdl");
    seed(&personal, "// personal\nthemes { personal {}; }\n");
    let applied = [config.clone(), asset.clone()].map(|p| fs::read(p).unwrap());
    let before = tree_snapshot::tree(env.home());
    let output = command(&env, true)
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
            .any(|item| item["path"] == path.to_str().unwrap() && item["action"] == action));
    }
    assert_eq!(tree_snapshot::tree(env.home()), before);
    command(&env, true)
        .args(["--quiet", "clean"])
        .assert()
        .success();
    let cleaned = fs::read_to_string(&config).unwrap();
    assert!(cleaned.contains("theme \"default\""));
    assert!(cleaned.contains("theme_dark \"default\""));
    assert!(cleaned.contains("theme_light \"personal\""));
    assert!(!asset.exists());
    assert_eq!(
        fs::read(&personal).unwrap(),
        b"// personal\nthemes { personal {}; }\n"
    );
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        [config, asset.clone()].map(|p| fs::read(p).unwrap()),
        applied
    );
    seed(&asset, "// PRIVATE\nthemes {}\n");
    command(&env, true)
        .args(["--quiet", "clean"])
        .assert()
        .success();
    assert_eq!(fs::read(asset).unwrap(), b"// PRIVATE\nthemes {}\n");
}

#[test]
fn zellij_custom_directory_file_and_theme_paths_match_baseline_and_selected_preview() {
    let (home, env) = fixture();
    let directory = home.path().join("alternate-zellij");
    let config = home.path().join("elsewhere/config.kdl");
    let themes = home.path().join("custom-themes");
    seed(
        &config,
        format!("// keep\ntheme_dir {:?}\n", themes.to_str().unwrap()),
    );
    for isolated in [true, false] {
        let injected = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.path().into()),
            "SLATE_HOME" if isolated => Some(home.path().into()),
            "ZELLIJ_CONFIG_DIR" => Some(directory.clone().into()),
            "ZELLIJ_CONFIG_FILE" => Some(config.clone().into()),
            _ => None,
        })
        .unwrap();
        let paths = ZellijAdapter::paths(&injected).unwrap();
        if isolated {
            assert_eq!(paths, ZellijAdapter::paths(&env).unwrap());
        } else {
            assert_eq!(paths, [config.clone(), themes.join("slate-sync.kdl")]);
        }
        let baseline = begin_restore_point_baseline_with_env(&injected).unwrap();
        for path in &paths {
            assert!(baseline
                .entries
                .iter()
                .any(|entry| &entry.original_path == path));
        }
        let before = tree_snapshot::tree(env.home());
        let output = command(&env, isolated)
            .env("ZELLIJ_CONFIG_DIR", &directory)
            .env("ZELLIJ_CONFIG_FILE", &config)
            .args(["tools", "sync", "zellij", "--dry-run", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let review: serde_json::Value = serde_json::from_slice(&output).unwrap();
        for path in paths {
            assert!(review["configuration_paths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == path.to_str().unwrap()));
        }
        assert_eq!(tree_snapshot::tree(env.home()), before);
        if !isolated {
            let original = fs::read(&config).unwrap();
            command(&env, false)
                .env("ZELLIJ_CONFIG_DIR", &directory)
                .env("ZELLIJ_CONFIG_FILE", &config)
                .args(["tools", "sync", "zellij", "--yes"])
                .assert()
                .success();
            assert!(themes.join("slate-sync.kdl").exists());
            assert!(!directory.join("themes/slate-sync.kdl").exists());
            assert!(!env.home().join(".config/zellij/config.kdl").exists());
            let point = list_restore_points_with_env(&injected)
                .unwrap()
                .into_iter()
                .find(|point| !point.is_baseline)
                .unwrap();
            assert!(execute_restore_with_env(&injected, &point.id)
                .unwrap()
                .is_fully_successful());
            assert_eq!(fs::read(&config).unwrap(), original);
            assert!(!themes.join("slate-sync.kdl").exists());
        }
    }
    // Without theme_dir, --config's parent is not the theme directory.
    seed(&config, "theme \"personal\"\n");
    let injected = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(home.path().into()),
        "ZELLIJ_CONFIG_DIR" => Some(directory.clone().into()),
        "ZELLIJ_CONFIG_FILE" => Some(config.clone().into()),
        _ => None,
    })
    .unwrap();
    assert_eq!(
        ZellijAdapter::paths(&injected).unwrap(),
        [config, directory.join("themes/slate-sync.kdl")]
    );
    let before = tree_snapshot::tree(env.home());
    command(&env, false)
        .env("ZELLIJ_CONFIG_DIR", "relative")
        .args(["tools", "sync", "zellij", "--yes"])
        .assert()
        .failure();
    assert_eq!(tree_snapshot::tree(env.home()), before);
}
