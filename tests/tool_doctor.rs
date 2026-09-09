//! File-only diagnostics: fixture executables are traps, never native probes.
use slate_cli::{
    adapter::{BtopAdapter, ToolAdapter},
    config::ConfigWriteGuard,
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::Path,
    time::Duration,
};

#[path = "tool_doctor/eza.rs"]
mod eza;
#[path = "tool_doctor/fastfetch.rs"]
mod fastfetch;
#[path = "tool_doctor/lazygit.rs"]
mod lazygit;
#[path = "support/tree.rs"]
mod tree_snapshot;
#[path = "tool_doctor/workspaces.rs"]
mod workspaces;

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    write(&env.managed_file("current"), "nord\n");
    for tool in ["starship", "btop", "nvim", "yazi", "zellij"] {
        let binary = env.user_local_bin().join(tool);
        write(
            &binary,
            "#!/bin/sh\nprintf unexpected > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
        );
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (home, env)
}

fn command(env: &SlateEnv, isolated: bool) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("HOME", env.home())
        .env("PATH", env.user_local_bin())
        .env("NO_COLOR", "1")
        .current_dir(env.home())
        .timeout(Duration::from_secs(8));
    if isolated {
        cmd.env("SLATE_HOME", env.home());
    }
    cmd
}

fn json(cmd: &mut assert_cmd::Command, target: &str) -> serde_json::Value {
    let output = cmd
        .args(["doctor", target, "--json"])
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
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["target"], target);
    assert!(report["scope"]
        .as_str()
        .unwrap()
        .contains("no tool is launched"));
    report
}

fn check<'a>(report: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == code)
        .unwrap_or_else(|| panic!("missing {code}: {report}"))
}

fn absent(report: &serde_json::Value, code: &str) {
    assert!(
        !report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["code"] == code),
        "{report}"
    );
}

fn diagnose(env: &SlateEnv, target: &str) -> serde_json::Value {
    let before = tree_snapshot::tree(env.home());
    let report = json(&mut command(env, true), target);
    let text = command(env, true)
        .args(["doctor", target])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!String::from_utf8_lossy(&text).contains("PRIVATE_CONTENT"));
    assert_eq!(tree_snapshot::tree(env.home()), before);
    report
}

#[test]
fn btop_doctor_distinguishes_disk_connection_palette_and_asset_ownership() {
    let (_home, env) = fixture();
    let themes = ThemeRegistry::new().unwrap();
    BtopAdapter
        .apply_theme_with_env(themes.get("nord").unwrap(), &env)
        .unwrap();
    let path = BtopAdapter::config_path(&env);
    let asset = BtopAdapter::theme_path(&env);
    let meta = fs::metadata(&path).unwrap();
    let report = diagnose(&env, "btop");
    for code in [
        "availability",
        "theme_reference",
        "asset_ownership",
        "palette_match",
    ] {
        assert_eq!(check(&report, code)["status"], "ok");
    }
    assert_eq!(fs::metadata(&path).unwrap().ino(), meta.ino());
    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        meta.modified().unwrap()
    );
    assert!(check(&report, "reload")["suggestion"]
        .as_str()
        .unwrap()
        .contains("old theme"));
    for (text, expected) in [
        (
            format!("# color_theme = \"{}\"\n", asset.display()),
            "warning",
        ),
        ("color_theme = \"slate-sync\"\n".into(), "warning"),
        (
            format!(
                "color_theme = \"{}\"\r\n# PRIVATE_CONTENT\r\n",
                asset.display()
            ),
            "ok",
        ),
        (
            format!(
                "color_theme = \"{}\"\ncolor_theme = \"PRIVATE_CONTENT\"\n",
                asset.display()
            ),
            "error",
        ),
        ("color_theme = \"PRIVATE_CONTENT\n".into(), "error"),
    ] {
        write(&path, text);
        assert_eq!(
            check(&diagnose(&env, "btop"), "theme_reference")["status"],
            expected
        );
    }
    write(&env.managed_file("current"), "catppuccin-latte");
    assert_eq!(
        check(&diagnose(&env, "btop"), "palette_match")["status"],
        "warning"
    );
    write(&asset, "# PRIVATE_CONTENT\ntheme[main_bg]=\"#123456\"\n");
    let report = diagnose(&env, "btop");
    assert_eq!(check(&report, "asset_ownership")["status"], "warning");
    absent(&report, "palette_match");
}

