//! Conditional choices and runtime validation in private HOME/SLATE_HOME only.
use serde_json::Value;
use slate_cli::{
    config::{list_restore_points_with_env, ConfigWriteGuard},
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

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn fixture(home: &Path) -> SlateEnv {
    for name in [
        "defaults",
        "gsettings",
        "pgrep",
        "nvim",
        "starship",
        "fastfetch",
        "osascript",
        "fc-list",
        "brew",
    ] {
        let path = home.join("bin").join(name);
        let body = if name == "defaults" {
            "printf x >> \"$HOME/appearance-reads\"\nprintf 'Dark\\n'\n"
        } else {
            "printf native > \"$HOME/UNEXPECTED\"\nexit 99\n"
        };
        write(&path, format!("#!/bin/sh\n{body}"));
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    SlateEnv::with_home(home.into())
}

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(8));
    command
}

fn inspect(home: &Path) -> Value {
    let output = command(home)
        .args(["config", "pairing", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_CONTENT"));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn auto_resolution_cli_explains_explicit_current_catalog_self_pair_and_defaults_without_desktop_queries(
) {
    for (pair, current, dark, light, dark_source, light_source, reason) in [
        (
            None,
            None,
            "catppuccin-mocha",
            "catppuccin-latte",
            "brand_default",
            "brand_default",
            Some("no_current_theme"),
        ),
        (
            None,
            Some("PRIVATE_CONTENT"),
            "catppuccin-mocha",
            "catppuccin-latte",
            "brand_default",
            "brand_default",
            Some("unknown_current_theme"),
        ),
        (
            None,
            Some("catppuccin-mocha"),
            "catppuccin-mocha",
            "catppuccin-latte",
            "current_theme",
            "catalog_pair",
            None,
        ),
        (
            None,
            Some("catppuccin-latte"),
            "catppuccin-mocha",
            "catppuccin-latte",
            "catalog_pair",
            "current_theme",
            None,
        ),
        (
            None,
            Some("nord"),
            "nord",
            "nord",
            "current_theme",
            "catalog_pair",
            None,
        ),
        (
            Some("dark_theme='gruvbox-dark'\nlight_theme='nord'\n"),
            Some("PRIVATE_CONTENT"),
            "gruvbox-dark",
            "nord",
            "configured",
            "configured",
            None,
        ),
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        if let Some(pair) = pair {
            write(&env.managed_file("auto.toml"), pair);
        }
        if let Some(current) = current {
            write(&env.managed_file("current"), current);
        }
        let before = snapshot::tree(td.path());
        let report = inspect(td.path());
        assert_eq!(report["schema_version"], 1);
        let choices = &report["resolution"];
        for (appearance, id, source) in
            [("dark", dark, dark_source), ("light", light, light_source)]
        {
            assert_eq!(choices[appearance]["status"], "resolved");
            assert_eq!(choices[appearance]["theme_id"], id);
            assert_eq!(choices[appearance]["source"], source);
            assert_eq!(choices[appearance]["requested_appearance"], appearance);
            if let Some(reason) = reason {
                assert_eq!(choices[appearance]["fallback_reason"], reason);
            }
        }
        if light == "nord" {
            assert_eq!(choices["light"]["theme_appearance"], "dark");
        }
        let text = command(td.path())
            .args(["config", "pairing"])
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8_lossy(&text.stdout);
        assert!(text.contains("Automatic choices (conditional; no desktop query):"));
        assert!(!text.contains("PRIVATE_CONTENT"));
        if light == "nord" {
            assert!(text.contains("Selected theme remains dark"));
        }
        assert_eq!(snapshot::tree(td.path()), before);
    }
}

#[test]
fn auto_resolution_cli_keeps_independent_explicit_choice_when_fallback_input_is_unreadable() {
    for kind in ["current-fifo", "unknown-pair", "malformed-pair"] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        let _guard = ConfigWriteGuard::acquire(&env).unwrap();
        write(
            &env.slate_cache_dir().join("preview-session.json"),
            "PRIVATE_CONTENT",
        );
        write(&env.managed_file("auto.toml"), "dark_theme='nord'\n");
        match kind {
            "current-fifo" => {
                let path = env.managed_file("current");
                let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "unknown-pair" => write(
                &env.managed_file("auto.toml"),
                "dark_theme='nord'\nlight_theme='PRIVATE_CONTENT'\n",
            ),
            "malformed-pair" => write(&env.managed_file("auto.toml"), "PRIVATE_CONTENT = ["),
            _ => unreachable!(),
        }
        let before = snapshot::tree(td.path());
        let report = inspect(td.path());
        let choices = &report["resolution"];
        assert_eq!(choices["light"]["status"], "error");
        assert!(choices["light"]["theme_id"].is_null());
        assert_eq!(
            choices["dark"]["status"],
            if kind == "malformed-pair" {
                "error"
            } else {
                "resolved"
            }
        );
        assert_eq!(snapshot::tree(td.path()), before);
    }
}

#[test]
fn auto_resolution_actual_auto_command_rejects_bad_pairing_before_theme_files_and_without_contents_in_errors(
) {
    for kind in ["unknown", "symlink", "malformed"] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        let pair = env.managed_file("auto.toml");
        write(&env.managed_file("current"), "catppuccin-mocha");
        match kind {
            "unknown" => write(
                &pair,
                "dark_theme='PRIVATE_CONTENT'\nlight_theme='PRIVATE_CONTENT'\n",
            ),
            "symlink" => {
                write(&td.path().join("target"), "dark_theme='nord'\n");
                symlink(td.path().join("target"), &pair).unwrap();
            }
            "malformed" => write(&pair, "PRIVATE_CONTENT = ["),
            _ => unreachable!(),
        }
        let before = snapshot::tree(env.config_dir());
        let output = command(td.path())
            .args(["theme", "--auto", "--quiet"])
            .assert()
            .failure()
            .get_output()
            .clone();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("PRIVATE_CONTENT"), "{error}");
        if kind == "unknown" {
            assert!(
                error.contains("auto-theme pairing is not in the current catalog"),
                "{error}"
            );
        }
        assert_eq!(snapshot::tree(env.config_dir()), before);
        assert!(list_restore_points_with_env(&env).unwrap().is_empty());
        assert!(!env.managed_file("managed").exists());
        assert!(!td.path().join("UNEXPECTED").exists());
        if cfg!(target_os = "macos") {
            assert_eq!(fs::read(td.path().join("appearance-reads")).unwrap(), b"x");
        }
    }
}
