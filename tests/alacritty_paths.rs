//! Real path selection, application, diagnostics and file-only recovery.
//! All profiles live under temporary roots; no terminal processes are launched.
use slate_cli::cli::theme_apply::ThemeApplyCoordinator;
use slate_cli::config::{
    begin_restore_point_baseline_with_env, execute_restore_with_env, get_restore_point_with_env,
    list_restore_points_with_env, ConfigManager, OriginalFileState,
};
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn candidates(env: &SlateEnv) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in [
        env.xdg_config_home().join("alacritty/alacritty.toml"),
        env.xdg_config_home().join("alacritty.toml"),
        env.home().join(".config/alacritty/alacritty.toml"),
        env.home().join(".alacritty.toml"),
    ] {
        if !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

fn custom_env(home: &Path, xdg: &Path) -> SlateEnv {
    SlateEnv::from_vars(|key| match key {
        "HOME" => Some(home.as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(xdg.as_os_str().to_owned()),
        _ => None,
    })
    .unwrap()
}

fn command(env: &SlateEnv) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("XDG_CONFIG_HOME", env.xdg_config_home())
        .env("XDG_CACHE_HOME", env.cache_dir())
        .env("PATH", env.home().join("bin"))
        .timeout(Duration::from_secs(5));
    if env.session().is_isolated() {
        command.env("SLATE_HOME", env.home());
    }
    command
}

#[test]
fn alacritty_native_user_paths_share_apply_doctor_and_baseline_recovery() {
    let themes = ThemeRegistry::new().unwrap();
    for custom in [false, true] {
        for selected in 0..if custom { 4 } else { 3 } {
            let td = tempfile::tempdir().unwrap();
            let home = td.path().join("profile");
            fs::create_dir_all(&home).unwrap();
            let env = if custom {
                custom_env(&home, &td.path().join("custom-xdg"))
            } else {
                SlateEnv::with_home(home)
            };
            let paths = candidates(&env);
            for (index, path) in paths.iter().enumerate().skip(selected) {
                write(path, format!("# PRIVATE_CONTENT candidate {index}\n[general]\nimport = ['user-{index}.toml']\n"));
            }
            let before: Vec<_> = paths.iter().map(|path| fs::read(path).ok()).collect();
            ConfigManager::with_env(&env)
                .unwrap()
                .set_current_font("Fixed Mono")
                .unwrap();
            let report = ThemeApplyCoordinator::new(&env)
                .apply_to_tools(themes.get("nord").unwrap(), &["alacritty".into()])
                .unwrap();
            report.ensure_no_failures().unwrap();
            assert_eq!(report.applied_count(), 1);
            let id = report.restore_point_id.unwrap();
            let point = get_restore_point_with_env(&env, &id).unwrap();
            assert!(!point.is_baseline);
            for (index, path) in paths.iter().enumerate() {
                if index != selected {
                    assert!(!point
                        .entries
                        .iter()
                        .any(|entry| entry.original_path == *path));
                    assert_eq!(fs::read(path).ok(), before[index]);
                    continue;
                }
                let entry = point
                    .entries
                    .iter()
                    .find(|entry| entry.original_path == *path)
                    .expect("the selected write target must have a recovery entry");
                match &before[index] {
                    Some(bytes) => {
                        assert_eq!(entry.original_state, OriginalFileState::Present);
                        assert_eq!(
                            fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
                            *bytes
                        );
                    }
                    None => assert_eq!(entry.original_state, OriginalFileState::Absent),
                }
                if index != selected {
                    assert_eq!(fs::read(path).ok(), before[index]);
                }
            }
            let main = fs::read_to_string(&paths[selected]).unwrap();
            assert!(main.contains(
                env.managed_file("managed/alacritty/colors.toml")
                    .to_str()
                    .unwrap()
            ));
            assert!(main.contains(
                env.managed_file("managed/alacritty/opacity.toml")
                    .to_str()
                    .unwrap()
            ));
            let before_doctor = tree_snapshot::tree(td.path());
            let output = command(&env)
                .args(["doctor", "alacritty", "--json"])
                .assert()
                .success()
                .get_output()
                .clone();
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(!text.contains("PRIVATE_CONTENT"));
            let report: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(
                    |check| check["message"] == "Slate's loader reference is present"
                        && check["path"].as_str() == paths[selected].to_str()
                ));
            assert_eq!(tree_snapshot::tree(td.path()), before_doctor);
            assert!(execute_restore_with_env(&env, &id)
                .unwrap()
                .is_fully_successful());
            for (index, path) in paths.iter().enumerate() {
                assert_eq!(fs::read(path).ok(), before[index]);
                if before[index].is_some() {
                    assert_eq!(
                        fs::metadata(path).unwrap().permissions().mode() & 0o777,
                        0o640
                    );
                }
            }
        }
    }
}