#[test]
fn starship_doctor_matches_generated_styles_and_plain_profiles_without_execution() {
    let (_home, env) = fixture();
    for theme in ["nord", "catppuccin-latte"] {
        write(&env.managed_file("current"), theme);
        for style in slate_cli::config::prompt::PromptStyle::ALL.map(|style| style.id()) {
            write(
                &env.managed_file("config.toml"),
                "[tools]\nstarship=false\n",
            );
            command(&env, true)
                .args(["prompt", style, "--yes"])
                .assert()
                .success();
            let report = diagnose(&env, "starship");
            for code in [
                "availability",
                "palette_selection",
                "palette_match",
                "layout_match",
            ] {
                assert_eq!(
                    check(&report, code)["status"],
                    "ok",
                    "{theme} {style}: {report}"
                );
            }
            assert_eq!(check(&report, "activation_preference")["status"], "warning");
            let before = tree_snapshot::tree(env.home());
            let report = json(
                command(&env, false).env(
                    "STARSHIP_CONFIG",
                    env.managed_file("managed/starship/plain.toml"),
                ),
                "starship",
            );
            check(&report, "plain_fallback");
            assert_eq!(check(&report, "palette_match")["status"], "ok");
            assert_eq!(
                check(&report, "layout_match")["status"],
                "ok",
                "{theme} {style}: {report}"
            );
            assert_eq!(tree_snapshot::tree(env.home()), before);
        }
    }
}

#[test]
fn starship_doctor_explains_overrides_custom_layout_and_mismatched_palette() {
    let (_home, env) = fixture();
    command(&env, true)
        .args(["prompt", "compact", "--yes"])
        .assert()
        .success();
    let primary = env.xdg_config_home().join("starship.toml");
    let generated = fs::read_to_string(&primary).unwrap();
    let mut personal: toml::Value = generated.parse().unwrap();
    personal["format"] = toml::Value::String("$custom".into());
    personal["palettes"]["slate"]["blue"] = toml::Value::String("#123456".into());
    write(&primary, toml::to_string(&personal).unwrap());
    let report = diagnose(&env, "starship");
    assert_eq!(check(&report, "palette_match")["status"], "warning");
    assert_eq!(check(&report, "layout_match")["status"], "warning");
    let custom = env.home().join("custom.toml");
    write(&custom, "palette = 'PRIVATE_CONTENT'\n");
    let before = tree_snapshot::tree(env.home());
    let report = json(
        command(&env, false).env("STARSHIP_CONFIG", &custom),
        "starship",
    );
    check(&report, "custom_override");
    assert_eq!(
        check(&report, "selected_config")["path"],
        custom.to_str().unwrap()
    );
    assert_eq!(check(&report, "palette_selection")["status"], "warning");
    let report = json(
        command(&env, false).env("STARSHIP_CONFIG", "custom.toml"),
        "starship",
    );
    check(&report, "relative_override");
    absent(&report, "config_file");
    let report = json(
        command(&env, true).env("STARSHIP_CONFIG", &custom),
        "starship",
    );
    assert_eq!(
        check(&report, "selected_config")["path"],
        primary.to_str().unwrap()
    );
    absent(&report, "custom_override");
    assert_eq!(tree_snapshot::tree(env.home()), before);
    let xdg = env.home().join("xdg");
    write(&xdg.join("starship.toml"), &generated);
    write(&xdg.join("slate/current"), "catppuccin-latte");
    let before = tree_snapshot::tree(env.home());
    let report = json(
        command(&env, false)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("STARSHIP_CONFIG", ""),
        "starship",
    );
    assert_eq!(
        check(&report, "selected_config")["path"],
        xdg.join("starship.toml").to_str().unwrap()
    );
    assert_eq!(check(&report, "palette_match")["status"], "warning");
    absent(&report, "layout_match");
    assert_eq!(tree_snapshot::tree(env.home()), before);
}

#[test]
fn tool_doctor_reports_unsafe_files_as_unknown_not_disconnected() {
    for target in ["btop", "starship", "yazi", "zellij"] {
        for case in [
            "utf8",
            "fifo",
            "symlink",
            "oversize",
            "outside-parent",
            "invalid",
        ] {
            let (_home, env) = fixture();
            let path = match target {
                "btop" => BtopAdapter::config_path(&env),
                "yazi" => slate_cli::adapter::YaziAdapter::config_path(&env),
                "zellij" => slate_cli::adapter::ZellijAdapter::config_path(&env).unwrap(),
                _ => env.xdg_config_home().join("starship.toml"),
            };
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let outside = tempfile::tempdir().unwrap();
            match case {
                "utf8" => write(&path, b"PRIVATE_CONTENT\xff"),
                "fifo" => {
                    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                }
                "symlink" => {
                    let source = env.home().join("linked");
                    write(&source, "PRIVATE_CONTENT");
                    symlink(source, &path).unwrap();
                }
                "oversize" => fs::File::create(&path)
                    .unwrap()
                    .set_len(8 * 1024 * 1024 + 1)
                    .unwrap(),
                "outside-parent" => {
                    // Redirect only a disposable fixture subtree, not host configuration.
                    let parent = path.parent().unwrap();
                    let backup = env.home().join("original-config-parent");
                    fs::rename(parent, backup).unwrap();
                    write(
                        &outside.path().join(path.file_name().unwrap()),
                        "PRIVATE_CONTENT",
                    );
                    symlink(outside.path(), parent).unwrap();
                }
                "invalid" => write(&path, "[PRIVATE_CONTENT\n"),
                _ => unreachable!(),
            }
            let outside_before = tree_snapshot::tree(outside.path());
            let report = diagnose(&env, target);
            let code = if case != "invalid" {
                "config_file"
            } else if target == "btop" {
                "theme_reference"
            } else {
                "config_syntax"
            };
            assert_eq!(check(&report, code)["status"], "error", "{case}: {report}");
            if target == "btop" && case != "invalid" {
                absent(&report, "theme_reference");
            }
            if target == "starship" {
                absent(&report, "palette_selection");
            }
            if target == "yazi" {
                absent(&report, "flavor_dark");
            }
            if target == "zellij" {
                absent(&report, "theme_static");
            }
            assert_eq!(tree_snapshot::tree(outside.path()), outside_before);
        }
    }
}

