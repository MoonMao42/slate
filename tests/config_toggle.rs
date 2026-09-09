//! Real Fastfetch CLI writes in disposable profiles; watcher lifecycle is covered
//! by injected stage tests, not by starting or stopping a host watcher here.
use slate_cli::{
    config::{execute_restore_with_env, list_restore_points_with_env, ConfigManager},
    env::SlateEnv,
};
use std::{
    ffi::CString,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    time::Duration,
};

#[path = "support/tree.rs"]
mod snapshot;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .arg("--quiet")
        .timeout(Duration::from_secs(10));
    command
}

fn write(path: &Path, bytes: impl AsRef<[u8]>, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn fixture(home: &Path) -> SlateEnv {
    let env = SlateEnv::with_home(home.into());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config.set_current_font("Private Mono").unwrap();
    for name in [
        "osascript",
        "starship",
        "fastfetch",
        "pgrep",
        "nvim",
        "fc-list",
        "gsettings",
        "dconf",
    ] {
        write(
            &home.join("bin").join(name),
            "#!/bin/sh\nprintf launch > \"$HOME/unexpected-launch\"\nexit 93\n",
            0o700,
        );
    }
    env
}

#[test]
fn config_toggle_cli_roundtrips_files_modes_and_absence_and_skips_identical_repeat() {
    for enabled in [true, false] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        let marker = env.managed_file("autorun-fastfetch");
        if !enabled {
            write(&marker, b"old bounded marker", 0o640);
        }
        write(
            &env.managed_file("managed/shell/env.bash"),
            b"original \xff",
            0o644,
        );
        write(&env.managed_file("user/keep"), b"unrelated", 0o600);
        let value = if enabled { "enable" } else { "disable" };
        let output = command(td.path())
            .args(["config", "set", "fastfetch", value])
            .assert()
            .success()
            .get_output()
            .clone();
        let config = ConfigManager::with_env(&env).unwrap();
        assert_eq!(config.has_fastfetch_autorun().unwrap(), enabled);
        for extension in ["bash", "zsh", "fish"] {
            let bytes =
                fs::read(env.managed_file(&format!("managed/shell/env.{extension}"))).unwrap();
            assert!(bytes.len() > 100);
            assert!(!bytes.contains(&0xff));
            assert_eq!(
                String::from_utf8_lossy(&bytes).contains("\n  fastfetch\n"),
                enabled
            );
        }
        let report = command(td.path())
            .args(["config", "get", "fastfetch", "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
        assert_eq!(report["settings"][0]["value"], enabled);
        let points = list_restore_points_with_env(&env).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].theme_name, "pre-config");
        assert_eq!(points[0].entries.len(), 5);
        assert!(!points[0].reapplies_theme());
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains(&format!("slate restore {} --dry-run", points[0].id)));
        let before_repeat = snapshot::tree(td.path());
        command(td.path())
            .args(["config", "set", "fastfetch", value])
            .assert()
            .success();
        assert_eq!(snapshot::tree(td.path()), before_repeat);
        command(td.path())
            .args(["restore", &points[0].id, "--dry-run", "--json"])
            .assert()
            .success();
        assert_eq!(snapshot::tree(td.path()), before_repeat);
        assert!(execute_restore_with_env(&env, &points[0].id)
            .unwrap()
            .results
            .iter()
            .all(|file| file.success));
        assert_eq!(
            fs::read(env.managed_file("managed/shell/env.bash")).unwrap(),
            b"original \xff"
        );
        assert_eq!(
            fs::metadata(env.managed_file("managed/shell/env.bash"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        assert!(!env.managed_file("managed/shell/env.fish").exists());
        assert!(!env.managed_file("managed/starship/plain.toml").exists());
        assert_eq!(
            fs::read(env.managed_file("user/keep")).unwrap(),
            b"unrelated"
        );
        if enabled {
            assert!(!marker.exists());
        } else {
            assert_eq!(fs::read(&marker).unwrap(), b"old bounded marker");
            assert_eq!(
                fs::metadata(marker).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        assert!(!td.path().join("unexpected-launch").exists());
    }
}

#[test]
fn config_toggle_cli_unsafe_output_or_invalid_input_keeps_old_flag_and_never_reports_success() {
    for variant in ["directory", "symlink", "fifo", "invalid-toml"] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        write(&env.managed_file("autorun-fastfetch"), b"old marker", 0o640);
        let bad = env.managed_file("managed/shell/env.fish");
        fs::create_dir_all(bad.parent().unwrap()).unwrap();
        match variant {
            "directory" => fs::create_dir(&bad).unwrap(),
            "symlink" => symlink(env.managed_file("autorun-fastfetch"), &bad).unwrap(),
            "fifo" => {
                let path = CString::new(bad.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "invalid-toml" => write(
                &env.managed_file("config.toml"),
                b"PRIVATE_CONTENT = [",
                0o600,
            ),
            _ => unreachable!(),
        }
        let before = snapshot::tree(&env.managed_file(""));
        let output = command(td.path())
            .args(["config", "set", "fastfetch", "disable"])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(!String::from_utf8_lossy(&output.stdout).contains("disabled"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONTENT"));
        assert_eq!(snapshot::tree(&env.managed_file("")), before);
        assert!(list_restore_points_with_env(&env).unwrap().is_empty());
        assert!(!td.path().join("unexpected-launch").exists());
    }
}
