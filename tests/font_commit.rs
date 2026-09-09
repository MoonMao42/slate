//! Private CLI profiles and filename/header fixtures only; no native fonts,
//! catalog installs, font caches, global binaries, or running apps are changed.
use slate_cli::{
    config::{execute_restore_with_env, list_restore_points_with_env, preview_restore_with_env},
    env::SlateEnv,
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "support/tree.rs"]
mod tree_snapshot;

const FAMILY: &str = "SlateCommitFixture Nerd Font";

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    write(
        &slate_cli::platform::fonts::user_font_dir(&env)
            .join("SlateCommitFixtureNerdFont-Regular.ttf"),
        b"\0\x01\0\0private discovery fixture",
    );
    write(&env.managed_file("current-font"), "Old Mono\n");
    write(&env.managed_file("current"), "nord\n");
    write(
        &env.managed_file("config.toml"),
        "[tools]\nstarship = true\n[preferences]\nsound = false\n",
    );
    write(
        &env.managed_file("managed/ghostty/theme.conf"),
        "# untouched theme\n",
    );
    write(
        &env.xdg_config_home().join("ghostty/config.ghostty"),
        "font-size = 13\n",
    );
    write(
        &env.xdg_config_home().join("alacritty/alacritty.toml"),
        "[font.normal]\nfamily = 'Old Mono'\nstyle = 'Italic'\n",
    );
    // Kitty intentionally absent: font selection must not opt it into setup.
    for name in ["curl", "brew", "fc-cache", "osascript", "ghostty", "pkill"] {
        let path = temp.path().join("bin").join(name);
        write(
            &path,
            "#!/bin/sh\nprintf unexpected > \"$HOME/native-command-ran\"\nexit 90\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (temp, env)
}

fn command(env: &SlateEnv) -> assert_cmd::Command {
    command_with_flags(env, &["--quiet"])
}

fn command_with_flags(env: &SlateEnv, flags: &[&str]) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("HOME", env.home())
        .env("SLATE_HOME", env.home())
        .env("PATH", env.home().join("bin"))
        .env("NO_COLOR", "1")
        .args(flags)
        .timeout(Duration::from_secs(10));
    cmd
}

fn files(env: &SlateEnv) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    tree_snapshot::tree(env.home())
        .into_iter()
        .filter(|(path, _)| {
            !fs::symlink_metadata(path).unwrap().is_dir()
                && !path.starts_with(env.slate_cache_dir().join("backups"))
                && *path != env.slate_cache_dir().join("preview-session.lock")
        })
        .collect()
}

#[test]
fn font_flow_cli_feedback_honors_quiet_and_auto_without_changing_publication() {
    for flags in [&[][..], &["--quiet"][..], &["--auto"][..]] {
        let (_temp, env) = fixture();
        let output = command_with_flags(&env, flags)
            .args(["font", FAMILY])
            .assert()
            .success()
            .get_output()
            .clone();
        let stdout = String::from_utf8(output.stdout).unwrap();
        if flags.contains(&"--quiet") {
            assert!(stdout.is_empty(), "{stdout}");
        } else {
            assert!(
                stdout.contains("Updated font to") && stdout.contains("Terminal font refs:"),
                "{stdout}"
            );
            assert_eq!(
                stdout.contains("your new palette lives there"),
                !flags.contains(&"--auto")
            );
        }
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("Pre-font recovery point:") && stderr.contains("slate restore"),
            "{stderr}"
        );
        assert_eq!(
            fs::read_to_string(env.managed_file("current-font")).unwrap(),
            FAMILY
        );
        assert!(!env.home().join("native-command-ran").exists());
    }
    let (_temp, env) = fixture();
    write(
        &env.xdg_config_home().join("alacritty/alacritty.toml"),
        "[invalid",
    );
    let output = command(&env)
        .args(["font", FAMILY])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty() && !output.stderr.is_empty());
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"Old Mono\n"
    );
}

#[test]
fn font_commit_cli_checkpoint_round_trip_and_repeat_noop() {
    let (_temp, env) = fixture();
    let before = files(&env);
    let output = command(&env)
        .args(["font", FAMILY])
        .assert()
        .success()
        .get_output()
        .clone();
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    let point = &points[0];
    assert_eq!(point.theme_name, "pre-font");
    assert!(!point.reapplies_theme() && !point.is_baseline);
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains(&format!("slate restore {} --dry-run", point.id)));
    assert!(!env.xdg_config_home().join("kitty/kitty.conf").exists());
    assert!(!env.home().join("native-command-ran").exists());
    let after = files(&env);
    for path in before.keys().chain(after.keys()) {
        let included = point
            .entries
            .iter()
            .any(|entry| entry.original_path == *path);
        if before.get(path) != after.get(path) {
            assert!(included, "missing recovery: {}", path.display());
        }
        if [
            env.managed_file("current"),
            env.managed_file("config.toml"),
            env.managed_file("managed/ghostty/theme.conf"),
        ]
        .contains(path)
        {
            assert!(!included);
        }
    }
    let saved = env.managed_file("current-font");
    let meta = fs::metadata(&saved).unwrap();
    command(&env).args(["font", FAMILY]).assert().success();
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 1);
    assert_eq!(fs::metadata(saved).unwrap().ino(), meta.ino());
    assert_eq!(files(&env), after);
    command(&env)
        .args(["restore", &point.id, "--dry-run"])
        .assert()
        .success();
    let preview = preview_restore_with_env(&env, &point.id).unwrap();
    assert_eq!(preview.blocked_count(), 0);
    assert!(!preview.may_regenerate_theme_files);
    let restored = execute_restore_with_env(&env, &point.id).unwrap();
    assert!(restored.is_fully_successful(), "{restored:?}");
    assert_eq!(
        files(&env),
        before,
        "restore bytes, permissions and absent files; preserve font fixture"
    );
}

