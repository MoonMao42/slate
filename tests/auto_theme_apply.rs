//! CLI post-commit pairing contracts. All programs are private shell fixtures;
//! no real editor, bat cache, desktop settings or watcher is invoked.
use slate_cli::{config::ConfigManager, env::SlateEnv};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    time::Duration,
};

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn program(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    write(&path, format!("#!/bin/sh\n{body}\n"));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn profile(home: &Path) -> SlateEnv {
    // Shadow every default adapter's external program, including fallback
    // aliases, so no host executable can be selected by discovery.
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
        program(home, name, "exit 0");
    }
    program(home, "nvim", "printf 'NVIM v0.8.0\\n'");
    // First read is Dark; any later read would be Light, reproducing the
    // desktop flip between automatic resolution and post-commit pairing.
    program(home, "defaults", "if [ -f \"$SLATE_TEST_APPEARANCE_LOG\" ]; then printf 'Light\\n'; else printf 'Dark\\n'; fi\nprintf x >> \"$SLATE_TEST_APPEARANCE_LOG\"");
    let env = SlateEnv::with_home(home.to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    env
}

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .env("SLATE_TEST_APPEARANCE_LOG", home.join("appearance.log"))
        .timeout(Duration::from_secs(12));
    command
}

#[test]
fn auto_theme_apply_preserves_pair_bytes_and_does_not_redetect_after_commit() {
    for quiet in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let env = profile(td.path());
        let light = if cfg!(target_os = "macos") {
            "catppuccin-latte"
        } else {
            "catppuccin-mocha"
        };
        let pair = format!(
            "# saved pairs\ndark_theme = 'catppuccin-mocha'\nlight_theme = '{light}'\nextra = 7\n"
        );
        write(&env.managed_file("auto.toml"), &pair);
        let mut cmd = command(td.path());
        if quiet {
            cmd.arg("--quiet");
        }
        let output = cmd
            .args(["theme", "--auto"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert_eq!(
            fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
            pair
        );
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "catppuccin-mocha"
        );
        if cfg!(target_os = "macos") {
            assert_eq!(fs::read(td.path().join("appearance.log")).unwrap(), b"x");
        }
        assert!(!String::from_utf8_lossy(&output.stderr).contains("warning:"));
        if quiet {
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn auto_theme_apply_fallback_does_not_create_a_pair_file() {
    let td = tempfile::tempdir().unwrap();
    let env = profile(td.path());
    command(td.path())
        .args(["theme", "--auto", "--quiet"])
        .assert()
        .success();
    assert!(!env.managed_file("auto.toml").exists());
    assert!(env.slate_cache_dir().join("current_theme.lua").is_file());
}

#[test]
fn auto_theme_apply_manual_pair_failure_warns_once_even_when_quiet() {
    for quiet in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let env = profile(td.path());
        let malformed = "light_theme = 'PRIVATE_PAIR_CONTENT'\ndark_theme = [\n";
        write(&env.managed_file("auto.toml"), malformed);
        let mut cmd = command(td.path());
        if quiet {
            cmd.arg("--quiet");
        }
        let output = cmd
            .args(["theme", "catppuccin-mocha"])
            .assert()
            .success()
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.matches("warning: auto-theme:").count(),
            1,
            "{stderr}"
        );
        assert!(stderr.contains("already saved"), "{stderr}");
        assert!(!stderr.contains("PRIVATE_PAIR_CONTENT"));
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "catppuccin-mocha"
        );
        assert!(
            fs::read_to_string(env.slate_cache_dir().join("current_theme.lua"))
                .unwrap()
                .contains("catppuccin-mocha")
        );
        assert_eq!(
            fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
            malformed
        );
        if quiet {
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn auto_theme_apply_quiet_shared_editor_warning_survives_stderr_redirection() {
    let td = tempfile::tempdir().unwrap();
    let env = profile(td.path());
    // Not a ready selected adapter, so the existing shared hook is best effort.
    program(td.path(), "nvim", "printf 'NVIM v0.7.0\\n'");
    let pair = "dark_theme = 'catppuccin-mocha'\nlight_theme = 'catppuccin-mocha'\n";
    write(&env.managed_file("auto.toml"), pair);
    let target = td.path().join("untouched-editor-target");
    write(&target, "original\n");
    let state = env.slate_cache_dir().join("current_theme.lua");
    symlink(&target, &state).unwrap();
    let output = command(td.path())
        .args(["theme", "--auto", "--quiet"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("warning: nvim:").count(), 1, "{stderr}");
    assert!(stderr.contains("already saved"), "{stderr}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "catppuccin-mocha"
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
        pair
    );
    assert_eq!(fs::read_link(&state).unwrap(), target);
    assert_eq!(fs::read(target).unwrap(), b"original\n");
}

#[test]
fn auto_theme_apply_precommit_failure_never_learns_a_pair_or_notifies_editor() {
    let td = tempfile::tempdir().unwrap();
    let env = profile(td.path());
    write(
        &env.managed_file("config.toml"),
        "[auto_theme]\nenabled = true\n[tools]\nstarship = 'PRIVATE_BAD_FLAG'\n",
    );
    let pair = "dark_theme = 'nord'\nlight_theme = 'catppuccin-latte'\n";
    write(&env.managed_file("auto.toml"), pair);
    let state = env.slate_cache_dir().join("current_theme.lua");
    write(&state, "old editor state\n");
    let output = command(td.path())
        .args(["theme", "catppuccin-mocha", "--quiet"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("shared shell configuration"), "{stderr}");
    assert!(stderr.contains("No theme files were written"), "{stderr}");
    assert!(slate_cli::config::list_restore_points_with_env(&env)
        .unwrap()
        .is_empty());
    assert!(!stderr.contains("already saved"));
    assert!(!stderr.contains("PRIVATE_BAD_FLAG"));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "nord"
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
        pair
    );
    assert_eq!(fs::read(&state).unwrap(), b"old editor state\n");
    assert!(!td.path().join("appearance.log").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn appearance_query_cli_failure_stops_before_theme_files_or_native_adapters() {
    for (body, expected) in [
        (
            "printf PRIVATE_QUERY_FAILURE >&2\n/bin/sleep 10",
            "timed out",
        ),
        ("printf 'PRIVATE Dark failed\\n'", "unrecognized value"),
        ("printf PRIVATE_QUERY_FAILURE >&2\nexit 23", "failed"),
        (
            "while :; do printf PRIVATE_QUERY_FLOOD >&2; done",
            "combined output",
        ),
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = profile(td.path());
        let pair = "dark_theme = 'catppuccin-mocha'\nlight_theme = 'catppuccin-latte'\n";
        write(&env.managed_file("auto.toml"), pair);
        let state = env.slate_cache_dir().join("current_theme.lua");
        write(&state, "return 'nord'\n");
        program(td.path(), "defaults", body);
        program(
            td.path(),
            "bat",
            "printf x >> \"$HOME/native.calls\"\nexit 0",
        );
        program(
            td.path(),
            "nvim",
            "printf x >> \"$HOME/native.calls\"\nprintf 'NVIM v0.8.0\\n'",
        );
        let output = command(td.path())
            .args(["theme", "--auto", "--quiet"])
            .assert()
            .failure()
            .get_output()
            .clone();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("PRIVATE"), "{error}");
        assert!(output.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "nord"
        );
        assert_eq!(
            fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
            pair
        );
        assert_eq!(fs::read_to_string(&state).unwrap(), "return 'nord'\n");
        assert!(!env.managed_file("managed/shell/env.zsh").exists());
        assert!(!td.path().join("native.calls").exists());
        assert!(slate_cli::config::list_restore_points_with_env(&env)
            .unwrap()
            .is_empty());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn appearance_query_manual_failure_warns_without_changing_the_saved_pair() {
    let td = tempfile::tempdir().unwrap();
    let env = profile(td.path());
    let pair = "dark_theme = 'nord'\nlight_theme = 'catppuccin-latte'\n";
    write(&env.managed_file("auto.toml"), pair);
    program(
        td.path(),
        "defaults",
        "printf PRIVATE_QUERY_FAILURE >&2\nexit 23",
    );
    let output = command(td.path())
        .args(["theme", "catppuccin-mocha", "--quiet"])
        .assert()
        .success()
        .get_output()
        .clone();
    let error = String::from_utf8_lossy(&output.stderr);
    assert_eq!(error.matches("warning: auto-theme:").count(), 1, "{error}");
    assert!(error.contains("already saved"));
    assert!(error.contains("appearance query failed"));
    assert!(!error.contains("PRIVATE"));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "catppuccin-mocha"
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
        pair
    );
}
