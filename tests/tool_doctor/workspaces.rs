use super::*;
use slate_cli::adapter::{YaziAdapter, ZellijAdapter};

fn apply(env: &SlateEnv, target: &str, theme: &str) {
    write(&env.managed_file("current"), theme);
    let registry = ThemeRegistry::new().unwrap();
    let theme = registry.get(theme).unwrap();
    if target == "yazi" {
        YaziAdapter.apply_theme_with_env(theme, env).unwrap();
    } else {
        ZellijAdapter.apply_theme_with_env(theme, env).unwrap();
    }
}

#[test]
fn yazi_doctor_distinguishes_flavor_slots_ui_syntax_and_personal_overrides() {
    let (_home, env) = fixture();
    for theme in ["nord", "catppuccin-latte"] {
        apply(&env, "yazi", theme);
        let report = diagnose(&env, "yazi");
        for code in [
            "availability",
            "flavor_dark",
            "flavor_light",
            "flavor_ownership",
            "syntax_ownership",
            "flavor_match",
            "syntax_match",
        ] {
            assert_eq!(check(&report, code)["status"], "ok", "{theme}: {report}");
        }
        assert_eq!(check(&report, "personal_overrides")["status"], "info");
        assert!(check(&report, "reload")["suggestion"]
            .as_str()
            .unwrap()
            .contains("reopen Yazi"));
    }
    let path = YaziAdapter::config_path(&env);
    write(&path, "# PRIVATE_CONTENT\nflavor={dark='slate-sync',light='PRIVATE_CONTENT'}\n[mgr]\ncwd={fg='#123456'}\n");
    let report = diagnose(&env, "yazi");
    assert_eq!(check(&report, "flavor_dark")["status"], "ok");
    assert_eq!(check(&report, "flavor_light")["status"], "warning");
    assert_eq!(check(&report, "personal_overrides")["status"], "warning");
    assert_eq!(check(&report, "flavor_match")["status"], "ok");
    for value in [
        "flavor=1\n",
        "[flavor]\ndark=1\n",
        "[flavor]\nlight=false\n",
    ] {
        write(&path, value);
        let report = diagnose(&env, "yazi");
        assert_eq!(check(&report, "config_syntax")["status"], "error");
        absent(&report, "flavor_dark");
    }
    fs::remove_file(path).unwrap();
    let report = diagnose(&env, "yazi");
    assert_eq!(check(&report, "flavor_dark")["status"], "warning");
    assert_eq!(check(&report, "flavor_match")["status"], "ok");
}

#[test]
fn zellij_doctor_distinguishes_three_slots_and_inline_or_external_theme_conflicts() {
    let (_home, env) = fixture();
    for theme in ["nord", "catppuccin-latte"] {
        apply(&env, "zellij", theme);
        let report = diagnose(&env, "zellij");
        for code in [
            "availability",
            "theme_static",
            "theme_dark",
            "theme_light",
            "theme_conflicts",
            "asset_ownership",
            "palette_match",
        ] {
            assert_eq!(check(&report, code)["status"], "ok", "{theme}: {report}");
        }
    }
    let [path, asset] = ZellijAdapter::paths(&env).unwrap();
    let personal = "// PRIVATE_CONTENT\ntheme r#\"slate-sync\"#; theme_dark \"PRIVATE_CONTENT\"\ntheme_light \"slate-sync\"\n/- theme \"ignored\"\n";
    write(&path, personal);
    let report = diagnose(&env, "zellij");
    assert_eq!(check(&report, "theme_static")["status"], "ok");
    assert_eq!(check(&report, "theme_dark")["status"], "warning");
    assert_eq!(check(&report, "theme_light")["status"], "ok");
    for value in [
        "theme 1\n",
        "theme \"slate-sync\"; theme \"PRIVATE_CONTENT\"\n",
    ] {
        write(&path, value);
        let report = diagnose(&env, "zellij");
        assert_eq!(check(&report, "config_syntax")["status"], "error");
        absent(&report, "theme_static");
    }
    write(&path, format!("{personal}themes {{ slate-sync {{}}; }}\n"));
    let report = diagnose(&env, "zellij");
    assert!(check(&report, "theme_conflicts")["message"]
        .as_str()
        .unwrap()
        .contains("inline"));
    write(&path, personal);
    let other = asset.parent().unwrap().join("personal.kdl");
    for (value, reason) in [
        (
            "// PRIVATE_CONTENT\nthemes { slate-sync {}; }\n",
            "another theme file",
        ),
        ("PRIVATE_CONTENT {", "invalid KDL"),
    ] {
        write(&other, value);
        let report = diagnose(&env, "zellij");
        assert_eq!(check(&report, "theme_conflicts")["status"], "error");
        assert!(check(&report, "theme_conflicts")["message"]
            .as_str()
            .unwrap()
            .contains(reason));
        // A matching asset is still observable, not evidence conflicts are absent.
        assert_eq!(check(&report, "palette_match")["status"], "ok");
    }
    fs::remove_file(&other).unwrap();
    fs::remove_file(path).unwrap();
    let report = diagnose(&env, "zellij");
    assert_eq!(check(&report, "theme_static")["status"], "warning");
}

