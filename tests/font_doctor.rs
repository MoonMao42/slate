//! Real diagnostics over private profiles; never use a native font/cache engine.
use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::Path,
    time::Duration,
};

#[path = "support/tree.rs"]
mod snapshot;

#[path = "font_doctor/references.rs"]
mod references_tests;

const FAMILY: &str = "SlateDoctorFixture Nerd Font";
const FONT: &[u8] = b"\0\x01\0\0private-synthetic-font-fixture";

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn command(env: &SlateEnv, isolated: bool) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("PATH", env.home().join("bin"))
        .env("XDG_CONFIG_HOME", env.xdg_config_home())
        .env("XDG_DATA_HOME", env.xdg_data_home())
        .env("XDG_CACHE_HOME", env.cache_dir())
        .env("NO_COLOR", "1")
        .current_dir(env.home())
        .timeout(Duration::from_secs(10));
    if isolated {
        command.env("SLATE_HOME", env.home());
    }
    command
}

fn checks<'a>(report: &'a serde_json::Value, code: &str) -> Vec<&'a serde_json::Value> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| check["code"] == code)
        .collect()
}

fn json(command: &mut assert_cmd::Command) -> serde_json::Value {
    let output = command
        .args(["doctor", "font", "--json"])
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
    assert_eq!(report["target"], "font");
    assert!(report["version_probe"].is_null());
    assert!(report["scope"]
        .as_str()
        .unwrap()
        .contains("No tool is launched"));
    report
}

