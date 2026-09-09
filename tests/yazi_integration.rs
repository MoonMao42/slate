//! Selection, snapshots and cleanup use only disposable profiles.
use slate_cli::{
    adapter::{ToolAdapter, YaziAdapter},
    config::{
        begin_restore_point_baseline_with_env, execute_restore_with_env,
        get_restore_point_with_env, list_restore_points_with_env,
    },
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[path = "yazi_integration/native.rs"]
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
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(&env.managed_file("current"), "nord\n");
    let binary = env.user_local_bin().join("yazi");
    seed(
        &binary,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    (home, env)
}

#[test]
fn yazi_selected_sync_reviews_exact_files_and_restores_personal_configuration() {
    let (_home, env) = fixture();
    let config = YaziAdapter::config_path(&env);
    let personal = "# keep\n[flavor]\ndark='mine'\n[mgr]\ncwd={fg='#123456'}\n";
    seed(&config, personal);
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();
    for name in ["yazi.toml", "keymap.toml", "init.lua", "package.toml"] {
        seed(&env.yazi_config_home().join(name), "PRIVATE_CONTENT");
    }
    let before = tree_snapshot::tree(env.home());
    let review = command(&env, true)
        .args(["tools", "sync", "yazi", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let review: serde_json::Value = serde_json::from_slice(&review).unwrap();
    let paths = review["configuration_paths"].as_array().unwrap();
    assert_eq!(paths.len(), 3);
    for path in YaziAdapter::paths(&env) {
        assert!(paths.iter().any(|p| p == path.to_str().unwrap()));
    }
    command(&env, true)
        .args(["tools", "sync", "yazi"])
        .assert()
        .failure();
    assert_eq!(tree_snapshot::tree(env.home()), before);
    let output = command(&env, true)
        .args(["tools", "sync", "yazi", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&output).contains("reopen Yazi"));
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    let point = get_restore_point_with_env(&env, &point.id).unwrap();
    assert_eq!(point.entries.len(), 3);
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    for name in ["yazi.toml", "keymap.toml", "init.lua", "package.toml"] {
        assert_eq!(
            fs::read(env.yazi_config_home().join(name)).unwrap(),
            b"PRIVATE_CONTENT"
        );
    }
    assert!(!env.home().join("UNEXPECTED_PROCESS").exists());
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(&config).unwrap(), personal.as_bytes());
    assert_eq!(
        fs::metadata(&config).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(!YaziAdapter::flavor_path(&env).exists());
    assert!(!YaziAdapter::syntax_path(&env).exists());
}

#[test]
fn yazi_clean_preview_and_undo_preserve_unowned_files() {
    let (_home, env) = fixture();
    YaziAdapter
        .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
        .unwrap();
    let other = env
        .yazi_config_home()
        .join("flavors/personal.yazi/flavor.toml");
    seed(&other, "PRIVATE_CONTENT");
    let applied = YaziAdapter::paths(&env).map(|path| fs::read(path).unwrap());
    let before = tree_snapshot::tree(env.home());
    let preview = command(&env, true)
        .args(["clean", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
    for (path, action) in [
        (YaziAdapter::config_path(&env), "rewrite"),
        (YaziAdapter::flavor_path(&env), "remove"),
        (YaziAdapter::syntax_path(&env), "remove"),
    ] {
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
    assert!(!YaziAdapter::flavor_path(&env).exists() && !YaziAdapter::syntax_path(&env).exists());
    assert_eq!(fs::read(&other).unwrap(), b"PRIVATE_CONTENT");
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        YaziAdapter::paths(&env).map(|path| fs::read(path).unwrap()),
        applied
    );
    seed(&YaziAdapter::flavor_path(&env), "PRIVATE_CONTENT");
    seed(&YaziAdapter::syntax_path(&env), "PRIVATE_CONTENT");
    command(&env, true)
        .args(["--quiet", "clean"])
        .assert()
        .success();
    assert_eq!(
        fs::read(YaziAdapter::flavor_path(&env)).unwrap(),
        b"PRIVATE_CONTENT"
    );
    assert_eq!(
        fs::read(YaziAdapter::syntax_path(&env)).unwrap(),
        b"PRIVATE_CONTENT"
    );
}

#[test]
fn yazi_custom_profile_and_isolation_share_preview_baseline_and_write_paths() {
    let (home, env) = fixture();
    let alternate = home.path().join("alternate-yazi");
    for isolated in [true, false] {
        let injected = SlateEnv::from_vars(|name| match name {
            "HOME" => Some(home.path().into()),
            "SLATE_HOME" if isolated => Some(home.path().into()),
            "YAZI_CONFIG_HOME" => Some(alternate.clone().into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            injected.yazi_config_home(),
            if isolated {
                env.yazi_config_home()
            } else {
                &alternate
            }
        );
        let baseline = begin_restore_point_baseline_with_env(&injected).unwrap();
        for path in YaziAdapter::paths(&injected) {
            assert!(baseline
                .entries
                .iter()
                .any(|entry| entry.original_path == path));
        }
        let before = tree_snapshot::tree(env.home());
        let output = command(&env, isolated)
            .env("YAZI_CONFIG_HOME", &alternate)
            .args(["tools", "sync", "yazi", "--dry-run", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let output: serde_json::Value = serde_json::from_slice(&output).unwrap();
        for path in YaziAdapter::paths(&injected) {
            assert!(output["configuration_paths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == path.to_str().unwrap()));
        }
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
    let before = tree_snapshot::tree(env.home());
    command(&env, false)
        .env("YAZI_CONFIG_HOME", "relative")
        .args(["tools", "sync", "yazi", "--yes"])
        .assert()
        .failure();
    assert_eq!(tree_snapshot::tree(env.home()), before);
}