#[test]
fn font_commit_cli_preflight_rejects_malformed_unsafe_or_oversized_files_without_snapshot() {
    for issue in ["toml", "symlink", "fifo", "large", "settings", "backup"] {
        let (_temp, env) = fixture();
        match issue {
            "toml" => write(
                &env.xdg_config_home().join("alacritty/alacritty.toml"),
                "[font.normal]\nfamily = [unterminated",
            ),
            "settings" => write(
                &env.managed_file("config.toml"),
                "[tools]\nstarship = 'not a bool'",
            ),
            "backup" => {
                write(&env.slate_cache_dir().join("backups"), "not a directory");
            }
            _ => {
                let path = env.managed_file("managed/shell/env.fish");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                match issue {
                    "symlink" => {
                        write(&env.home().join("foreign"), "keep");
                        symlink(env.home().join("foreign"), path).unwrap();
                    }
                    "fifo" => {
                        let path =
                            std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                    }
                    "large" => fs::File::create(path)
                        .unwrap()
                        .set_len(8 * 1024 * 1024 + 1)
                        .unwrap(),
                    _ => unreachable!(),
                }
            }
        }
        // The tree helper records special-file metadata, never reads a FIFO.
        let before = files(&env);
        let output = command(&env)
            .args(["font", FAMILY])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("Pre-font recovery point:"),
            "{issue}"
        );
        if issue != "backup" {
            assert!(list_restore_points_with_env(&env).unwrap().is_empty());
        }
        assert_eq!(files(&env), before, "{issue}");
        assert!(!env.home().join("native-command-ran").exists());
    }
}

#[test]
fn font_commit_import_reuses_one_pre_import_point_and_restores_font_changes() {
    let (_temp, env) = fixture();
    let before = files(&env);
    let output = command(&env)
        .args([
            "import",
            "slate://v1/none/SlateCommitFixture%20Nerd%20Font/none/none",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Pre-font recovery point:"));
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].theme_name, "pre-import");
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        FAMILY.as_bytes()
    );
    let restored = execute_restore_with_env(&env, &points[0].id).unwrap();
    assert!(restored.is_fully_successful());
    assert_eq!(files(&env), before);
}

#[test]
fn font_literal_cli_preview_reconnects_after_reset_and_preserves_kitty_continuation() {
    let (_temp, env) = fixture();
    let ghostty = env.xdg_config_home().join("ghostty/config.ghostty");
    let managed = env.managed_file("managed/ghostty/font.conf");
    let original = format!(
        "\u{feff}config-file = \"{}\"\nconfig-file =\nconfig-file = /outside # {}\n",
        managed.display(),
        managed.display()
    );
    write(&ghostty, &original);
    let kitty = env.xdg_config_home().join("kitty/kitty.conf");
    let continued = format!(
        "include {}/\n\\font.conf\n",
        env.managed_file("managed/kitty").display()
    );
    write(&kitty, &continued);
    let before = files(&env);
    let preview = command(&env)
        .args(["font", FAMILY, "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
    let actions = preview["files"].as_array().unwrap();
    assert_eq!(
        actions
            .iter()
            .find(|entry| entry["path"] == ghostty.to_str().unwrap())
            .unwrap()["action"],
        "update"
    );
    assert_eq!(
        actions
            .iter()
            .find(|entry| entry["path"] == kitty.to_str().unwrap())
            .unwrap()["action"],
        "unchanged"
    );
    assert_eq!(files(&env), before);
    let output = command_with_flags(&env, &[])
        .args(["font", FAMILY])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&output);
    assert!(
        text.contains("observed for Ghostty, Alacritty, Kitty"),
        "{text}"
    );
    assert!(text.contains("Effective font not verified."));
    assert_eq!(
        fs::read_to_string(&ghostty).unwrap(),
        format!("{original}config-file = \"{}\"\n", managed.display())
    );
    assert_eq!(fs::read_to_string(&kitty).unwrap(), continued);
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert!(execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(files(&env), before);
}
