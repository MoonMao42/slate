use super::*;
use crate::theme::ThemeRegistry;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[test]
fn zellij_palettes_cover_every_bundled_theme_and_all_native_components() {
    let themes = ThemeRegistry::new().unwrap();
    for theme in themes.all() {
        let generated = palette::render(theme).unwrap();
        assert!(owns_theme(generated.as_bytes()));
        let doc = parse(generated.as_bytes()).unwrap();
        let colors = doc
            .get("themes")
            .unwrap()
            .children()
            .unwrap()
            .get(NAME)
            .unwrap()
            .children()
            .unwrap();
        assert_eq!(colors.nodes().len(), 15);
        for node in colors.nodes() {
            let values = node.children().unwrap();
            assert_eq!(
                values.nodes().len(),
                if node.name().value() == "multiplayer_user_colors" {
                    10
                } else {
                    6
                }
            );
            for color in values.nodes() {
                assert_eq!(color.entries()[0].value().as_string().unwrap().len(), 7);
            }
        }
    }
}

#[test]
fn zellij_selection_is_lossless_for_comments_raw_strings_nested_choices_and_crlf() {
    let before = b"// PRIVATE\r\ntheme r#\"mine\"#; theme_dark \"dark\" // note\r\nkeybinds { normal { bind \"x\" { theme \"nested\"; }; }; }\r\n/- theme \"disabled\"\r\n";
    let selected = edit(before, false).unwrap();
    let expected = String::from_utf8(before.to_vec())
        .unwrap()
        .replace("r#\"mine\"#", "\"slate-sync\"")
        .replace("\"dark\"", "\"slate-sync\"")
        + "theme_light \"slate-sync\"\r\n";
    assert_eq!(selected, expected.as_bytes());
    assert_eq!(edit(&selected, false).unwrap(), selected);
    let clean = String::from_utf8(clean_config(&selected).unwrap()).unwrap();
    assert!(clean.contains("theme \"default\"; theme_dark \"default\" // note"));
    assert!(clean.contains("theme \"nested\""));
    assert!(clean.contains("/- theme \"disabled\""));
    for bad in [
        "theme 1\n",
        "theme \"a\"; theme \"b\";",
        "theme \"a\" other=1\n",
        "theme (type)\"a\"\n",
        "PRIVATE_BAD {",
    ] {
        assert!(edit(bad.as_bytes(), false)
            .unwrap_err()
            .to_string()
            .ends_with("file contents omitted"));
    }
}

#[test]
fn zellij_comment_masking_keeps_original_unicode_comments_and_exact_edit_spans() {
    let comment = format!(
        "/* 中文\r\n{} PRIVATE {} */",
        "/*".repeat(256),
        "*/".repeat(256)
    );
    let original = format!("{comment}theme /* 参数 */ r#\"mine\"#\r\nnode \"/* literal */\"\r\n");
    let expected = original.replace("r#\"mine\"#", "\"slate-sync\"")
        + "theme_dark \"slate-sync\"\r\ntheme_light \"slate-sync\"\r\n";
    let selected = edit(original.as_bytes(), false).unwrap();
    assert_eq!(selected, expected.as_bytes());
    assert_eq!(edit(&selected, false).unwrap(), selected);
    assert_eq!(
        clean_config(&selected).unwrap(),
        expected.replace("\"slate-sync\"", "\"default\"").as_bytes()
    );
}

