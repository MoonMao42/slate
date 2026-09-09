//! Real CLI imports and file restores in private profiles. Stub external tools;
//! never download fonts, rebuild host caches, reload host apps or run setup.
use slate_cli::config::{
    execute_restore_with_env, list_restore_points_with_env, preview_restore_with_env, RestorePoint,
};
use slate_cli::env::SlateEnv;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn write(path: &Path, bytes: impl AsRef<[u8]>, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("SLATE_HOME", home)
        .env("HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(10));
    command
}

// Keep file/link contents and modes; empty dirs and recovery history are not an
// import checkpoint's promise. The lock file deliberately remains after use.
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

fn checkpoint(env: &SlateEnv) -> RestorePoint {
    let points = list_restore_points_with_env(env).unwrap();
    let mut points = points.into_iter().filter(|p| p.theme_name == "pre-import");
    let point = points.next().expect("mandatory checkpoint");
    assert!(points.next().is_none());
    assert!(!point.reapplies_theme());
    assert!(!point.is_baseline);
    let dir = env.slate_cache_dir().join("backups").join(&point.id);
    assert_eq!(
        fs::metadata(dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for entry in &point.entries {
        if let Some(path) = &entry.backup_path {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    point
}

fn assert_round_trip(env: &SlateEnv, before: &BTreeMap<PathBuf, (u32, Vec<u8>)>) {
    let point = checkpoint(env);
    let after = files(env.home());
    let targets: BTreeSet<_> = point.entries.iter().map(|e| &e.original_path).collect();
    for path in before.keys().chain(after.keys()) {
        if before.get(path) != after.get(path) {
            assert!(
                targets.contains(path),
                "write missing from recovery contract: {}",
                path.display()
            );
        }
    }
    let preview = preview_restore_with_env(env, &point.id).unwrap();
    assert_eq!(preview.blocked_count(), 0);
    assert!(!preview.may_regenerate_theme_files);
    let report = execute_restore_with_env(env, &point.id).unwrap();
    assert!(report.is_fully_successful(), "{report:?}");
    assert_eq!(
        &files(env.home()),
        before,
        "bytes/modes/absent-file restoration"
    );
    let undo = execute_restore_with_env(env, &report.pre_restore_point_id).unwrap();
    assert!(undo.is_fully_successful());
    assert_eq!(
        files(env.home()),
        after,
        "undo restore must cover the same exact files"
    );
}

#[test]
fn opacity_import_is_recoverable_without_snapshotting_unrelated_dotfiles() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    write(
        &env.managed_file("config.toml"),
        "[tools]\nstarship = true\nzsh_highlighting = true\n",
        0o640,
    );
    write(&env.managed_file("current"), "nord\n", 0o600);
    write(&env.managed_file("autorun-fastfetch"), "", 0o600);
    write(
        &env.managed_file("managed/ghostty/opacity.conf"),
        "# exact old content\n",
        0o640,
    );
    write(
        &home.path().join("dotfile"),
        "# unrelated linked rc\n",
        0o600,
    );
    symlink(home.path().join("dotfile"), env.zshrc_path()).unwrap();
    let before = files(home.path());
    let output = command(home.path())
        .args(["--quiet", "import", "slate://none/none/frosted/none"])
        .assert()
        .success()
        .get_output()
        .clone();
    let point = checkpoint(&env);
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains(&format!("slate restore {} --dry-run", point.id)));
    assert!(point
        .entries
        .iter()
        .all(|e| e.original_path != env.zshrc_path()));
    assert_round_trip(&env, &before);
}

#[test]
fn whole_import_covers_adapter_outputs_and_recovers_success_or_later_failure() {
    for fail in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        for tool in [
            "ghostty",
            "alacritty",
            "kitty",
            "starship",
            "bat",
            "delta",
            "eza",
            "lazygit",
            "fastfetch",
            "tmux",
            "nvim",
            "opencode",
        ] {
            write(
                &home.path().join("bin").join(tool),
                if tool == "nvim" {
                    "#!/bin/sh\nprintf 'NVIM v0.8.0\\n'\n"
                } else {
                    "#!/bin/sh\nexit 0\n"
                },
                0o755,
            );
        }
        for tool in ["brew", "curl", "osascript", "killall", "pkill"] {
            write(
                &home.path().join("bin").join(tool),
                "#!/bin/sh\nprintf invoked > \"$HOME/UNEXPECTED_PROCESS\"\nexit 97\n",
                0o755,
            );
        }
        write(
            &env.managed_file("config.toml"),
            "[tools]\nstarship = true\nzsh_highlighting = true\n",
            0o640,
        );
        write(&env.managed_file("current"), "nord\n", 0o640);
        write(&env.managed_file("current-font"), "Old Mono\n", 0o640);
        write(&env.managed_file("current-opacity"), "solid\n", 0o600);
        write(
            &env.managed_file("managed/ghostty/theme.conf"),
            "# old custom colors\n",
            0o640,
        );
        write(
            &env.managed_file("user/keep.conf"),
            "keep my overrides\n",
            0o600,
        );
        write(
            &env.xdg_config_home().join("ghostty/config.ghostty"),
            "font-size = 13\n",
            0o640,
        );
        write(
            &env.xdg_config_home().join("alacritty/alacritty.toml"),
            "# original\n",
            0o640,
        );
        // Kitty config and bat theme files intentionally start absent.
        write(
            &env.xdg_config_home().join("starship.toml"),
            "# custom prompt\n",
            0o640,
        );
        write(
            &env.xdg_config_home().join("opencode/tui.json"),
            if fail {
                "invalid JSON"
            } else {
                "{\"scroll_speed\":3}"
            },
            0o640,
        );
        write(
            &home.path().join(".gitconfig"),
            "[user]\n\tname = Fixture\n",
            0o640,
        );
        write(&env.tmux_config_path(), "# user tmux\n", 0o640);
        write(
            &env.nvim_config_dir().join("init.lua"),
            "-- unchanged; no setup\n",
            0o640,
        );
        write(
            &home
                .path()
                .join(".zsh/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh"),
            "# detection fixture; not executed\n",
            0o640,
        );
        write(
            &slate_cli::platform::fonts::user_font_dir(&env)
                .join("FixtureMonoNerdFont-Regular.ttf"),
            "\0\x01\0\0private discovery fixture",
            0o600,
        );
        let before = files(home.path());
        let result = command(home.path())
            .args([
                "--quiet",
                "import",
                "slate://v1/catppuccin-mocha/FixtureMono%20Nerd%20Font/frosted/s,f",
            ])
            .assert();
        let output = if fail {
            result.failure()
        } else {
            result.success()
        }
        .get_output()
        .clone();
        let point = checkpoint(&env);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(&format!("slate restore {} --dry-run", point.id)));
        assert!(!home.path().join("UNEXPECTED_PROCESS").exists(), "{stderr}");
        assert!(env.managed_file("managed/kitty/font.conf").exists());
        assert!(env
            .xdg_config_home()
            .join("bat/themes/slate-nord.tmTheme")
            .exists());
        assert_ne!(
            fs::read(env.managed_file("current-font")).unwrap(),
            b"Old Mono\n"
        );
        if fail {
            assert!(stderr.contains("Import was incomplete"));
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains("Config imported successfully")
            );
            assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
        }
        assert_round_trip(&env, &before);
    }
}

