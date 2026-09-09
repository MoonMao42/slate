//! Read-only diagnostics in private profiles, with no terminal launches.
use slate_cli::{env::SlateEnv, opacity::OpacityPreset};
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn command(home: &Path, isolated: bool) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .current_dir(home)
        .timeout(Duration::from_secs(5));
    if isolated {
        command.env("SLATE_HOME", home);
    }
    command
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn seed(env: &SlateEnv, preset: OpacityPreset) {
    write(
        &env.managed_file("current-opacity"),
        preset.to_string().to_lowercase(),
    );
    slate_cli::adapter::ghostty::write_opacity_config(env, preset).unwrap();
    slate_cli::adapter::ghostty::write_blur_radius(env, preset).unwrap();
    slate_cli::adapter::alacritty::write_opacity_config(env, preset).unwrap();
    slate_cli::adapter::kitty::write_opacity_config(env, preset).unwrap();
    for tool in ["ghostty", "kitten", "alacritty", "osascript"] {
        let path = env.home().join("bin").join(tool);
        write(
            &path,
            "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nprintf 'UNEXPECTED_TOOL' >&2\nexit 92\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn json(command: &mut assert_cmd::Command) -> serde_json::Value {
    let output = command
        .args(["doctor", "opacity", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE_CONTENT") && !text.contains("UNEXPECTED_TOOL"));
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["target"], "opacity");
    assert!(report["scope"].as_str().unwrap().contains("exact-byte"));
    assert!(report["scope"]
        .as_str()
        .unwrap()
        .contains("no tool is launched"));
    report
}

fn count(report: &serde_json::Value, code: &str) -> usize {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["code"] == code)
        .count()
}

fn check<'a>(report: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == code)
        .unwrap_or_else(|| panic!("missing {code}: {report}"))
}

#[test]
fn opacity_doctor_compares_all_presets_without_claiming_live_application() {
    for preset in [
        OpacityPreset::Solid,
        OpacityPreset::Frosted,
        OpacityPreset::Clear,
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        seed(&env, preset);
        let path = env.managed_file("current-opacity");
        let meta = fs::metadata(&path).unwrap();
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut command(home.path(), true));
        assert_eq!(check(&report, "preset_valid")["status"], "ok");
        assert_eq!(count(&report, "output_matches"), 4);
        assert_eq!(count(&report, "output_differs"), 0);
        assert_eq!(tree_snapshot::tree(home.path()), before);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(
            (meta.ino(), meta.modified().unwrap()),
            (after.ino(), after.modified().unwrap())
        );

        // A comment/CRLF change is byte drift, not proof of invalid runtime syntax.
        write(
            &env.managed_file("managed/ghostty/blur.conf"),
            "# PRIVATE_CONTENT\r\nbackground-blur = 20\r\n",
        );
        fs::remove_file(env.managed_file("managed/kitty/opacity.conf")).unwrap();
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut command(home.path(), true));
        assert_eq!(count(&report, "output_matches"), 2);
        assert_eq!(check(&report, "output_differs")["status"], "warning");
        assert!(check(&report, "output_differs")["message"]
            .as_str()
            .unwrap()
            .contains("not a syntax"));
        assert_eq!(check(&report, "output_missing")["status"], "info");
        assert!(check(&report, "output_differs")["suggestion"]
            .as_str()
            .unwrap()
            .contains(&format!(
                "slate config set opacity {}",
                preset.to_string().to_lowercase()
            )));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn opacity_doctor_distinguishes_unset_invalid_and_noncanonical_state() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(&env, OpacityPreset::Frosted);
    let path = env.managed_file("current-opacity");
    for (value, code) in [
        (None, "preset_unset"),
        (Some(&b" \n"[..]), "preset_unset"),
        (Some(&b"PRIVATE_CONTENT"[..]), "preset_invalid"),
        (Some(&b"PRIVATE_CONTENT\xff"[..]), "preset_invalid"),
    ] {
        if let Some(value) = value {
            write(&path, value);
        } else {
            fs::remove_file(&path).unwrap();
        }
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut command(home.path(), true));
        check(&report, code);
        assert_eq!(count(&report, "output_matches"), 0);
        assert_eq!(count(&report, "output_uncompared"), 4);
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    write(&path, " FrOsTeD\r\n");
    let before = tree_snapshot::tree(home.path());
    let report = json(&mut command(home.path(), true));
    assert_eq!(check(&report, "preset_noncanonical")["status"], "info");
    assert_eq!(count(&report, "output_matches"), 4);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn opacity_doctor_rejects_unsafe_sources_without_hiding_other_files() {
    for (name, code, limit) in [
        ("current-opacity", "preset_unreadable", 4096),
        (
            "managed/kitty/opacity.conf",
            "output_unreadable",
            8 * 1024 * 1024,
        ),
    ] {
        for kind in ["fifo", "directory", "link", "dangling", "large"] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            seed(&env, OpacityPreset::Frosted);
            let path = env.managed_file(name);
            fs::remove_file(&path).unwrap();
            match kind {
                "fifo" => {
                    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
                }
                "directory" => fs::create_dir(&path).unwrap(),
                "link" | "dangling" => {
                    let target = home.path().join("linked-content");
                    if kind == "link" {
                        write(&target, "PRIVATE_CONTENT");
                    }
                    symlink(target, &path).unwrap();
                }
                "large" => fs::File::create(&path).unwrap().set_len(limit + 1).unwrap(),
                _ => unreachable!(),
            }
            let before = tree_snapshot::tree(home.path());
            let report = json(&mut command(home.path(), true));
            assert_eq!(check(&report, code)["status"], "error");
            assert!(!check(&report, code)["message"]
                .as_str()
                .unwrap()
                .contains("cancelled before applying"));
            assert_eq!(
                count(&report, "output_matches"),
                if code == "preset_unreadable" { 0 } else { 3 }
            );
            assert_eq!(tree_snapshot::tree(home.path()), before);
        }
    }
}