fn seed(env: &SlateEnv) {
    write(&env.managed_file("current-font"), FAMILY);
    write(
        &env.managed_file("managed/ghostty/font.conf"),
        format!("font-family = \"{FAMILY}\"\n"),
    );
    write(
        &env.managed_file("managed/alacritty/font.toml"),
        format!("[font.normal]\nfamily = \"{FAMILY}\"\n"),
    );
    write(
        &env.managed_file("managed/kitty/font.conf"),
        format!("font_family {FAMILY}\n"),
    );
    write(
        &slate_cli::platform::fonts::user_font_dir(env)
            .join("nested/SlateDoctorFixtureNerdFont-Regular.TTF"),
        FONT,
    );
    for tool in [
        "fc-cache",
        "fc-match",
        "fc-list",
        "brew",
        "curl",
        "ghostty",
        "kitten",
        "alacritty",
        "osascript",
    ] {
        let path = env.home().join("bin").join(tool);
        write(
            &path,
            "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nprintf UNEXPECTED_TOOL >&2\nexit 92\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn font_doctor_empty_and_busy_profiles_are_read_only_without_default_selection() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    let before = snapshot::tree(home.path());
    let report = json(&mut command(&env, true));
    assert_eq!(checks(&report, "family_unset").len(), 1);
    assert_eq!(checks(&report, "output_missing").len(), 3);
    assert!(report["font_inventory"]["selected_family"].is_null());
    assert_eq!(snapshot::tree(home.path()), before);

    seed(&env);
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    let config = env.managed_file("config.toml");
    let path = std::ffi::CString::new(config.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let before = snapshot::tree(home.path());
    let state_before = fs::metadata(env.managed_file("current-font")).unwrap();
    let report = json(&mut command(&env, true));
    assert_eq!(checks(&report, "output_matches").len(), 3);
    assert_eq!(checks(&report, "family_candidate_found").len(), 1);
    let output = command(&env, true)
        .args(["doctor", "font"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(FAMILY) && text.contains("font doctor"));
    assert!(!text.contains("PRIVATE_CONTENT") && !text.contains('\u{1b}'));
    assert!(
        text.find("Saved font family:").unwrap()
            < text.find("Selected user-font directory").unwrap()
    );
    let state_after = fs::metadata(env.managed_file("current-font")).unwrap();
    assert_eq!(
        (
            state_before.ino(),
            state_before.mode(),
            state_before.modified().unwrap()
        ),
        (
            state_after.ino(),
            state_after.mode(),
            state_after.modified().unwrap()
        )
    );
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn font_doctor_separates_output_drift_missing_files_and_partial_discovery() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    write(
        &env.managed_file("managed/kitty/font.conf"),
        "PRIVATE_CONTENT",
    );
    fs::remove_file(env.managed_file("managed/alacritty/font.toml")).unwrap();
    let fonts = slate_cli::platform::fonts::user_font_dir(&env);
    symlink(fonts.join("missing"), fonts.join("broken.ttf")).unwrap();
    let before = snapshot::tree(home.path());
    let report = json(&mut command(&env, true));
    for code in [
        "output_matches",
        "output_differs",
        "output_missing",
        "family_candidate_found",
        "scan_incomplete",
    ] {
        assert_eq!(checks(&report, code).len(), 1, "{code}");
    }
    assert_eq!(report["font_inventory"]["scan_complete"], false);
    assert!(!checks(&report, "scan_issue").is_empty());
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn font_doctor_bad_saved_state_does_not_leak_bytes_or_hide_outputs() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    for (bytes, code) in [
        (b" \n".as_slice(), "family_unset"),
        (b"PRIVATE_CONTENT\xff".as_slice(), "family_invalid"),
        (
            b"PRIVATE_CONTENT\nfont_family=Bad".as_slice(),
            "family_invalid",
        ),
    ] {
        write(&env.managed_file("current-font"), bytes);
        let before = snapshot::tree(home.path());
        let report = json(&mut command(&env, true));
        assert_eq!(checks(&report, code).len(), 1);
        assert_eq!(checks(&report, "output_uncompared").len(), 3);
        assert!(report["font_inventory"]["selected_family"].is_null());
        assert_eq!(snapshot::tree(home.path()), before);
    }
    write(&env.managed_file("current-font"), format!(" {FAMILY}\r\n"));
    let before = snapshot::tree(home.path());
    let report = json(&mut command(&env, true));
    assert_eq!(checks(&report, "family_noncanonical").len(), 1);
    assert_eq!(checks(&report, "output_matches").len(), 3);
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn font_doctor_rejects_unsafe_sources_and_preserves_unrelated_files() {
    for (relative, code, limit) in [
        ("current-font", "family_unreadable", 4096),
        (
            "managed/kitty/font.conf",
            "output_unreadable",
            8 * 1024 * 1024,
        ),
    ] {
        for kind in ["fifo", "directory", "link", "dangling", "large"] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().into());
            seed(&env);
            let path = env.managed_file(relative);
            fs::remove_file(&path).unwrap();
            match kind {
                "fifo" => {
                    let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                }
                "directory" => fs::create_dir(&path).unwrap(),
                "link" | "dangling" => {
                    let target = env.home().join("external-file");
                    if kind == "link" {
                        write(&target, "PRIVATE_CONTENT");
                    }
                    symlink(target, &path).unwrap();
                }
                "large" => fs::File::create(&path).unwrap().set_len(limit + 1).unwrap(),
                _ => unreachable!(),
            }
            let before = snapshot::tree(home.path());
            let report = json(&mut command(&env, true));
            assert_eq!(checks(&report, code).len(), 1, "{kind}, {relative}");
            assert_eq!(
                checks(&report, "output_matches").len(),
                if code == "family_unreadable" { 0 } else { 2 }
            );
            assert_eq!(snapshot::tree(home.path()), before);
        }
    }
}

#[test]
fn font_doctor_honors_xdg_and_isolation_and_explains_remote_scope() {
    for isolated in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" | "SLATE_HOME" if key == "HOME" || isolated => Some(home.path().into()),
            "XDG_CONFIG_HOME" => Some(outside.path().join("config").into_os_string()),
            "XDG_DATA_HOME" => Some(outside.path().join("data").into_os_string()),
            _ => None,
        })
        .unwrap();
        seed(&env);
        let before = snapshot::tree(home.path());
        let external_before = snapshot::tree(outside.path());
        let report = json(
            command(&env, isolated)
                .env("XDG_CONFIG_HOME", outside.path().join("config"))
                .env("XDG_DATA_HOME", outside.path().join("data"))
                .env("SSH_CONNECTION", "192.0.2.1 1234 192.0.2.2 22"),
        );
        assert_eq!(
            checks(&report, "user_font_directory")[0]["path"],
            slate_cli::platform::fonts::user_font_dir(&env)
                .to_str()
                .unwrap()
        );
        assert_eq!(checks(&report, "family_candidate_found").len(), 1);
        assert_eq!(checks(&report, "output_matches").len(), 3);
        // Profile isolation and SSH transport are independent captured facts.
        assert_eq!(checks(&report, "remote_session").len(), 1);
        if isolated {
            assert!(!report
                .to_string()
                .contains(outside.path().to_str().unwrap()));
        }
        assert_eq!(snapshot::tree(home.path()), before);
        assert_eq!(snapshot::tree(outside.path()), external_before);
    }
}

#[test]
fn font_doctor_does_not_read_config_escaping_isolated_profile_or_run_native_flag() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    seed(&env);
    let directory = env.managed_file("managed/ghostty");
    fs::rename(&directory, home.path().join("saved-ghostty")).unwrap();
    write(&outside.path().join("font.conf"), "PRIVATE_CONTENT");
    symlink(outside.path(), directory).unwrap();
    let before = snapshot::tree(home.path());
    let external_before = snapshot::tree(outside.path());
    let report = json(&mut command(&env, true));
    assert_eq!(checks(&report, "output_unreadable").len(), 1);
    assert_eq!(checks(&report, "output_matches").len(), 2);
    command(&env, true)
        .args(["doctor", "font", "--check-version"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("only supported"));
    assert_eq!(snapshot::tree(home.path()), before);
    assert_eq!(snapshot::tree(outside.path()), external_before);
}
