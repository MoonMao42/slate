//! Exercise only isolated profiles; native-looking tools and installers are traps.
use slate_cli::{adapter::BtopAdapter, config::list_restore_points_with_env, env::SlateEnv};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[path = "support/tree.rs"]
mod tree_snapshot;

fn seed(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(&env.managed_file("current"), "nord\n");
    for tool in ["btop", "nvim", "starship", "brew", "apt-get"] {
        let path = env.user_local_bin().join(tool);
        seed(
            &path,
            "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (home, env)
}

fn command(env: &SlateEnv) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("SLATE_HOME", env.home())
        .env("PATH", env.user_local_bin())
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(8));
    command
}

#[test]
fn tmux_cli_sync_and_restore_preserve_personal_configuration_without_starting_a_server() {
    for existing_palette in [false, true] {
        let (home, env) = fixture();
        let binary = env.user_local_bin().join("tmux");
        seed(
            &binary,
            "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
        );
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let config = env.tmux_config_path();
        let personal = "# private layout\nset -g prefix C-a\nbind-key r display-message personal\nset -g status-left PRIVATE\n";
        seed(&config, personal);
        fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();
        let palette = env.managed_file("managed/tmux/colors.conf");
        if existing_palette {
            seed(&palette, "# previous palette\n");
            fs::set_permissions(&palette, fs::Permissions::from_mode(0o600)).unwrap();
        }
        seed(&env.zshrc_path(), "# PRIVATE_STARTUP\n");
        let before = tree_snapshot::tree(home.path());
        let preview = command(&env)
            .args(["tools", "sync", "tmux", "--dry-run", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
        let paths = preview["configuration_paths"].as_array().unwrap();
        assert!(paths.contains(&serde_json::json!(config)));
        assert!(paths.contains(&serde_json::json!(palette)));
        assert_eq!(tree_snapshot::tree(home.path()), before);
        command(&env)
            .args(["tools", "sync", "tmux", "--yes"])
            .assert()
            .success();
        assert_eq!(
            slate_cli::adapter::marker_block::strip_managed_blocks(
                &fs::read_to_string(&config).unwrap()
            ),
            personal
        );
        assert!(fs::read_to_string(&palette)
            .unwrap()
            .contains("window-status-current-style"));
        assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
        let points = list_restore_points_with_env(&env).unwrap();
        assert_eq!(points.len(), 1);
        assert!(
            slate_cli::config::execute_restore_with_env(&env, &points[0].id)
                .unwrap()
                .is_fully_successful()
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), personal);
        assert_eq!(
            fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o640
        );
        if existing_palette {
            assert_eq!(
                fs::read_to_string(&palette).unwrap(),
                "# previous palette\n"
            );
            assert_eq!(
                fs::metadata(&palette).unwrap().permissions().mode() & 0o777,
                0o600
            );
        } else {
            assert!(!palette.exists());
        }
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "nord\n"
        );
        assert_eq!(
            fs::read_to_string(env.zshrc_path()).unwrap(),
            "# PRIVATE_STARTUP\n"
        );
        assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    }
}

#[test]
fn delta_cli_sync_previews_two_paths_and_preserves_personal_settings_without_probes() {
    for existing in [false, true] {
        verify_delta_cli_sync_and_restore(existing);
    }
}