#[test]
fn unsafe_or_oversized_checkpoint_sources_stop_before_settings_and_sound_io() {
    for kind in [
        "fifo-settings",
        "fifo-output",
        "link",
        "directory",
        "oversized",
        "backup-link",
        "parent-escape",
        "dangling-parent",
    ] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        write(&env.managed_file("current"), "nord\n", 0o640);
        write(
            &outside.path().join("sentinel"),
            "PRIVATE_CONTENT_DO_NOT_PRINT",
            0o640,
        );
        let mut target = env.managed_file("managed/ghostty/opacity.conf");
        if kind == "fifo-settings" {
            target = env.managed_file("config.toml");
        }
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match kind {
            "fifo-settings" | "fifo-output" => {
                let path = std::ffi::CString::new(target.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "link" => symlink(outside.path().join("sentinel"), &target).unwrap(),
            "directory" => fs::create_dir(&target).unwrap(),
            "oversized" => {
                fs::File::create(&target)
                    .unwrap()
                    .set_len(8 * 1024 * 1024 + 1)
                    .unwrap();
            }
            "backup-link" => {
                fs::create_dir_all(env.slate_cache_dir()).unwrap();
                symlink(outside.path(), env.slate_cache_dir().join("backups")).unwrap();
            }
            "parent-escape" | "dangling-parent" => {
                let dir = env.config_dir().join("managed/starship");
                symlink(
                    if kind == "parent-escape" {
                        outside.path().to_owned()
                    } else {
                        home.path().join("missing")
                    },
                    dir,
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = files(home.path());
        let outside_before = tree_snapshot::tree(outside.path());
        // Deliberately not --quiet: sound initialization used to read config
        // before the import handler could reject a FIFO.
        let output = command(home.path())
            .args(["import", "slate://none/none/frosted/none"])
            .assert()
            .failure()
            .get_output()
            .clone();
        let text = String::from_utf8_lossy(&output.stderr);
        assert!(text.contains("before applying settings"), "{kind}: {text}");
        assert!(!text.contains("PRIVATE_CONTENT_DO_NOT_PRINT"));
        assert_eq!(files(home.path()), before, "{kind}");
        assert_eq!(
            tree_snapshot::tree(outside.path()),
            outside_before,
            "{kind}"
        );
        if kind != "backup-link" {
            assert!(
                list_restore_points_with_env(&env).unwrap().is_empty(),
                "incomplete checkpoint retained: {kind}"
            );
        }
    }
}

#[test]
fn isolated_import_ignores_external_bat_overrides_and_recovers_local_assets() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    // Import targets every installed adapter: shadow all native probes with
    // private executables rather than falling back to host installations.
    for tool in [
        "ghostty",
        "alacritty",
        "kitty",
        "starship",
        "bat",
        "delta",
        "eza",
        "lazygit",
        "fastfetch",
        "tmux",
        "opencode",
    ] {
        write(
            &home.path().join("bin").join(tool),
            "#!/bin/sh\nexit 0\n",
            0o755,
        );
    }
    write(
        &home.path().join("bin/nvim"),
        "#!/bin/sh\nprintf 'NVIM v0.8.0\\n'\n",
        0o755,
    );
    let before = files(home.path());
    let outside_before = tree_snapshot::tree(outside.path());
    command(home.path())
        .env("BAT_CONFIG_DIR", outside.path())
        .env("BAT_CONFIG_PATH", outside.path().join("batrc"))
        .env("BAT_CACHE_PATH", outside.path().join("cache"))
        .args(["--quiet", "import", "slate://nord/none/none/none"])
        .assert()
        .success();
    assert!(env
        .bat_config_dir()
        .join("themes/slate-nord.tmTheme")
        .exists());
    let point = checkpoint(&env);
    assert!(
        point
            .entries
            .iter()
            .any(|entry| entry.original_path
                == env.bat_config_dir().join("themes/slate-nord.tmTheme"))
    );
    assert_round_trip(&env, &before);
    assert_eq!(tree_snapshot::tree(outside.path()), outside_before);
}
