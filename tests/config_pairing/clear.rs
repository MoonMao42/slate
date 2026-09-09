use super::*;
use slate_cli::{cli::config::pairing::PairingOptions, config::execute_restore_with_env};

#[test]
fn pairing_clear_cli_previews_then_restores_normal_fallback_without_touching_watcher_state() {
    let td = tempfile::tempdir().unwrap();
    let env = fixture(td.path());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    write(&env.managed_file("current"), "catppuccin-mocha");
    let path = env.managed_file("auto.toml");
    let original =
        "# keep this\ndark_theme='nord'\nlight_theme='gruvbox-light' # light note\nextra=7\n";
    write(&path, original);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    write(
        &env.managed_file("managed/bin/slate-dark-mode-notify"),
        "original helper",
    );
    let before = snapshot::tree(td.path());
    let preview = report(
        td.path(),
        &["config", "pairing", "--clear-light", "--dry-run", "--json"],
    );
    assert_eq!(preview["action"], "preview");
    assert_eq!(preview["changed"], true);
    assert_eq!(preview["before"]["light"]["theme_id"], "gruvbox-light");
    assert_eq!(preview["pairing"]["light"]["status"], "unset");
    assert_eq!(preview["pairing"]["dark"]["theme_id"], "nord");
    assert_eq!(snapshot::tree(td.path()), before);
    let saved = report(td.path(), &["config", "pairing", "--clear-light", "--json"]);
    assert_eq!(saved["action"], "saved");
    let resolved = report(td.path(), &["config", "pairing", "--json"]);
    assert_eq!(resolved["resolution"]["dark"]["source"], "configured");
    assert_eq!(resolved["resolution"]["light"]["source"], "catalog_pair");
    assert_eq!(
        resolved["resolution"]["light"]["theme_id"],
        "catppuccin-latte"
    );
    assert!(config.is_auto_theme_enabled().unwrap());
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "catppuccin-mocha"
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("managed/bin/slate-dark-mode-notify")).unwrap(),
        "original helper"
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let repeated = report(td.path(), &["config", "pairing", "--clear-light", "--json"]);
    assert_eq!(repeated["action"], "unchanged");
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
    let id = saved["restore_point_id"].as_str().unwrap();
    assert!(execute_restore_with_env(&env, id)
        .unwrap()
        .results
        .iter()
        .all(|entry| entry.success));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(!td.path().join("UNEXPECTED").exists());
}

#[test]
fn pairing_clear_cli_supports_mixed_edits_and_absent_noop_but_rejects_conflicts_before_profile_setup(
) {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("absent");
    let preview = report(
        &home,
        &[
            "config",
            "pairing",
            "--clear-dark",
            "--clear-light",
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(preview["changed"], false);
    assert!(!home.exists());
    for args in [
        vec!["config", "pairing", "--clear-dark", "--dark", "nord"],
        vec![
            "config",
            "pairing",
            "--clear-light",
            "--light",
            "catppuccin-latte",
        ],
    ] {
        command(&home).args(args).assert().failure();
        assert!(!home.exists());
    }
    let invalid = PairingOptions {
        dark: Some("nord".into()),
        clear_dark: true,
        ..Default::default()
    };
    assert!(invalid.validate().is_err());
    let unchanged = report(
        &home,
        &[
            "config",
            "pairing",
            "--clear-dark",
            "--clear-light",
            "--json",
        ],
    );
    assert_eq!(unchanged["action"], "unchanged");
    let env = SlateEnv::with_home(home.clone());
    assert!(!env.managed_file("auto.toml").exists());
    assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    write(
        &env.managed_file("auto.toml"),
        "dark_theme='PRIVATE_CONTENT'\nlight_theme='gruvbox-light'\n",
    );
    let mixed = report(
        &home,
        &[
            "config",
            "pairing",
            "--clear-dark",
            "--light",
            "catppuccin-latte",
            "--json",
        ],
    );
    assert_eq!(mixed["before"]["dark"]["status"], "error");
    assert_eq!(mixed["pairing"]["dark"]["status"], "unset");
    assert_eq!(mixed["pairing"]["light"]["theme_id"], "catppuccin-latte");
    assert!(!mixed.to_string().contains("PRIVATE_CONTENT"));
    let both = report(
        &home,
        &[
            "config",
            "pairing",
            "--clear-dark",
            "--clear-light",
            "--json",
        ],
    );
    assert_eq!(both["pairing"]["dark"]["status"], "unset");
    assert_eq!(both["pairing"]["light"]["status"], "unset");
    assert!(env.managed_file("auto.toml").is_file());
    assert!(fs::read(env.managed_file("auto.toml")).unwrap().is_empty());
    assert!(!env.managed_file("config.toml").exists());
    assert!(!env.managed_file("current").exists());
    let resolved = report(&home, &["config", "pairing", "--json"]);
    assert_eq!(resolved["resolution"]["dark"]["source"], "brand_default");
    assert_eq!(resolved["resolution"]["light"]["source"], "brand_default");
}

#[test]
fn pairing_clear_cli_refuses_unsafe_documents_invalid_types_and_writer_or_backup_blockers() {
    for kind in ["symlink", "wrong-type", "malformed", "backup", "writer"] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        let path = env.managed_file("auto.toml");
        write(&path, "dark_theme='nord'\n");
        let guard = if kind == "writer" {
            Some(ConfigWriteGuard::acquire(&env).unwrap())
        } else {
            None
        };
        match kind {
            "symlink" => {
                fs::remove_file(&path).unwrap();
                write(&td.path().join("target"), "PRIVATE_CONTENT");
                symlink(td.path().join("target"), &path).unwrap();
            }
            "wrong-type" => write(&path, "dark_theme=123\n"),
            "malformed" => write(&path, "PRIVATE_CONTENT = ["),
            "backup" => write(&env.slate_cache_dir().join("backups"), "blocked"),
            "writer" => {}
            _ => unreachable!(),
        }
        let before = snapshot::tree(env.config_dir());
        let output = command(td.path())
            .args(["config", "pairing", "--clear-dark", "--json"])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONTENT"));
        assert_eq!(snapshot::tree(env.config_dir()), before);
        assert!(!td.path().join("UNEXPECTED").exists());
        drop(guard);
    }
}