#[test]
fn workspace_doctors_report_palette_drift_unowned_and_missing_assets_separately() {
    let (_home, env) = fixture();
    for target in ["yazi", "zellij"] {
        apply(&env, target, "nord");
    }
    let assets = [
        (
            "yazi",
            YaziAdapter::flavor_path(&env),
            "flavor_ownership",
            "flavor_match",
        ),
        (
            "yazi",
            YaziAdapter::syntax_path(&env),
            "syntax_ownership",
            "syntax_match",
        ),
        (
            "zellij",
            ZellijAdapter::paths(&env).unwrap()[1].clone(),
            "asset_ownership",
            "palette_match",
        ),
    ];
    for (target, path, ownership, palette) in assets {
        let original = fs::read(&path).unwrap();
        write(&path, [original.as_slice(), b"\n"].concat());
        let report = diagnose(&env, target);
        assert_eq!(check(&report, ownership)["status"], "ok");
        assert_eq!(check(&report, palette)["status"], "warning");
        write(&path, "PRIVATE_CONTENT");
        let report = diagnose(&env, target);
        assert_eq!(check(&report, ownership)["status"], "warning");
        absent(&report, palette);
        fs::remove_file(&path).unwrap();
        let report = diagnose(&env, target);
        assert_eq!(check(&report, ownership)["status"], "warning");
        absent(&report, palette);
        write(&path, original);
    }
    write(&env.managed_file("current"), "catppuccin-latte");
    assert_eq!(
        check(&diagnose(&env, "yazi"), "syntax_match")["status"],
        "warning"
    );
    assert_eq!(
        check(&diagnose(&env, "zellij"), "palette_match")["status"],
        "warning"
    );
}

#[test]
fn workspace_doctors_follow_custom_profiles_and_keep_isolation_boundaries() {
    let (_home, env) = fixture();
    let yazi = env.home().join("custom-yazi");
    let zellij = env.home().join("custom-zellij");
    let config = env.home().join("elsewhere/personal.kdl");
    let themes = env.home().join("custom-themes");
    write(
        &config,
        format!("theme_dir {:?}\n", themes.to_str().unwrap()),
    );
    let custom = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(env.home().into()),
        "YAZI_CONFIG_HOME" => Some(yazi.clone().into()),
        "ZELLIJ_CONFIG_DIR" => Some(zellij.clone().into()),
        "ZELLIJ_CONFIG_FILE" => Some(config.clone().into()),
        _ => None,
    })
    .unwrap();
    for target in ["yazi", "zellij"] {
        apply(&custom, target, "nord");
    }
    for isolated in [false, true] {
        for target in ["yazi", "zellij"] {
            let before = tree_snapshot::tree(env.home());
            let report = json(
                command(&env, isolated)
                    .env("YAZI_CONFIG_HOME", &yazi)
                    .env("ZELLIJ_CONFIG_DIR", &zellij)
                    .env("ZELLIJ_CONFIG_FILE", &config),
                target,
            );
            let expected_path = match (target, isolated) {
                ("yazi", false) => yazi.join("theme.toml"),
                ("zellij", false) => config.clone(),
                ("yazi", true) => YaziAdapter::config_path(&env),
                _ => ZellijAdapter::config_path(&env).unwrap(),
            };
            assert_eq!(
                check(&report, "config_file")["path"],
                expected_path.to_str().unwrap()
            );
            if !isolated {
                assert_eq!(
                    check(
                        &report,
                        if target == "yazi" {
                            "flavor_match"
                        } else {
                            "palette_match"
                        }
                    )["status"],
                    "ok"
                );
            }
            assert_eq!(tree_snapshot::tree(env.home()), before);
        }
    }
    let before = tree_snapshot::tree(env.home());
    for key in ["ZELLIJ_CONFIG_DIR", "ZELLIJ_CONFIG_FILE"] {
        for value in ["", "relative"] {
            let report = json(command(&env, false).env(key, value), "zellij");
            assert_eq!(check(&report, "config_path")["status"], "error");
            absent(&report, "config_file");
        }
    }
    let report = json(
        command(&env, false).env("YAZI_CONFIG_HOME", "relative"),
        "yazi",
    );
    assert_eq!(check(&report, "config_file")["status"], "error");
    assert_eq!(tree_snapshot::tree(env.home()), before);
    let outside = tempfile::tempdir().unwrap();
    let outside_asset = outside.path().join("slate-sync.kdl");
    write(&outside_asset, "PRIVATE_CONTENT");
    let path = ZellijAdapter::config_path(&env).unwrap();
    write(
        &path,
        format!("theme_dir {:?}\n", outside.path().to_str().unwrap()),
    );
    let outside_before = tree_snapshot::tree(outside.path());
    let report = diagnose(&env, "zellij");
    assert_eq!(check(&report, "theme_conflicts")["status"], "error");
    assert_eq!(check(&report, "asset_file")["status"], "error");
    absent(&report, "asset_ownership");
    assert_eq!(tree_snapshot::tree(outside.path()), outside_before);
}