#[test]
fn opacity_doctor_uses_profile_paths_and_reports_layout_and_session_limits() {
    for case in [
        "xdg",
        "isolated",
        "directory-alias",
        "conflicting-alias",
        "backup-storage",
        "remote",
    ] {
        let home = tempfile::tempdir().unwrap();
        let xdg = home.path().join("custom-xdg");
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.path().as_os_str().to_owned()),
            "XDG_CONFIG_HOME" if case == "xdg" => Some(xdg.as_os_str().to_owned()),
            _ => None,
        })
        .unwrap();
        seed(&env, OpacityPreset::Clear);
        let mut cmd = command(home.path(), case != "xdg" && case != "remote");
        if case == "xdg" {
            cmd.env("XDG_CONFIG_HOME", &xdg);
        }
        if case == "isolated" {
            cmd.env("XDG_CONFIG_HOME", &xdg);
        }
        if case == "remote" {
            cmd.env("SSH_CONNECTION", "remote session");
        }
        if case == "directory-alias" || case == "conflicting-alias" {
            let original = env.managed_file("managed/ghostty");
            let actual = home.path().join("actual-ghostty");
            fs::rename(&original, &actual).unwrap();
            symlink(
                if case == "directory-alias" {
                    actual
                } else {
                    env.managed_file("managed/kitty")
                },
                original,
            )
            .unwrap();
        }
        if case == "backup-storage" {
            write(&env.slate_cache_dir().join("backups"), "PRIVATE_CONTENT");
        }
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut cmd);
        assert_eq!(
            check(&report, "preset_valid")["path"],
            env.managed_file("current-opacity").to_str().unwrap()
        );
        match case {
            "conflicting-alias" => {
                assert_eq!(check(&report, "output_alias_conflict")["status"], "error");
            }
            "backup-storage" => {
                assert_eq!(check(&report, "write_paths_blocked")["status"], "error");
                assert_eq!(count(&report, "output_matches"), 4);
            }
            "remote" => {
                check(&report, "remote_session");
            }
            _ => {
                assert_eq!(count(&report, "output_matches"), 4);
            }
        }
        if case == "isolated" {
            check(&report, "isolated_profile");
            assert!(!report.to_string().contains(xdg.to_str().unwrap()));
        }
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn opacity_doctor_does_not_read_escaping_isolated_directories() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(&env, OpacityPreset::Clear);
    let original = env.managed_file("managed/ghostty");
    fs::rename(&original, home.path().join("saved-ghostty")).unwrap();
    write(&outside.path().join("opacity.conf"), "PRIVATE_CONTENT");
    symlink(outside.path(), original).unwrap();
    let before = tree_snapshot::tree(home.path());
    let external_before = tree_snapshot::tree(outside.path());
    let report = json(&mut command(home.path(), true));
    assert_eq!(count(&report, "output_unreadable"), 2);
    assert!(check(&report, "output_unreadable")["message"]
        .as_str()
        .unwrap()
        .contains("escapes the isolated"));
    assert_eq!(count(&report, "output_matches"), 2);
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert_eq!(tree_snapshot::tree(outside.path()), external_before);
}
