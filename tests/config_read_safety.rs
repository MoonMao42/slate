//! Shared configuration IO, exercised only in private profiles.
use slate_cli::config::{
    begin_restore_point_baseline_with_env, list_restore_points_with_env, ConfigManager,
};
use slate_cli::env::SlateEnv;
use std::fs;
use std::os::unix::fs::{symlink, FileTypeExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn command(home: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(3));
    cmd
}

fn fifo(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(std::process::Command::new("/usr/bin/mkfifo")
        .arg(path)
        .status()
        .unwrap()
        .success());
}

#[test]
fn preference_fifo_fails_without_hanging_or_initializing_sound() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    let path = config.base_path().join("config.toml");
    fifo(&path);
    command(home.path())
        .args(["config", "set", "sound", "off"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("regular file"));
    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
    assert!(!env.slate_cache_dir().join("sounds").exists());
}

#[test]
fn malformed_preferences_report_errors_without_panicking_or_disclosing_contents() {
    for content in [
        "preferences = false\n",
        "[preferences]\nsound = PRIVATE_SECRET\n",
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        let path = config.base_path().join("config.toml");
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let output = command(home.path())
            .args(["config", "set", "sound", "off"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(!message.contains("panicked"), "{message}");
        assert!(!message.contains("PRIVATE_SECRET"), "{message}");
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(!env.slate_cache_dir().join("sounds").exists());
    }
}

#[test]
fn ordinary_theme_snapshots_and_tracking_refuse_fifos_before_application() {
    for filename in [
        "config.toml",
        "auto.toml",
        "current",
        "current-font",
        "managed/shell/env.zsh",
    ] {
        for existing_theme in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            if existing_theme && filename != "current" {
                config.set_current_theme("nord").unwrap();
            }
            let path = env.managed_file(filename);
            fifo(&path);
            let before = tree_snapshot::tree(env.config_dir());
            command(home.path())
                .args(["theme", "nord", "--quiet"])
                .assert()
                .code(1)
                .stderr(predicates::str::contains("regular file"));
            assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
            assert_eq!(tree_snapshot::tree(env.config_dir()), before, "{filename}");
            assert!(list_restore_points_with_env(&env).unwrap().is_empty());
            assert!(!env.slate_cache_dir().join("sounds").exists());
        }
    }
}

#[test]
fn setters_preserve_comments_inline_tables_other_preferences_and_partial_pairs() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    let path = env.managed_file("config.toml");
    for text in [
        "# settings\n[preferences]\nsound = true # personal choice\nother = 'keep'\n[tools]\nstarship = false\n",
        "# settings\npreferences = { sound = true, other = 'keep' } # inline\n[tools]\nstarship = false\n",
    ] {
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        config.set_sound_enabled(false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), text.replacen("sound = true", "sound = false", 1));
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
        assert!(!config.is_sound_enabled().unwrap());
        assert!(!config.is_starship_enabled().unwrap());
    }
    let auto = env.managed_file("auto.toml");
    let text = "# pairing\ndark_theme = \"nord\" # night\nlight_theme = 'github-light' # day\ncustom = { keep = true }\n";
    fs::write(&auto, text).unwrap();
    config.write_auto_config(Some("dracula"), None).unwrap();
    let expected = text.replace("\"nord\"", "\"dracula\"");
    assert_eq!(fs::read_to_string(&auto).unwrap(), expected);
    config.write_auto_config(None, None).unwrap();
    assert_eq!(fs::read_to_string(&auto).unwrap(), expected);
    assert_eq!(
        config
            .read_auto_config()
            .unwrap()
            .unwrap()
            .light_theme
            .as_deref(),
        Some("github-light")
    );
}

#[test]
fn settings_enforce_byte_limits_utf8_and_missing_defaults_without_overwriting() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    assert!(config.is_sound_enabled().unwrap());
    assert!(!config.is_auto_theme_enabled().unwrap());
    assert!(config.get_current_theme().unwrap().is_none());
    assert!(config.read_auto_config().unwrap().is_none());
    for filename in [
        "config.toml",
        "auto.toml",
        "current",
        "current-font",
        "current-opacity",
    ] {
        let path = env.managed_file(filename);
        let limit = if filename.ends_with(".toml") {
            256 * 1024
        } else {
            4 * 1024
        };
        for bytes in [vec![b'x'; limit + 1], vec![0xff, 0xfe, 0x80]] {
            fs::write(&path, &bytes).unwrap();
            let result = match filename {
                "config.toml" => config.is_sound_enabled().map(|_| ()),
                "auto.toml" => config.read_auto_config().map(|_| ()),
                "current" => config.get_current_theme().map(|_| ()),
                "current-font" => config.get_current_font().map(|_| ()),
                _ => config.get_current_opacity().map(|_| ()),
            };
            assert!(result.is_err(), "{filename}");
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_file(&path).unwrap();
    }
    let path = env.managed_file("config.toml");
    let mut exact = "#".repeat(256 * 1024 - 1);
    exact.push('\n');
    fs::write(&path, &exact).unwrap();
    assert!(config.is_sound_enabled().unwrap());
    assert!(
        config.set_sound_enabled(false).is_err(),
        "new document would exceed read limit"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), exact);
    config.set_current_theme(&"a".repeat(4096)).unwrap();
    assert_eq!(config.get_current_theme().unwrap().unwrap().len(), 4096);
    assert!(config.set_current_theme(&"a".repeat(4097)).is_err());
    assert_eq!(config.get_current_theme().unwrap().unwrap().len(), 4096);
}

#[test]
fn ordinary_reads_keep_linked_dotfiles_but_reject_broken_links_and_oversized_snapshots() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    let target = home.path().join("settings");
    let path = env.managed_file("config.toml");
    let preferences = "[preferences]\nsound = false\n[auto_theme]\nenabled = true\n";
    fs::write(&target, preferences).unwrap();
    symlink(&target, &path).unwrap();
    assert!(!config.is_sound_enabled().unwrap());
    assert!(config.is_auto_theme_enabled().unwrap());
    assert!(config.set_sound_enabled(true).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), preferences);
    let source = home.path().join("dotfile");
    let bytes = [b'#', 0xff, 0, b'\n'];
    fs::write(&source, bytes).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&source, env.zshrc_path()).unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    let entry = point
        .entries
        .iter()
        .find(|entry| entry.tool_key == "zshrc")
        .unwrap();
    assert_eq!(entry.unix_mode, Some(0o640));
    assert_eq!(
        fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
        bytes
    );
    let simple_backup = config.backup_file(&env.zshrc_path()).unwrap();
    assert_eq!(fs::read(&simple_backup).unwrap(), bytes);
    assert_eq!(
        fs::metadata(simple_backup).unwrap().permissions().mode() & 0o777,
        0o600
    );

    fs::remove_file(&target).unwrap();
    assert!(config.is_sound_enabled().is_err());
    assert!(config.is_auto_theme_enabled().is_err());
    assert!(config.set_sound_enabled(true).is_err());
    assert!(begin_restore_point_baseline_with_env(&env).is_err());
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
    fs::remove_file(&path).unwrap();
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    assert!(begin_restore_point_baseline_with_env(&env)
        .unwrap_err()
        .to_string()
        .contains("checkpoint limit"));
    assert!(config.backup_file(&env.zshrc_path()).is_err());
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
}