#[test]
fn tool_doctor_keeps_checks_available_during_recovery_and_invalid_preferences() {
    let (_home, env) = fixture();
    command(&env, true)
        .args(["prompt", "minimal", "--yes"])
        .assert()
        .success();
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    // Recovery storage is not a prerequisite for a read-only file observation.
    fs::rename(
        env.slate_cache_dir().join("backups"),
        env.home().join("fixture-backups"),
    )
    .unwrap();
    write(&env.slate_cache_dir().join("backups"), "PRIVATE_CONTENT");
    for (content, code) in [
        ("[PRIVATE_CONTENT\n", "settings_syntax"),
        ("tools='PRIVATE_CONTENT'\n", "activation_preference"),
        (
            "[tools]\nstarship='PRIVATE_CONTENT'\n",
            "activation_preference",
        ),
        ("prompt='PRIVATE_CONTENT'\n", "saved_layout"),
        ("[prompt]\nstyle='PRIVATE_CONTENT'\n", "saved_layout"),
    ] {
        write(&env.managed_file("config.toml"), content);
        let report = diagnose(&env, "starship");
        assert_eq!(check(&report, code)["status"], "error");
        assert_eq!(check(&report, "palette_match")["status"], "ok");
    }
    write(&env.managed_file("current"), "PRIVATE_CONTENT");
    for target in ["btop", "starship", "yazi", "zellij"] {
        let report = diagnose(&env, target);
        assert_eq!(check(&report, "saved_theme")["status"], "warning");
        absent(&report, "palette_match");
        absent(&report, "flavor_match");
        absent(&report, "syntax_match");
        let before = tree_snapshot::tree(env.home());
        command(&env, true)
            .args(["doctor", target, "--check-version"])
            .assert()
            .failure();
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
}

#[test]
fn tool_doctor_bounds_preferences_theme_records_and_assets() {
    for (file, code, limit, target) in [
        (".config/slate/current", "theme_file", 4 * 1024, "btop"),
        (
            ".config/slate/config.toml",
            "settings_file",
            256 * 1024,
            "starship",
        ),
        (
            ".config/btop/themes/slate-sync.theme",
            "asset_file",
            8 * 1024 * 1024,
            "btop",
        ),
        (
            ".config/yazi/flavors/slate-sync.yazi/flavor.toml",
            "flavor_file",
            8 * 1024 * 1024,
            "yazi",
        ),
        (
            ".config/yazi/flavors/slate-sync.yazi/tmtheme.xml",
            "syntax_file",
            8 * 1024 * 1024,
            "yazi",
        ),
        (
            ".config/zellij/themes/slate-sync.kdl",
            "asset_file",
            8 * 1024 * 1024,
            "zellij",
        ),
    ] {
        for case in ["utf8", "fifo", "symlink", "oversize"] {
            let (_home, env) = fixture();
            let path = env.home().join(file);
            // Only fixture-owned records may be removed to make a special-file case.
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match case {
                "utf8" => write(&path, b"PRIVATE_CONTENT\xff"),
                "fifo" => {
                    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                }
                "symlink" => {
                    let source = env.home().join("linked");
                    write(&source, "PRIVATE_CONTENT");
                    symlink(source, &path).unwrap();
                }
                "oversize" => fs::File::create(&path).unwrap().set_len(limit + 1).unwrap(),
                _ => unreachable!(),
            }
            let report = diagnose(&env, target);
            assert_eq!(check(&report, code)["status"], "error");
            if code == "theme_file" {
                absent(&report, "saved_theme");
            }
            if code == "settings_file" {
                absent(&report, "activation_preference");
            }
            if code == "asset_file" {
                absent(&report, "asset_ownership");
            }
            absent(&report, "palette_match");
        }
    }
}

#[test]
fn tool_doctor_empty_profile_does_not_invent_activation_or_create_files() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    for target in ["btop", "starship"] {
        let report = diagnose(&env, target);
        assert_eq!(check(&report, "availability")["status"], "warning");
        assert_eq!(check(&report, "saved_theme")["status"], "warning");
        assert_eq!(check(&report, "config_file")["status"], "info");
        absent(&report, "palette_match");
        absent(&report, "layout_match");
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    }
}