#[test]
fn alacritty_directory_aliases_produce_one_restorable_target() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile");
    let real = home.join(".config");
    let alias = home.join("xdg-alias");
    fs::create_dir_all(&real).unwrap();
    symlink(&real, &alias).unwrap();
    let env = custom_env(&home, &alias);
    let path = real.join("alacritty/alacritty.toml");
    write(&path, "# user alias\n");
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    assert_eq!(
        point
            .entries
            .iter()
            .filter(|entry| entry.display_tool == "Alacritty")
            .count(),
        3
    );
    let aliased_path = alias.join("alacritty/alacritty.toml");
    assert!(point
        .entries
        .iter()
        .any(|entry| entry.original_path == aliased_path));
    assert!(!point
        .entries
        .iter()
        .any(|entry| entry.original_path == path));
    write(&path, "# changed\n");
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read_to_string(path).unwrap(), "# user alias\n");
    assert!(fs::symlink_metadata(alias).unwrap().is_symlink());
}

#[test]
fn alacritty_clean_previews_and_restores_all_user_candidates() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let managed = env.managed_file("managed/alacritty/colors.toml");
    write(&managed, "# PRIVATE_CONTENT owned colors\n");
    let paths = candidates(&env);
    let original = format!(
        "# PRIVATE_CONTENT user\n[general]\nimport = ['user.toml', '{}'] # retain note\n",
        managed.display()
    );
    for path in &paths {
        write(path, &original);
    }
    let before = tree_snapshot::tree(td.path());
    let output = command(&env)
        .args(["clean", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tree_snapshot::tree(td.path()), before);
    for path in &paths {
        assert!(plan["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |change| change["path"].as_str() == path.to_str() && change["action"] == "rewrite"
            ));
    }
    command(&env).args(["--quiet", "clean"]).assert().success();
    for path in &paths {
        let text = fs::read_to_string(path).unwrap();
        let parsed: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(parsed["general"]["import"].as_array().unwrap().len(), 1);
        assert!(text.contains("# retain note"));
    }
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    for path in &paths {
        assert_eq!(fs::read_to_string(path).unwrap(), original);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert_eq!(
        fs::read_to_string(managed).unwrap(),
        "# PRIVATE_CONTENT owned colors\n"
    );
}

#[test]
fn alacritty_diagnostics_surface_a_blocked_preferred_path_without_falling_back() {
    for blocker in ["link", "fifo", "directory"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let preferred = env.xdg_config_home().join("alacritty/alacritty.toml");
        fs::create_dir_all(preferred.parent().unwrap()).unwrap();
        match blocker {
            "link" => symlink(td.path().join("absent-target"), &preferred).unwrap(),
            "fifo" => {
                let path =
                    std::ffi::CString::new(preferred.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&preferred).unwrap(),
            _ => unreachable!(),
        }
        write(
            &env.home().join(".alacritty.toml"),
            "# valid lower config\n",
        );
        let before = tree_snapshot::tree(td.path());
        let output = command(&env)
            .args(["doctor", "alacritty", "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |check| check["status"] == "error" && check["path"].as_str() == preferred.to_str()
            ));
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}
