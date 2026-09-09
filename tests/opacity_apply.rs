//! Standalone opacity uses private file checkpoints; no terminal is launched.
use slate_cli::config::{
    execute_restore_with_env, list_restore_points_with_env, preview_restore_with_env,
    ConfigManager, ConfigWriteGuard,
};
use slate_cli::env::SlateEnv;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

const OUTPUTS: [(&str, &str); 4] = [
    ("ghostty/opacity.conf", "background-opacity = 0.85\n"),
    ("ghostty/blur.conf", "background-blur = 20\n"),
    ("alacritty/opacity.toml", "[window]\nopacity = 0.85\n"),
    ("kitty/opacity.conf", "background_opacity 0.85\n"),
];

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .args(["--quiet", "config", "set", "opacity", "frosted"])
        .timeout(Duration::from_secs(10));
    command
}

fn write(path: &Path, bytes: impl AsRef<[u8]>, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn fixture(home: &Path) -> SlateEnv {
    let env = SlateEnv::with_home(home.to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    for tool in ["osascript", "kitten", "ghostty", "killall"] {
        write(
            &home.join("bin").join(tool),
            "#!/bin/sh\nprintf 'DO_NOT_LAUNCH_TERMINALS' >&2\nexit 99\n",
            0o755,
        );
    }
    env
}

fn files(home: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    tree_snapshot::tree(home)
        .into_iter()
        .filter(|(path, _)| {
            !fs::symlink_metadata(path).unwrap().is_dir()
                && !path.starts_with(home.join(".cache/slate/backups"))
                && path != &home.join(".cache/slate/preview-session.lock")
        })
        .collect()
}

#[test]
fn standalone_opacity_round_trips_bytes_permissions_and_absence() {
    for existing in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = fixture(home.path());
        if existing {
            write(&env.managed_file("current-opacity"), "solid", 0o600);
            for (index, (path, _)) in OUTPUTS.iter().enumerate() {
                // Mixed present/absent generated files, including arbitrary bytes.
                if index % 2 == 0 {
                    write(
                        &env.managed_file(&format!("managed/{path}")),
                        b"# original\xff\r\n",
                        0o640,
                    );
                }
            }
        }
        write(
            &env.managed_file("user/ghostty/keep.conf"),
            "# unrelated\n",
            0o600,
        );
        let before = files(home.path());
        let output = command(home.path()).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stderr}");
        assert!(!stderr.contains("DO_NOT_LAUNCH_TERMINALS"));
        for (path, expected) in OUTPUTS {
            assert_eq!(
                fs::read_to_string(env.managed_file(&format!("managed/{path}"))).unwrap(),
                expected
            );
        }
        assert_eq!(
            fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
            "frosted"
        );
        let points = list_restore_points_with_env(&env).unwrap();
        assert_eq!(points.len(), 1);
        let point = &points[0];
        assert_eq!(point.theme_name, "pre-opacity");
        assert_eq!(point.entries.len(), 5);
        assert!(!point.reapplies_theme());
        assert!(stderr.contains(&format!("slate restore {} --dry-run", point.id)));
        for entry in &point.entries {
            if let Some(path) = &entry.backup_path {
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        let after = files(home.path());
        for path in before.keys().chain(after.keys()) {
            if before.get(path) != after.get(path) {
                assert!(
                    point.entries.iter().any(|e| &e.original_path == path),
                    "{} not backed up",
                    path.display()
                );
            }
        }
        let tree = tree_snapshot::tree(home.path());
        let preview = preview_restore_with_env(&env, &point.id).unwrap();
        assert!(!preview.may_regenerate_theme_files);
        assert_eq!(preview.blocked_count(), 0);
        assert_eq!(tree_snapshot::tree(home.path()), tree);
        let restored = execute_restore_with_env(&env, &point.id).unwrap();
        assert!(restored.is_fully_successful());
        assert_eq!(files(home.path()), before);
        let undo = execute_restore_with_env(&env, &restored.pre_restore_point_id).unwrap();
        assert!(undo.is_fully_successful());
        assert_eq!(files(home.path()), after);
    }
}

#[test]
fn unsafe_opacity_targets_stop_before_any_configuration_write() {
    for kind in [
        "link",
        "directory",
        "fifo",
        "oversized",
        "parent-link",
        "escape",
        "overlap",
        "conflicting-alias",
    ] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = fixture(home.path());
        write(&env.managed_file("current-opacity"), "solid", 0o600);
        let target = env.managed_file("managed/kitty/opacity.conf");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match kind {
            "link" => {
                write(&outside.path().join("original"), b"PRIVATE_BYTES", 0o600);
                symlink(outside.path().join("original"), &target).unwrap();
            }
            "directory" => fs::create_dir(&target).unwrap(),
            "fifo" => {
                let path = std::ffi::CString::new(target.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "oversized" => fs::File::create(&target)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            "parent-link" => symlink(
                home.path().join("missing"),
                env.managed_file("managed/ghostty"),
            )
            .unwrap(),
            "escape" => symlink(outside.path(), env.managed_file("managed/ghostty")).unwrap(),
            "overlap" => symlink(
                env.slate_cache_dir().join("backups"),
                env.managed_file("managed/ghostty"),
            )
            .unwrap(),
            "conflicting-alias" => symlink(
                target.parent().unwrap(),
                env.managed_file("managed/ghostty"),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(home.path());
        let external_before = tree_snapshot::tree(outside.path());
        let output = command(home.path()).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{kind}: {stderr}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Opacity set to"));
        assert!(!stderr.contains("PRIVATE_BYTES") && !stderr.contains("DO_NOT_LAUNCH_TERMINALS"));
        assert_eq!(tree_snapshot::tree(home.path()), before, "{kind}");
        assert_eq!(
            tree_snapshot::tree(outside.path()),
            external_before,
            "{kind}"
        );
        assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    }
}

#[test]
fn opacity_directory_aliases_remain_recoverable_inside_the_profile() {
    let home = tempfile::tempdir().unwrap();
    let env = fixture(home.path());
    let actual = home.path().join("actual-config");
    fs::create_dir(&actual).unwrap();
    fs::create_dir_all(env.managed_file("managed")).unwrap();
    symlink(&actual, env.managed_file("managed/ghostty")).unwrap();
    write(&actual.join("opacity.conf"), "# custom opacity\n", 0o640);
    let before = files(home.path());
    let output = command(home.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(files(home.path()), before);
    assert_eq!(
        fs::read_link(env.managed_file("managed/ghostty")).unwrap(),
        actual
    );
}

#[test]
fn repeated_opacity_preserves_file_identity_and_backup_history() {
    let home = tempfile::tempdir().unwrap();
    let env = fixture(home.path());
    command(home.path()).assert().success();
    let before = tree_snapshot::tree(home.path());
    let identities: Vec<_> = OUTPUTS
        .iter()
        .map(|(name, _)| env.managed_file(&format!("managed/{name}")))
        .chain(std::iter::once(env.managed_file("current-opacity")))
        .map(|path| {
            let meta = fs::metadata(&path).unwrap();
            (path, meta.ino(), meta.mtime(), meta.mtime_nsec())
        })
        .collect();
    let output = command(home.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        list_restore_points_with_env(&env).unwrap().len(),
        1,
        "no-op must not create a second checkpoint"
    );
    assert_eq!(tree_snapshot::tree(home.path()), before);
    for (path, inode, modified, nanos) in identities {
        let meta = fs::metadata(path).unwrap();
        assert_eq!(
            (meta.ino(), meta.mtime(), meta.mtime_nsec()),
            (inode, modified, nanos)
        );
    }
}

#[test]
fn matching_preset_repairs_only_missing_or_changed_files() {
    for damaged in [
        "managed/ghostty/opacity.conf",
        "managed/ghostty/blur.conf",
        "managed/alacritty/opacity.toml",
        "managed/kitty/opacity.conf",
        "current-opacity",
    ] {
        for missing in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let env = fixture(home.path());
            command(home.path()).assert().success();
            let target = env.managed_file(damaged);
            if missing {
                fs::remove_file(&target).unwrap();
            } else {
                write(&target, b"# PRIVATE_MANAGED_EDIT\xff\r\n", 0o640);
            }
            let before = files(home.path());
            let old_points = list_restore_points_with_env(&env).unwrap();
            let unchanged: Vec<_> = OUTPUTS
                .iter()
                .map(|(name, _)| env.managed_file(&format!("managed/{name}")))
                .chain(std::iter::once(env.managed_file("current-opacity")))
                .filter(|path| path != &target)
                .map(|path| {
                    let meta = fs::metadata(&path).unwrap();
                    (path, meta.ino(), meta.mtime(), meta.mtime_nsec())
                })
                .collect();
            command(home.path()).assert().success();
            let points = list_restore_points_with_env(&env).unwrap();
            assert_eq!(points.len(), 2);
            for (path, inode, modified, nanos) in unchanged {
                let meta = fs::metadata(path).unwrap();
                assert_eq!(
                    (meta.ino(), meta.mtime(), meta.mtime_nsec()),
                    (inode, modified, nanos),
                    "{damaged}"
                );
            }
            for (name, expected) in OUTPUTS {
                assert_eq!(
                    fs::read_to_string(env.managed_file(&format!("managed/{name}"))).unwrap(),
                    expected
                );
            }
            assert_eq!(
                fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
                "frosted"
            );
            if !missing {
                assert_eq!(
                    fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                    0o640
                );
            }
            let point = points
                .iter()
                .find(|p| old_points.iter().all(|old| p.id != old.id))
                .unwrap();
            assert!(execute_restore_with_env(&env, &point.id)
                .unwrap()
                .is_fully_successful());
            assert_eq!(files(home.path()), before);
        }
    }
}

#[test]
fn public_opacity_writers_share_templates_and_noop_behavior() {
    use slate_cli::opacity::OpacityPreset;
    for (preset, value, blur) in [
        (OpacityPreset::Solid, "1", "0"),
        (OpacityPreset::Frosted, "0.85", "20"),
        (OpacityPreset::Clear, "0.75", "0"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let expected = [
            format!("background-opacity = {value}\n"),
            format!("background-blur = {blur}\n"),
            format!("[window]\nopacity = {value}\n"),
            format!("background_opacity {value}\n"),
        ];
        let writers = [
            slate_cli::adapter::ghostty::write_opacity_config,
            slate_cli::adapter::ghostty::write_blur_radius,
            slate_cli::adapter::alacritty::write_opacity_config,
            slate_cli::adapter::kitty::write_opacity_config,
        ];
        for ((writer, (name, _)), expected) in writers.into_iter().zip(OUTPUTS).zip(expected) {
            writer(&env, preset).unwrap();
            let path = env.managed_file(&format!("managed/{name}"));
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let meta = fs::metadata(&path).unwrap();
            writer(&env, preset).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
            let after = fs::metadata(&path).unwrap();
            assert_eq!(
                (after.ino(), after.mtime(), after.mtime_nsec(), after.mode()),
                (meta.ino(), meta.mtime(), meta.mtime_nsec(), meta.mode())
            );
        }
        assert!(!env.managed_file("current-opacity").exists());
        assert!(!env.slate_cache_dir().exists());
    }
}

#[test]
fn matching_preset_never_bypasses_unsafe_or_unreadable_files() {
    for kind in ["matching-link", "dangling-link", "oversized-state", "fifo"] {
        let home = tempfile::tempdir().unwrap();
        let env = fixture(home.path());
        command(home.path()).assert().success();
        let target = env.managed_file("managed/kitty/opacity.conf");
        match kind {
            "matching-link" | "dangling-link" => {
                let other = home.path().join("link-target");
                if kind == "matching-link" {
                    fs::write(&other, OUTPUTS[3].1).unwrap();
                }
                fs::remove_file(&target).unwrap();
                symlink(other, &target).unwrap();
            }
            "oversized-state" => {
                fs::File::create(env.managed_file("current-opacity"))
                    .unwrap()
                    .set_len(4097)
                    .unwrap();
            }
            "fifo" => {
                fs::remove_file(&target).unwrap();
                let path = std::ffi::CString::new(target.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(home.path());
        let output = command(home.path()).output().unwrap();
        assert!(!output.status.success(), "{kind}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Opacity set to"));
        assert_eq!(tree_snapshot::tree(home.path()), before, "{kind}");
        assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
    }
}
