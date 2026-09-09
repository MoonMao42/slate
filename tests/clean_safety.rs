//! Exercise the real clean command only in a private HOME with harmless process
//! spies. A regression must never reach the host's pkill or application reload.
use slate_cli::config::{
    execute_restore_with_env, list_restore_points_with_env, preview_restore_with_env,
    ConfigWriteGuard, OriginalFileState, RestoreAction,
};
use slate_cli::env::SlateEnv;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn write(path: &Path, bytes: impl AsRef<[u8]>, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn command(home: &Path, spies: &Path, outside: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env("SLATE_HOME", home)
        .env("HOME", outside)
        .env("XDG_CONFIG_HOME", outside.join("config"))
        .env("XDG_CACHE_HOME", outside.join("cache"))
        .env("ZDOTDIR", outside)
        .env("NVIM_APPNAME", "host-profile")
        .env("STARSHIP_CONFIG", outside.join("starship.toml"))
        .env("PATH", spies)
        .env("SLATE_CLEAN_PROBE", outside.join("process-calls"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(10));
    command
}

fn spies(path: &Path) {
    for name in ["pkill", "pgrep", "osascript", "killall", "defaults", "nvim"] {
        write(
            &path.join(name),
            "#!/bin/sh\nprintf '%s\\n' \"${0##*/}\" >> \"$SLATE_CLEAN_PROBE\"\nexit 97\n",
            0o755,
        );
    }
}

fn file_tree(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        let meta = fs::symlink_metadata(path).unwrap();
        if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), out);
            }
        } else if meta.is_file() {
            out.insert(
                path.strip_prefix(root).unwrap().to_owned(),
                (meta.permissions().mode() & 0o777, fs::read(path).unwrap()),
            );
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

#[test]
fn clean_isolated_profile_backs_up_all_removed_files_and_restores_modes() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile");
    let outside = td.path().join("outside");
    let tools = td.path().join("spies");
    spies(&tools);
    write(&outside.join("starship.toml"), "palette = 'slate'\n", 0o640);
    let host_before = file_tree(&outside);
    let env = SlateEnv::with_home(home.clone());
    let marker = format!(
        "# keep\n{}\nsource '/slate/managed'\n{}\n# also keep\n",
        slate_cli::adapter::marker_block::START,
        slate_cli::adapter::marker_block::END,
    );
    for path in [env.zshrc_path(), env.bashrc_path(), env.bash_profile_path()]
        .into_iter()
        .chain(env.tmux_config_candidates())
    {
        write(&path, &marker, 0o640);
    }
    let removed = [
        (
            ".config/slate/config.toml",
            "[preferences]\nsound = false\n",
            0o600,
        ),
        (".config/slate/current", "nord\n", 0o600),
        (
            ".config/slate/managed/bin/slate-dark-mode-notify",
            "#!/bin/sh\nexit 0\n",
            0o755,
        ),
        (
            ".config/slate/managed/ghostty/font.conf",
            "font-size = 12\n",
            0o640,
        ),
        (
            ".config/slate/custom-state.txt",
            "preserve in backup\n",
            0o600,
        ),
        (".config/nvim/lua/slate/init.lua", "-- loader\n", 0o644),
        (".config/nvim/colors/slate-nord.lua", "-- shim\n", 0o644),
        (".cache/slate/current_theme.lua", "-- state\n", 0o600),
        (".config/fish/conf.d/slate.fish", "# loader\n", 0o640),
        (".config/opencode/tui.json", "{\"theme\":\"system\"}", 0o600),
        (
            ".config/opencode/tui.jsonc",
            "{\"theme\":\"system\"}",
            0o600,
        ),
    ];
    for (path, bytes, mode) in removed {
        write(&home.join(path), bytes, mode);
    }
    write(
        &home.join(".config/starship.toml"),
        "palette = 'slate'\n",
        0o640,
    );
    let kept = [
        ".config/slate/user/ghostty/local.conf",
        ".config/nvim/colors/slate-notes.txt",
    ];
    for path in kept {
        write(&home.join(path), "keep my content\n", 0o640);
    }
    let before = file_tree(&home);
    let output = command(&home, &tools, &outside)
        .arg("clean")
        .assert()
        .success()
        .stderr(predicates::str::contains("isolated profile"))
        .get_output()
        .clone();
    assert_eq!(
        file_tree(&outside),
        host_before,
        "host paths or processes touched"
    );
    for (path, _, _) in removed {
        assert!(!home.join(path).exists(), "not removed: {path}");
    }
    for path in kept {
        assert_eq!(
            fs::read_to_string(home.join(path)).unwrap(),
            "keep my content\n"
        );
    }
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    let point = &points[0];
    assert!(String::from_utf8_lossy(&output.stderr).contains(&point.id));
    assert!(point.theme_name.starts_with("pre-clean"));
    let snapshot_dir = env.slate_cache_dir().join("backups").join(&point.id);
    assert_eq!(
        fs::metadata(&snapshot_dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for entry in fs::read_dir(&snapshot_dir).unwrap() {
        assert_eq!(
            entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    for entry in &point.entries {
        assert!(entry.original_path.starts_with(&home));
        if entry.original_state == OriginalFileState::Present {
            let original = &before[entry.original_path.strip_prefix(&home).unwrap()];
            assert_eq!(entry.unix_mode, Some(original.0));
            assert_eq!(
                fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
                original.1
            );
        }
    }
    for (path, _, _) in removed {
        assert!(point
            .entries
            .iter()
            .any(|entry| entry.original_path == home.join(path)));
    }
    assert!(
        !preview_restore_with_env(&env, &point.id)
            .unwrap()
            .may_regenerate_theme_files
    );
    // The library restore has no app/process side effects. All manifest paths
    // have been checked against this fixture before permitting the write.
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    for (path, expected) in &before {
        assert_eq!(
            file_tree(&home).get(path),
            Some(expected),
            "restore mismatch: {path:?}"
        );
    }
    let watcher = home.join(removed[2].0);
    fs::set_permissions(&watcher, fs::Permissions::from_mode(0o600)).unwrap();
    let plan = preview_restore_with_env(&env, &point.id).unwrap();
    assert!(plan
        .changes
        .iter()
        .any(|change| change.original_path == watcher && change.action == RestoreAction::Replace));
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        fs::metadata(watcher).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(file_tree(&outside), host_before);
}

#[test]
fn clean_refuses_unsafe_targets_and_incomplete_backups_before_deleting() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile");
    let outside = td.path().join("outside");
    let tools = td.path().join("spies");
    spies(&tools);
    write(&outside.join("protected"), "outside content\n", 0o600);
    let env = SlateEnv::with_home(home.clone());
    write(
        &env.managed_file("config.toml"),
        "[preferences]\nsound = false\n",
        0o600,
    );
    write(
        &env.managed_file("managed/keep.txt"),
        "managed content\n",
        0o600,
    );
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let original = file_tree(&home);
    let host_before = file_tree(&outside);
    for link in [home.join(".zshrc"), env.managed_file("managed/linked")] {
        symlink(outside.join("protected"), &link).unwrap();
        command(&home, &tools, &outside)
            .arg("clean")
            .assert()
            .failure();
        assert_eq!(file_tree(&home), original);
        assert_eq!(file_tree(&outside), host_before);
        fs::remove_file(link).unwrap();
    }
    // An ancestor link is dangerous even if the target file is ordinary.
    symlink(&outside, home.join(".config/nvim")).unwrap();
    command(&home, &tools, &outside)
        .arg("clean")
        .assert()
        .failure()
        .stderr(predicates::str::contains("escapes the isolated SLATE_HOME"));
    fs::remove_file(home.join(".config/nvim")).unwrap();
    assert_eq!(file_tree(&home), original);
    // Storage redirection is rejected before main creates a lock/sound cache.
    for root in [".config", ".cache"] {
        let saved = td.path().join("saved-storage");
        fs::rename(home.join(root), &saved).unwrap();
        symlink(&outside, home.join(root)).unwrap();
        command(&home, &tools, &outside)
            .arg("clean")
            .assert()
            .failure()
            .stderr(predicates::str::contains("escapes the isolated SLATE_HOME"));
        fs::remove_file(home.join(root)).unwrap();
        fs::rename(saved, home.join(root)).unwrap();
        assert_eq!(file_tree(&home), original);
        assert_eq!(file_tree(&outside), host_before);
    }
    // Snapshot read failure, after the target list has been inspected.
    let unreadable = env.managed_file("managed/keep.txt");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    // Root can read mode-000 fixtures, so inject a non-directory backup root on
    // that unusual test host instead; never assume permission denial as root.
    if unsafe { libc::geteuid() } != 0 {
        command(&home, &tools, &outside)
            .arg("clean")
            .assert()
            .failure()
            .stderr(predicates::str::contains("pre-clean snapshot failed"));
    } else {
        write(&env.slate_cache_dir().join("backups"), "blocked\n", 0o600);
        command(&home, &tools, &outside)
            .arg("clean")
            .assert()
            .failure();
        fs::remove_file(env.slate_cache_dir().join("backups")).unwrap();
    }
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(file_tree(&home), original);
    assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    assert_eq!(file_tree(&outside), host_before);
}

#[test]
fn clean_refuses_reversed_shell_markers_without_truncating_user_tail() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile");
    let outside = td.path().join("outside");
    let tools = td.path().join("spies");
    spies(&tools);
    fs::create_dir_all(&outside).unwrap();
    let env = SlateEnv::with_home(home.clone());
    write(
        &env.managed_file("config.toml"),
        "[preferences]\nsound = false\n",
        0o600,
    );
    let content = format!(
        "{}\n# before\n{}\nPRIVATE_USER_TAIL\n",
        slate_cli::adapter::marker_block::END,
        slate_cli::adapter::marker_block::START,
    );
    write(&env.zshrc_path(), &content, 0o640);
    let before = file_tree(&home);
    let output = command(&home, &tools, &outside)
        .arg("clean")
        .output()
        .unwrap();
    assert_eq!(
        fs::read(env.zshrc_path()).unwrap(),
        content.as_bytes(),
        "clean truncated the user tail"
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Marker block state corrupted"), "{error}");
    assert!(!error.contains("PRIVATE_USER_TAIL"));
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert!(error.contains(&points[0].id), "{error}");
    for (path, bytes) in before {
        assert_eq!(file_tree(&home).get(&path), Some(&bytes));
    }
    assert!(!outside.join("process-calls").exists());
}

#[test]
fn clean_preserves_user_code_after_lua_and_vim_managed_blocks() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile");
    let outside = td.path().join("outside");
    let tools = td.path().join("spies");
    spies(&tools);
    fs::create_dir_all(&outside).unwrap();
    let env = SlateEnv::with_home(home.clone());
    write(
        &env.managed_file("config.toml"),
        "[preferences]\nsound = false\n",
        0o600,
    );
    for (name, prefix, user_code) in [
        ("init.lua", "-- ", "vim.opt.number = true"),
        ("init.vim", "\" ", "set number"),
    ] {
        let content = format!(
            "{prefix}before\r\n{prefix}{}\r\nload_slate\r\n{prefix}{}\r\n{user_code}\r\n",
            slate_cli::adapter::marker_block::START,
            slate_cli::adapter::marker_block::END,
        );
        write(&env.nvim_config_dir().join(name), content, 0o640);
    }
    command(&home, &tools, &outside)
        .arg("clean")
        .assert()
        .success();
    for (name, prefix, user_code) in [
        ("init.lua", "-- ", "vim.opt.number = true"),
        ("init.vim", "\" ", "set number"),
    ] {
        let path = env.nvim_config_dir().join(name);
        assert_eq!(
            fs::read(&path).unwrap(),
            format!("{prefix}before\r\n{user_code}\r\n").as_bytes()
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert!(!outside.join("process-calls").exists());
}

#[test]
fn clean_partial_failure_reports_its_snapshot_and_legacy_manifests_still_restore() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile");
    let outside = td.path().join("outside");
    let tools = td.path().join("spies");
    spies(&tools);
    fs::create_dir_all(&outside).unwrap();
    let env = SlateEnv::with_home(home.clone());
    write(
        &env.managed_file("config.toml"),
        "[preferences]\nsound = false\n",
        0o600,
    );
    write(&env.managed_file("managed/keep.txt"), "managed\n", 0o600);
    let zsh = format!(
        "# user\n{}\nsource loader\n{}\n",
        slate_cli::adapter::marker_block::START,
        slate_cli::adapter::marker_block::END
    );
    write(&env.zshrc_path(), &zsh, 0o640);
    write(
        &home.join(".config/alacritty/alacritty.toml"),
        "malformed [",
        0o640,
    );
    let output = command(&home, &tools, &outside)
        .arg("clean")
        .assert()
        .failure()
        .stderr(predicates::str::contains("Some files may have changed"))
        .get_output()
        .clone();
    assert_eq!(fs::read_to_string(env.zshrc_path()).unwrap(), "# user\n");
    assert_eq!(
        fs::read_to_string(env.managed_file("managed/keep.txt")).unwrap(),
        "managed\n"
    );
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains(&format!("slate restore {} --dry-run", point.id)));
    assert!(!outside.join("process-calls").exists());
    let manifest_path = env
        .slate_cache_dir()
        .join("backups")
        .join(&point.id)
        .join("manifest.toml");
    let mut manifest: toml::Value = fs::read_to_string(&manifest_path).unwrap().parse().unwrap();
    for entry in manifest["entries"].as_array_mut().unwrap() {
        entry.as_table_mut().unwrap().remove("unix_mode");
    }
    fs::write(manifest_path, toml::to_string(&manifest).unwrap()).unwrap();
    assert!(point
        .entries
        .iter()
        .all(|entry| entry.original_path.starts_with(&home)));
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read_to_string(env.zshrc_path()).unwrap(), zsh);
}