#[test]
fn zellij_apply_preserves_modes_noop_identity_and_foreign_themes() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    let config = ZellijAdapter::config_path(&env).unwrap();
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "theme \"personal\"\nmouse_mode false\n").unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();
    let themes = ThemeRegistry::new().unwrap();
    let nord = themes.get("nord").unwrap();
    apply(&env, nord).unwrap();
    let paths = paths(&env).unwrap();
    let inodes = paths.clone().map(|p| fs::metadata(p).unwrap().ino());
    apply(&env, nord).unwrap();
    assert_eq!(
        paths.clone().map(|p| fs::metadata(p).unwrap().ino()),
        inodes
    );
    assert_eq!(
        fs::metadata(&config).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let saved = fs::read(&config).unwrap();
    fs::write(&paths[1], "// FOREIGN\nthemes {}\n").unwrap();
    assert!(apply(&env, nord).is_err());
    assert_eq!(fs::read(&config).unwrap(), saved);
    assert!(fs::read_to_string(&paths[1]).unwrap().contains("FOREIGN"));
}

#[test]
fn zellij_collisions_unsafe_links_and_changed_sources_stop_overwrites() {
    let themes = ThemeRegistry::new().unwrap();
    for case in ["inline", "external", "link", "change", "relative-dir"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().into());
        let [config, asset] = paths(&env).unwrap();
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "theme \"mine\"\n").unwrap();
        match case {
            "inline" => fs::write(&config, "themes { slate-sync {}; }\n").unwrap(),
            "external" => {
                fs::create_dir_all(asset.parent().unwrap()).unwrap();
                fs::write(
                    asset.parent().unwrap().join("personal.kdl"),
                    "themes { slate-sync {}; }\n",
                )
                .unwrap();
            }
            "link" => {
                let target = home.path().join("private");
                fs::write(&target, "PRIVATE").unwrap();
                fs::create_dir_all(asset.parent().unwrap()).unwrap();
                std::os::unix::fs::symlink(target, &asset).unwrap();
            }
            "change" => {
                let captured = File::read(&env, config.clone()).unwrap();
                fs::write(&config, "theme \"new\"\n").unwrap();
                assert!(captured.verify(&env).is_err());
                continue;
            }
            "relative-dir" => fs::write(&config, "theme_dir \"relative\"\n").unwrap(),
            _ => unreachable!(),
        }
        let before = fs::read(&config).unwrap();
        let error = apply(&env, themes.get("nord").unwrap())
            .unwrap_err()
            .to_string();
        if case == "inline" || case == "external" {
            assert!(
                error.contains("theme already exists") || error.contains("defines slate-sync"),
                "{case}: {error}"
            );
        }
        assert_eq!(fs::read(&config).unwrap(), before);
        if case != "link" {
            assert!(!asset.exists());
        }
    }
}

#[test]
fn zellij_changed_theme_dir_or_directory_alias_cannot_escape_inspected_targets() {
    let registry = ThemeRegistry::new().unwrap();
    for redirect_alias in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().into());
        let config = ZellijAdapter::config_path(&env).unwrap();
        let first = home.path().join("themes-a");
        let second = home.path().join("themes-b");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let alias = home.path().join("themes-alias");
        std::os::unix::fs::symlink(&first, &alias).unwrap();
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            format!("theme_dir {:?}\n", alias.to_str().unwrap()),
        )
        .unwrap();
        let targets = paths(&env).unwrap();
        if redirect_alias {
            fs::remove_file(&alias).unwrap();
            std::os::unix::fs::symlink(&second, &alias).unwrap();
        } else {
            fs::write(
                &config,
                format!("theme_dir {:?}\n", second.to_str().unwrap()),
            )
            .unwrap();
        }
        let before = fs::read(&config).unwrap();
        let error = apply(&env.clone(), registry.get("nord").unwrap())
            .unwrap_err()
            .to_string();
        assert!(error.contains("target paths changed"), "{error}");
        assert_eq!(fs::read(&config).unwrap(), before);
        assert!(!targets[1].exists());
        assert!(!first.join("slate-sync.kdl").exists());
        assert!(!second.join("slate-sync.kdl").exists());
        // A new invocation may review the new target and then apply normally.
        apply(
            &SlateEnv::with_home(home.path().into()),
            registry.get("nord").unwrap(),
        )
        .unwrap();
        assert!(second.join("slate-sync.kdl").exists());
    }
}
