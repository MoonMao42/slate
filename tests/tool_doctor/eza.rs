use super::*;
use slate_cli::adapter::EzaAdapter;

fn seed() -> (tempfile::TempDir, SlateEnv) {
    let (temp, env) = fixture();
    let binary = env.user_local_bin().join("eza");
    write(
        &binary,
        "#!/bin/sh\nprintf unexpected > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    EzaAdapter
        .apply_theme_with_env(ThemeRegistry::new().unwrap().get("nord").unwrap(), &env)
        .unwrap();
    (temp, env)
}

#[test]
fn eza_override_matching_requires_known_theme_and_preserves_private_bytes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let (_temp, env) = seed();
    let registry = ThemeRegistry::new().unwrap();
    let (ls, eza) =
        slate_cli::adapter::ls_colors::render_strings(&registry.get("nord").unwrap().palette);
    let before = tree_snapshot::tree(env.home());
    for variable in ["EZA_COLORS", "LS_COLORS"] {
        let raw = OsString::from_vec(b"PRIVATE_COLOR_\xff\x1b[2J".to_vec());
        let report = json(command(&env, false).env(variable, &raw), "eza");
        assert_eq!(check(&report, "color_overrides")["status"], "warning");
        assert!(!report.to_string().contains("PRIVATE_COLOR"));
        let isolated = json(command(&env, true).env(variable, &raw), "eza");
        assert_eq!(check(&isolated, "color_overrides")["status"], "info");
        assert!(check(&isolated, "color_overrides")["message"]
            .as_str()
            .unwrap()
            .starts_with("No nonempty"));
    }
    assert_eq!(tree_snapshot::tree(env.home()), before);
    for theme_record in ["unknown-theme\n", ""] {
        write(&env.managed_file("current"), theme_record);
        let before = tree_snapshot::tree(env.home());
        let report = json(
            command(&env, false)
                .env("EZA_COLORS", &eza)
                .env("LS_COLORS", &ls),
            "eza",
        );
        assert_eq!(check(&report, "color_overrides")["status"], "warning");
        assert!(!check(&report, "color_overrides")["message"]
            .as_str()
            .unwrap()
            .contains("exactly match"));
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
}

#[test]
fn eza_doctor_recognizes_exact_current_exports_but_not_stale_or_mixed_values() {
    let (_temp, env) = seed();
    let registry = ThemeRegistry::new().unwrap();
    let (ls, eza) =
        slate_cli::adapter::ls_colors::render_strings(&registry.get("nord").unwrap().palette);
    let before = tree_snapshot::tree(env.home());
    let (old_ls, old_eza) = slate_cli::adapter::ls_colors::render_strings(
        &registry.get("catppuccin-mocha").unwrap().palette,
    );
    for (eza_value, ls_value, status) in [
        (eza.as_str(), ls.as_str(), "info"),
        (eza.as_str(), "", "info"),
        ("", ls.as_str(), "info"),
        (eza.as_str(), "PRIVATE_CONTENT", "warning"),
        ("PRIVATE_CONTENT", ls.as_str(), "warning"),
        (old_eza.as_str(), old_ls.as_str(), "warning"),
    ] {
        let report = json(
            command(&env, false)
                .env("EZA_COLORS", eza_value)
                .env("LS_COLORS", ls_value),
            "eza",
        );
        assert_eq!(check(&report, "color_overrides")["status"], status);
        assert!(!report.to_string().contains("PRIVATE_CONTENT"));
        if status == "info" {
            assert!(check(&report, "color_overrides")["message"]
                .as_str()
                .unwrap()
                .contains("exactly match"));
        }
    }
    assert_eq!(tree_snapshot::tree(env.home()), before);
}

#[test]
fn eza_doctor_separates_palette_selection_and_overrides_without_writes() {
    let (_temp, env) = seed();
    let managed = EzaAdapter::theme_path(&env);
    write(
        &env.eza_config_home().join("theme.yml"),
        "PRIVATE_CONTENT: not parsed\n",
    );
    let before = tree_snapshot::tree(env.home());
    let report = json(&mut command(&env, false), "eza");
    assert_eq!(check(&report, "palette_match")["status"], "ok");
    assert_eq!(check(&report, "environment_selection")["status"], "warning");
    assert_eq!(check(&report, "color_overrides")["status"], "info");
    for variable in ["EZA_COLORS", "LS_COLORS"] {
        let report = json(
            command(&env, false)
                .env("EZA_CONFIG_DIR", managed.parent().unwrap())
                .env(variable, "PRIVATE_CONTENT"),
            "eza",
        );
        assert_eq!(check(&report, "environment_selection")["status"], "info");
        assert_eq!(check(&report, "color_overrides")["status"], "warning");
        let isolated = json(
            command(&env, true)
                .env("EZA_CONFIG_DIR", managed.parent().unwrap())
                .env(variable, "PRIVATE_CONTENT"),
            "eza",
        );
        assert_eq!(check(&isolated, "isolated_profile")["status"], "info");
        assert_eq!(
            check(&isolated, "environment_selection")["status"],
            "warning"
        );
        assert_eq!(check(&isolated, "color_overrides")["status"], "info");
    }
    let info = command(&env, false)
        .args(["tools", "info", "eza", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let info: serde_json::Value = serde_json::from_slice(&info).unwrap();
    assert_eq!(info["recommended_action"]["action"], "check");
    assert_eq!(tree_snapshot::tree(env.home()), before);
}

#[test]
fn eza_doctor_flags_missing_selected_palette_without_guessing_unreadable_or_personal_state() {
    let (_temp, env) = seed();
    let managed = EzaAdapter::theme_path(&env);
    fs::remove_file(&managed).unwrap();
    for saved in ["nord", "PRIVATE_CONTENT"] {
        write(&env.managed_file("current"), saved);
        let before = tree_snapshot::tree(env.home());
        let report = json(
            command(&env, false).env("EZA_CONFIG_DIR", managed.parent().unwrap()),
            "eza",
        );
        assert_eq!(
            check(&report, "selected_palette_missing")["status"],
            "error"
        );
        absent(&report, "palette_match");
        let unselected = json(&mut command(&env, false), "eza");
        absent(&unselected, "selected_palette_missing");
        let isolated = json(
            command(&env, true).env("EZA_CONFIG_DIR", managed.parent().unwrap()),
            "eza",
        );
        absent(&isolated, "selected_palette_missing");
        assert_eq!(tree_snapshot::tree(env.home()), before);
    }
    // Unsafe or unreadable is not the same observation as absent.
    symlink(env.managed_file("current"), &managed).unwrap();
    let before = tree_snapshot::tree(env.home());
    let report = json(
        command(&env, false).env("EZA_CONFIG_DIR", managed.parent().unwrap()),
        "eza",
    );
    assert_eq!(check(&report, "palette_file")["status"], "error");
    absent(&report, "selected_palette_missing");
    assert_eq!(tree_snapshot::tree(env.home()), before);
}

#[test]
fn eza_doctor_handles_stale_unknown_oversized_and_linked_files_readonly() {
    let (_temp, env) = seed();
    let managed = EzaAdapter::theme_path(&env);
    write(&managed, "colors:\n  info: '#123456'\n");
    let report = json(&mut command(&env, true), "eza");
    assert_eq!(check(&report, "palette_match")["status"], "warning");
    write(&env.managed_file("current"), "PRIVATE_CONTENT");
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    fs::OpenOptions::new()
        .write(true)
        .open(&managed)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    let before = tree_snapshot::tree(env.home());
    let report = json(&mut command(&env, true), "eza");
    assert_eq!(check(&report, "palette_file")["status"], "error");
    absent(&report, "palette_match");
    assert_eq!(tree_snapshot::tree(env.home()), before);
    fs::remove_file(&managed).unwrap();
    symlink(env.managed_file("current"), &managed).unwrap();
    let before = tree_snapshot::tree(env.home());
    let report = json(&mut command(&env, true), "eza");
    assert_eq!(check(&report, "palette_file")["status"], "error");
    absent(&report, "palette_match");
    assert_eq!(tree_snapshot::tree(env.home()), before);
}
