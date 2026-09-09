//! Exercise the actual watcher apply callback inside a private-profile child.
//! No event source, real desktop query, native adapter or host watcher is used.
use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};

fn program(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
#[ignore = "private subprocess fixture, invoked only by its bounded parent"]
fn watcher_apply_private_child() {
    let env = SlateEnv::from_process().unwrap();
    assert!(env.session().is_isolated());
    let case = std::env::var("SLATE_WATCHER_APPLY_CASE").unwrap();
    assert!(["pair", "absent", "warning"].contains(&case.as_str()));
    for name in ["defaults", "nvim", "bat"] {
        assert_eq!(
            crate::detection::command_in_actual_path(name),
            Some(env.home().join("bin").join(name))
        );
    }
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    let pair = "# preserved automatic pairs\ndark_theme = 'catppuccin-mocha'\nlight_theme = 'catppuccin-latte'\nextra = 7\n";
    if case != "absent" {
        fs::write(env.managed_file("auto.toml"), pair).unwrap();
    }
    let editor_target = env.home().join("untouched-editor");
    let editor_state = env.slate_cache_dir().join("current_theme.lua");
    if case == "warning" {
        fs::write(&editor_target, "untouched\n").unwrap();
        fs::create_dir_all(editor_state.parent().unwrap()).unwrap();
        symlink(&editor_target, &editor_state).unwrap();
    }
    assert!(apply_auto_theme_quiet_with_env(&env).unwrap());
    assert_eq!(fs::read(env.home().join("appearance.calls")).unwrap(), b"x");
    if case == "absent" {
        assert!(!env.managed_file("auto.toml").exists());
        assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    } else {
        assert_eq!(
            fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
            pair
        );
        assert_eq!(
            config.get_current_theme().unwrap().as_deref(),
            Some("catppuccin-mocha")
        );
    }
    if case == "warning" {
        assert_eq!(fs::read_link(editor_state).unwrap(), editor_target);
        assert_eq!(fs::read(editor_target).unwrap(), b"untouched\n");
    }
    assert!(!env.slate_cache_dir().join("watchers").exists());
}

#[test]
fn watcher_apply_callback_preserves_pairs_and_logs_retained_warnings() {
    for case in ["pair", "absent", "warning"] {
        let td = tempfile::tempdir().unwrap();
        for name in [
            "ghostty",
            "alacritty",
            "kitty",
            "starship",
            "bat",
            "batcat",
            "delta",
            "eza",
            "lazygit",
            "fastfetch",
            "tmux",
            "opencode",
            "zsh",
            "gsettings",
            "gdbus",
            "brew",
        ] {
            program(td.path(), name, "exit 0");
        }
        program(td.path(), "nvim", "printf 'NVIM v0.7.0\\n'");
        program(td.path(), "defaults", "if [ -f \"$HOME/appearance.calls\" ]; then printf 'Light\\n'; else printf 'Dark\\n'; fi\nprintf x >> \"$HOME/appearance.calls\"");
        let output = assert_cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "platform::dark_mode_notify::apply_tests::watcher_apply_private_child",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", td.path().join("bin"))
            .env("NO_COLOR", "1")
            .env("SLATE_WATCHER_APPLY_CASE", case)
            .current_dir(td.path())
            .timeout(Duration::from_secs(12))
            .assert()
            .success()
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if case == "warning" {
            assert_eq!(stderr.matches("warning: nvim:").count(), 1, "{stderr}");
            assert!(stderr.contains("already saved"), "{stderr}");
        } else {
            assert!(!stderr.contains("warning:"), "{stderr}");
        }
    }
}
