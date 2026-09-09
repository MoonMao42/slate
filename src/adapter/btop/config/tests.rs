use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
};

fn seed(path: &std::path::Path, text: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

#[test]
fn btop_palette_covers_all_native_keys_for_every_slate_theme() {
    let registry = crate::theme::ThemeRegistry::new().unwrap();
    for theme in registry.all() {
        let text = palette::render(theme).unwrap();
        assert!(is_owned_theme(text.as_bytes()));
        let lines: Vec<_> = text.lines().skip(1).collect();
        let keys: std::collections::HashSet<_> = lines
            .iter()
            .map(|line| line.split_once('=').unwrap().0)
            .collect();
        assert_eq!(lines.len(), 48);
        assert_eq!(keys.len(), lines.len());
        assert!(text.contains(&format!("theme[main_bg]=\"{}\"", theme.palette.background)));
        assert!(text.contains(&format!("theme[main_fg]=\"{}\"", theme.palette.foreground)));
        for line in lines {
            let value = line.split_once('=').unwrap().1.trim_matches('"');
            assert_eq!(value.len(), 7);
            assert!(value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }
}

#[test]
fn btop_config_splices_one_value_preserving_layout_comments_and_line_endings() {
    for original in [
        "# 用户\r\ncolor_theme = \"old\"  # keep\r\ntheme_background = false\r\nupdate_ms = 1500\r\n",
        "color_theme=old\nshown_boxes = \"cpu mem net proc\"",
        "  color_theme = \"old\"\nclock_format = \"%H:%M # = ok\"\n",
    ] {
        let out = set_theme(original, "/a space/slate-sync.theme").unwrap();
        let quoted = if original.contains("\"old\"") { "\"old\"" } else { "old" };
        assert_eq!(out, original.replace(quoted, "\"/a space/slate-sync.theme\"").as_bytes());
        assert_eq!(set_theme(std::str::from_utf8(&out).unwrap(), "/a space/slate-sync.theme").unwrap(), out);
    }
    assert_eq!(
        set_theme("# no setting", "/x").unwrap(),
        b"# no setting\ncolor_theme = \"/x\"\n"
    );
    for bad in [
        "color_theme = \"a\"\ncolor_theme = \"b\"",
        "color_theme = \"unterminated",
        "color_theme = \"x\"junk",
        "invalid\ncolor_theme = x",
        "color_theme =",
        "other = \"multi\nline\"",
    ] {
        assert!(set_theme(bad, "/x").is_err(), "{bad:?}");
    }
}

#[test]
fn btop_apply_preserves_preferences_modes_and_noop_file_identity() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = BtopAdapter::config_path(&env);
    let asset = BtopAdapter::theme_path(&env);
    let input =
        "# personal\r\ncolor_theme = \"old\"\r\ntheme_background = false\r\nupdate_ms = 1500\r\n";
    seed(&config, input);
    let registry = crate::theme::ThemeRegistry::new().unwrap();
    apply(&env, registry.get("nord").unwrap()).unwrap();
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        input.replace("\"old\"", &format!("\"{}\"", asset.display()))
    );
    assert_eq!(
        fs::metadata(&config).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let stamp = |path: &std::path::Path| {
        let m = fs::metadata(path).unwrap();
        (m.ino(), m.modified().unwrap(), fs::read(path).unwrap())
    };
    let before = (stamp(&config), stamp(&asset));
    apply(&env, registry.get("nord").unwrap()).unwrap();
    assert_eq!((stamp(&config), stamp(&asset)), before);
    apply(&env, registry.get("catppuccin-latte").unwrap()).unwrap();
    assert_eq!(stamp(&config), before.0);
    assert_ne!(fs::read(&asset).unwrap(), before.1 .2);
    assert!(
        !env.config_dir().exists(),
        "adapter must not initialize unrelated Slate files"
    );
    assert!(!env.slate_cache_dir().exists());
}

#[test]
fn btop_invalid_or_foreign_destinations_fail_before_writes() {
    for case in [
        "duplicate",
        "utf8",
        "foreign",
        "link",
        "outside",
        "oversized",
        "bad-palette",
    ] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let config = BtopAdapter::config_path(&env);
        let asset = BtopAdapter::theme_path(&env);
        seed(&config, "color_theme = \"old\"\n");
        let mut theme = crate::theme::ThemeRegistry::new()
            .unwrap()
            .get("nord")
            .unwrap()
            .clone();
        match case {
            "duplicate" => seed(&config, "color_theme=x\ncolor_theme=y\n"),
            "utf8" => seed(&config, [0xff]),
            "foreign" => seed(&asset, "# user-created theme\n"),
            "link" => {
                fs::create_dir_all(asset.parent().unwrap()).unwrap();
                symlink(&config, &asset).unwrap();
            }
            "outside" => {
                seed(&outside.path().join("slate-sync.theme"), "PRIVATE_CONTENT");
                symlink(outside.path(), asset.parent().unwrap()).unwrap();
            }
            "oversized" => {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&config)
                    .unwrap()
                    .set_len(MAX_TOOL_CONFIG_BYTES + 1)
                    .unwrap();
            }
            "bad-palette" => theme.palette.red = "PRIVATE_CONTENT\n".into(),
            _ => unreachable!(),
        }
        let before = fs::read(&config).unwrap();
        assert!(apply(&env, &theme).is_err(), "{case}");
        assert_eq!(fs::read(&config).unwrap(), before);
        if matches!(case, "duplicate" | "utf8" | "oversized" | "bad-palette") {
            assert!(!asset.exists());
        }
        if case == "foreign" {
            assert_eq!(fs::read(&asset).unwrap(), b"# user-created theme\n");
        }
        if case == "outside" {
            assert_eq!(
                fs::read(outside.path().join("slate-sync.theme")).unwrap(),
                b"PRIVATE_CONTENT"
            );
        }
        assert!(!env.slate_cache_dir().exists());
    }
}

#[test]
fn btop_prepared_write_rejects_external_changes_and_directory_redirection() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let path = BtopAdapter::config_path(&env);
    seed(&path, "color_theme=x\n");
    let mut prepared = FilePlan::read(&env, path.clone()).unwrap();
    prepared.bytes = b"color_theme=y\n".to_vec();
    seed(&path, "color_theme=user\n");
    assert!(prepared.publish(&env).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"color_theme=user\n");
    let prepared = FilePlan::read(&env, path.clone()).unwrap();
    let moved = home.path().join("moved");
    fs::rename(path.parent().unwrap(), &moved).unwrap();
    symlink(&moved, path.parent().unwrap()).unwrap();
    assert!(
        prepared.verify(&env).is_err(),
        "same inode under a redirected parent is not the prepared destination"
    );
}

#[test]
fn btop_cleanup_disconnects_only_the_exact_owned_reference() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let original = format!(
        "# user\ncolor_theme = \"{}\" # keep\nupdate_ms=1500\n",
        BtopAdapter::theme_path(&env).display()
    );
    assert_eq!(
        clean_config(&env, original.as_bytes()).unwrap(),
        b"# user\ncolor_theme = \"Default\" # keep\nupdate_ms=1500\n"
    );
    for text in [
        "color_theme = \"personal\"\n",
        "color_theme = \"slate-sync\"\n",
        "color_theme = \"/another/slate-sync.theme\"\n",
    ] {
        assert_eq!(
            clean_config(&env, text.as_bytes()).unwrap(),
            text.as_bytes()
        );
    }
}