fn verify_delta_cli_sync_and_restore(existing: bool) {
    let (home, env) = fixture();
    let binary = env.user_local_bin().join("delta");
    seed(
        &binary,
        "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let gitconfig = home.path().join(".gitconfig");
    let personal = "[core]\n pager = custom\n[user]\n name = Fixture\n";
    seed(&gitconfig, personal);
    fs::set_permissions(&gitconfig, fs::Permissions::from_mode(0o640)).unwrap();
    let managed = env.managed_file("managed/delta/colors");
    let previous = "[delta]\n syntax-theme = previous\n line-numbers = false\n";
    if existing {
        seed(&managed, previous);
        fs::set_permissions(&managed, fs::Permissions::from_mode(0o600)).unwrap();
    }
    seed(&env.zshrc_path(), "# PRIVATE_STARTUP\n");
    let before = tree_snapshot::tree(home.path());
    let details = command(&env)
        .args(["tools", "info", "delta", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let details = String::from_utf8(details).unwrap();
    assert!(details.contains("preserves pager selection"));
    assert!(details.contains("git config --show-origin --get core.pager"));
    assert!(details.contains("An unset value is not proof of the effective pager"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let output = command(&env)
        .args(["tools", "sync", "delta", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let paths = plan["configuration_paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&serde_json::json!(gitconfig)));
    assert!(paths.contains(&serde_json::json!(managed)));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let applied = command(&env)
        .args(["tools", "sync", "delta", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let applied = String::from_utf8(applied).unwrap();
    assert!(applied.contains("sync preserved pager selection and did not activate Delta"));
    assert!(applied.contains("No shell restart is required for the saved colors"));
    assert!(!applied.contains("Sync did not regenerate shell startup"));
    let content = fs::read_to_string(&gitconfig).unwrap();
    assert_eq!(
        slate_cli::adapter::marker_block::strip_managed_blocks(&content),
        personal
    );
    let themes = slate_cli::theme::ThemeRegistry::new().unwrap();
    let syntax = &themes.get("nord").unwrap().tool_refs["delta"];
    assert!(fs::read_to_string(&managed)
        .unwrap()
        .contains(&format!("syntax-theme = \"{syntax}\"")));
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "nord\n"
    );
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].entries.len(), 2);
    let after_sync = tree_snapshot::tree(home.path());
    command(&env)
        .args(["restore", &points[0].id, "--dry-run", "--json"])
        .assert()
        .success();
    assert_eq!(tree_snapshot::tree(home.path()), after_sync);
    assert!(
        slate_cli::config::execute_restore_with_env(&env, &points[0].id)
            .unwrap()
            .is_fully_successful()
    );
    assert_eq!(fs::read_to_string(&gitconfig).unwrap(), personal);
    assert_eq!(
        fs::metadata(&gitconfig).unwrap().permissions().mode() & 0o777,
        0o640
    );
    if existing {
        assert_eq!(fs::read_to_string(&managed).unwrap(), previous);
        assert_eq!(
            fs::metadata(&managed).unwrap().permissions().mode() & 0o777,
            0o600
        );
    } else {
        assert!(!managed.exists());
    }
    assert_eq!(
        fs::read_to_string(env.zshrc_path()).unwrap(),
        "# PRIVATE_STARTUP\n"
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "nord\n"
    );
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
}

#[test]
fn managed_color_sync_has_one_file_recovery_without_shell_or_personal_changes() {
    for id in ["lazygit", "eza", "fastfetch", "zsh-syntax-highlighting"] {
        verify_managed_color_sync(id);
    }
}

fn verify_managed_color_sync(id: &str) {
    let (home, env) = fixture();
    let binary = env.user_local_bin().join(id);
    seed(
        &binary,
        "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    if id == "zsh-syntax-highlighting" {
        seed(
            &env.home()
                .join(".zsh/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh"),
            "printf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\n",
        );
    }
    let managed = match id {
        "eza" => slate_cli::adapter::EzaAdapter::theme_path(&env),
        "fastfetch" => slate_cli::adapter::FastfetchAdapter::theme_path(&env),
        "zsh-syntax-highlighting" => slate_cli::adapter::ZshHighlightAdapter::theme_path(&env),
        _ => slate_cli::adapter::LazygitAdapter::theme_path(&env),
    };
    let personal = env.xdg_config_home().join(match id {
        "eza" => "eza/theme.yml",
        "fastfetch" => "fastfetch/config.jsonc",
        "zsh-syntax-highlighting" => "zsh/personal-highlights.zsh",
        _ => "lazygit/config.yml",
    });
    let shell = env.managed_file("managed/shell/env.zsh");
    seed(&env.zshrc_path(), "# PRIVATE_STARTUP\n");
    seed(
        &managed,
        "gui:\n  theme:\n    activeBorderColor: '#89b4fa'\n",
    );
    fs::set_permissions(&managed, fs::Permissions::from_mode(0o640)).unwrap();
    let original = fs::read(&managed).unwrap();
    seed(&personal, "# PRIVATE_CONFIG\ngui:\n  scrollHeight: 7\n");
    seed(&shell, "# PRIVATE_SHELL\n");
    let before = tree_snapshot::tree(home.path());
    let preview = command(&env)
        .args(["tools", "sync", id, "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
    assert_eq!(preview["configuration_paths"], serde_json::json!([managed]));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let output = command(&env)
        .args(["tools", "sync", id, "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    if id == "zsh-syntax-highlighting" {
        assert!(output.contains(&format!("Adapter details: slate tools info {id}")));
    } else {
        assert!(output.contains(&format!("Read-only check: slate doctor {id}")));
    }
    assert!(output.contains("live appearance is not verified"));
    assert!(output.contains("Sync did not regenerate shell startup"));
    match id {
        "fastfetch" => {
            assert!(output.contains("Run fastfetch manually"));
            assert!(output.contains("startup autorun is not required or enabled by this sync"));
            assert!(output.contains("An explicit --config bypasses Slate's preset"));
            assert!(output.contains("personal layouts are not merged"));
        }
        "zsh-syntax-highlighting" => {
            assert!(output.contains("plugin loaded before Slate's color snippet"));
            assert!(output.contains("does not install or load the plugin"));
            assert!(output.contains("Later style assignments can override them"));
        }
        _ => {}
    }
    assert!(fs::read_to_string(&managed).unwrap().contains(match id {
        "eza" => "directory: {foreground: '",
        "fastfetch" => "\"modules\": [",
        "zsh-syntax-highlighting" => "ZSH_HIGHLIGHT_STYLES[unknown-token]",
        _ => "activeBorderColor: ['",
    }));
    assert_eq!(
        fs::read_to_string(&personal).unwrap(),
        "# PRIVATE_CONFIG\ngui:\n  scrollHeight: 7\n"
    );
    assert_eq!(fs::read_to_string(&shell).unwrap(), "# PRIVATE_SHELL\n");
    assert_eq!(
        fs::read_to_string(env.zshrc_path()).unwrap(),
        "# PRIVATE_STARTUP\n"
    );
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    if id == "fastfetch" {
        // The recommended doctor must agree with the preset just synchronized,
        // and remain read-only before its restore point is inspected below.
        let before_doctor = tree_snapshot::tree(home.path());
        let doctor = command(&env)
            .args(["doctor", "fastfetch", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let doctor: serde_json::Value = serde_json::from_slice(&doctor).unwrap();
        assert!(doctor["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| { check["code"] == "preset_match" && check["status"] == "ok" }));
        assert_eq!(tree_snapshot::tree(home.path()), before_doctor);
    }
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].entries.len(), 1);
    assert_eq!(points[0].entries[0].original_path, managed);
    assert!(
        slate_cli::config::execute_restore_with_env(&env, &points[0].id)
            .unwrap()
            .is_fully_successful()
    );
    assert_eq!(fs::read(&managed).unwrap(), original);
    assert_eq!(
        fs::metadata(&managed).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(fs::read_to_string(&shell).unwrap(), "# PRIVATE_SHELL\n");
    assert_eq!(
        fs::read_to_string(env.zshrc_path()).unwrap(),
        "# PRIVATE_STARTUP\n"
    );
}

#[test]
fn tools_inventory_and_preview_never_launch_tools_or_mutate_profile() {
    let (home, env) = fixture();
    let before = tree_snapshot::tree(home.path());
    let text = command(&env)
        .args(["tools", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("slate tools install btop --dry-run"));
    assert!(text.contains("slate tools sync btop --dry-run"));
    assert!(text.contains("Installation, theme sync and startup integration are separate steps"));
    assert!(!text.contains("Install tools or add shell/editor startup hooks"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let inventory = command(&env)
        .args(["tools", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inventory: serde_json::Value = serde_json::from_slice(&inventory).unwrap();
    assert_eq!(inventory["theme"], "nord");
    assert_eq!(inventory["tools"].as_array().unwrap().len(), 16);
    for id in ["btop", "nvim"] {
        assert!(inventory["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == id && t["available"] == true));
    }
    let preview = command(&env)
        .args(["tools", "sync", "btop", "nvim", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
    assert_eq!(preview["configuration_paths"].as_array().unwrap().len(), 3);
    command(&env).args(["tools"]).assert().success();
    command(&env)
        .args(["tools", "sync", "btop"])
        .assert()
        .code(1);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn tools_theme_entry_readiness_distinguishes_absent_unknown_and_unreadable_state() {
    let (home, env) = fixture();
    fs::remove_file(env.managed_file("current")).unwrap();
    for state in 0..3 {
        if state == 1 {
            seed(&env.managed_file("current"), "PRIVATE_UNKNOWN");
        }
        if state == 2 {
            fs::remove_file(env.managed_file("current")).unwrap();
            fs::create_dir(env.managed_file("current")).unwrap();
        }
        let before = tree_snapshot::tree(home.path());
        for args in [
            vec!["tools", "list", "--json"],
            vec!["tools", "info", "btop", "--json"],
        ] {
            let output = command(&env)
                .args(&args)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            assert!(!String::from_utf8_lossy(&output).contains("PRIVATE_UNKNOWN"));
            let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(report["theme_selection_available"], state != 2);
            assert!(report["theme"].is_null());
            if args[1] == "info" {
                assert_eq!(
                    report["recommended_action"]["action"],
                    if state == 2 { "refresh" } else { "theme" }
                );
                assert_eq!(report["sync_review_available"], false);
            }
        }
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn tool_info_explains_next_steps_even_without_a_theme_or_during_recovery() {
    let (home, env) = fixture();
    // An unreadable theme must not hide discovery or leak its contents.
    seed(&env.managed_file("current"), "PRIVATE_UNKNOWN");
    seed(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_RECOVERY",
    );
    let before = tree_snapshot::tree(home.path());
    for id in ["btop", "starship", "yazi", "zellij", "nvim"] {
        let output = command(&env)
            .args(["tools", "info", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(!String::from_utf8_lossy(&output).contains("PRIVATE_"));
        let info: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(info["schema_version"], 1);
        assert_eq!(info["tool"]["id"], id);
        assert!(info["theme"].is_null());
        assert_eq!(info["sync_review_available"], false);
        assert!(!info["next_steps"].as_array().unwrap().is_empty());
        if id == "nvim" {
            assert_eq!(info["installation"]["guided_install"], false);
        }
    }
    let output = command(&env)
        .args(["tools", "info", "btop"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&output).contains("System monitor"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn tools_sync_cli_requires_explicit_consent_and_updates_only_selected_configs() {
    let (home, env) = fixture();
    seed(
        &BtopAdapter::config_path(&env),
        "color_theme=personal\ntheme_background=false\n",
    );
    seed(&env.managed_file("auto.toml"), "PRIVATE_PAIRING\n");
    seed(
        &env.managed_file("managed/shell/env.zsh"),
        "PRIVATE_SHELL\n",
    );
    command(&env)
        .args(["tools", "sync", "btop", "--yes"])
        .assert()
        .success();
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    assert_eq!(
        fs::read(env.managed_file("auto.toml")).unwrap(),
        b"PRIVATE_PAIRING\n"
    );
    assert_eq!(
        fs::read(env.managed_file("managed/shell/env.zsh")).unwrap(),
        b"PRIVATE_SHELL\n"
    );
    assert!(BtopAdapter::theme_path(&env).exists());
    assert!(!env.slate_cache_dir().join("current_theme.lua").exists());
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].entries.len(), 2);
}

#[test]
fn tools_invalid_inputs_and_broken_saved_state_fail_without_writes_or_content_leaks() {
    let (home, env) = fixture();
    for current in ["nord", "PRIVATE_UNKNOWN", ""] {
        seed(&env.managed_file("current"), current);
        let before = tree_snapshot::tree(home.path());
        for args in [
            vec!["tools", "sync", "btop", "BAD\x1b[31m\n", "--yes"],
            vec!["tools", "sync", "ls_colors", "--yes"],
            vec!["tools", "sync", "nerd-font", "--yes"],
            vec!["tools", "sync", "btop", "--json"],
            vec!["tools", "sync", "btop", "--dry-run", "--yes"],
            vec!["tools", "info", "BAD\x1b[31m\n", "--json"],
        ] {
            let output = command(&env)
                .args(args)
                .assert()
                .failure()
                .get_output()
                .clone();
            assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_UNKNOWN"));
            assert_eq!(tree_snapshot::tree(home.path()), before);
        }
        if current != "nord" {
            let output = command(&env)
                .args(["tools", "sync", "btop", "--yes"])
                .assert()
                .code(1)
                .get_output()
                .clone();
            assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_UNKNOWN"));
            assert_eq!(tree_snapshot::tree(home.path()), before);
        }
    }
    let mut no_home = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    let output = no_home
        .env_clear()
        .args(["tools", "sync", "BAD\x1b[31m\n", "--yes"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("adapter IDs"));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn tools_sync_backup_failure_stops_before_adapter_writes() {
    let (home, env) = fixture();
    seed(&env.slate_cache_dir().join("backups"), "not a directory");
    let before = tree_snapshot::tree(home.path());
    command(&env)
        .args(["tools", "sync", "btop", "--yes"])
        .assert()
        .code(1);
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert!(!BtopAdapter::theme_path(&env).exists());
}

#[test]
fn tools_sync_native_compatibility_failure_is_not_reported_as_a_changed_review() {
    let (home, env) = fixture();
    let output = command(&env)
        .args(["tools", "sync", "btop", "nvim", "--yes"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("readiness check failed"), "{error}");
    assert!(!error.contains("changed after review"), "{error}");
    assert!(
        home.path().join("UNEXPECTED_PROCESS").exists(),
        "explicitly approved nvim version check"
    );
    assert!(!BtopAdapter::theme_path(&env).exists());
    assert!(!env.slate_cache_dir().join("backups").exists());
}

#[test]
fn tools_sync_partial_failure_keeps_recovery_and_never_changes_global_theme() {
    let (_home, env) = fixture();
    let config = BtopAdapter::config_path(&env);
    seed(&config, "color_theme=personal\ncolor_theme=duplicate\n");
    let starship = env.xdg_config_home().join("starship.toml");
    seed(&starship, "# personal\nformat = '$directory'\n");
    let original = fs::read(&starship).unwrap();
    let output = command(&env)
        .args(["tools", "sync", "btop", "starship", "--yes"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("Earlier file writes"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("slate restore"));
    assert_ne!(fs::read(&starship).unwrap(), original);
    assert!(!BtopAdapter::theme_path(&env).exists());
    assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
    let point = list_restore_points_with_env(&env).unwrap().remove(0);
    assert_eq!(point.entries.len(), 3);
    assert!(slate_cli::config::execute_restore_with_env(&env, &point.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(starship).unwrap(), original);
}
