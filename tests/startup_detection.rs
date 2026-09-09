//! Read-only setup hints. Hazardous files run in a private subprocess so an IO
//! regression fails its deadline instead of hanging the entire test runner.
use slate_cli::cli::wizard_core::Wizard;
use slate_cli::env::SlateEnv;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn fifo(path: &Path) {
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
}

fn probe(home: &Path, font: Option<&str>, theme: Option<&str>) {
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .env("SLATE_STARTUP_EXPECT_FONT", font.unwrap_or_default())
        .env("SLATE_STARTUP_EXPECT_THEME", theme.unwrap_or_default())
        .args([
            "--exact",
            "startup_detection_child",
            "--ignored",
            "--nocapture",
        ])
        .timeout(Duration::from_secs(4))
        .assert()
        .success();
}

#[test]
#[ignore = "called only by deadline-guarded private-profile tests"]
fn startup_detection_child() {
    assert!(std::env::var_os("SLATE_HOME").is_some());
    let expected = |key| std::env::var(key).ok().filter(|value| !value.is_empty());
    let wizard = Wizard::new().unwrap();
    assert_eq!(
        wizard.get_context().current_font,
        expected("SLATE_STARTUP_EXPECT_FONT")
    );
    assert_eq!(
        wizard.get_context().current_theme,
        expected("SLATE_STARTUP_EXPECT_THEME")
    );
}

#[test]
fn startup_detection_skips_fifo_without_hanging() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let path = env.xdg_config_home().join("ghostty/config.ghostty");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fifo(&path);
    write(
        &env.xdg_config_home().join("alacritty/alacritty.toml"),
        "[font.normal]\nfamily = 'Fallback Font'\n",
    );
    let before = tree_snapshot::tree(td.path());
    probe(td.path(), Some("Fallback Font"), None);
    assert_eq!(tree_snapshot::tree(td.path()), before);
}

#[test]
fn startup_detection_skips_unsafe_sources_and_keeps_other_hints() {
    for source in ["ghostty", "alacritty", "theme"] {
        for case in [
            "fifo",
            "linked_fifo",
            "directory",
            "dangling",
            "large",
            "invalid",
        ] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
            if source == "ghostty" {
                write(&alacritty, "[font.normal]\nfamily = 'Fallback Font'\n");
            }
            let path = match source {
                "ghostty" => env.xdg_config_home().join("ghostty/config.ghostty"),
                "alacritty" => alacritty,
                "theme" => env.managed_file("current"),
                _ => unreachable!(),
            };
            if source != "theme" {
                write(&env.managed_file("current"), "nord\n");
            }
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match case {
                "fifo" => fifo(&path),
                "linked_fifo" => {
                    let target = td.path().join("pipe");
                    fifo(&target);
                    symlink(target, &path).unwrap();
                }
                "directory" => fs::create_dir(&path).unwrap(),
                "dangling" => symlink(td.path().join("absent"), &path).unwrap(),
                "large" => fs::File::create(&path)
                    .unwrap()
                    .set_len(if source == "theme" {
                        4097
                    } else {
                        8 * 1024 * 1024 + 1
                    })
                    .unwrap(),
                "invalid" => write(&path, b"\xff\0\x1b[31mPRIVATE_CONTENT\n"),
                _ => unreachable!(),
            }
            let before = tree_snapshot::tree(td.path());
            probe(
                td.path(),
                (source == "ghostty").then_some("Fallback Font"),
                (source != "theme").then_some("nord"),
            );
            assert_eq!(tree_snapshot::tree(td.path()), before, "{source}: {case}");
        }
    }
}

#[test]
fn startup_detection_accepts_exact_limits_but_never_parses_truncated_prefixes() {
    for (relative, prefix, limit, font, theme) in [
        (
            ".config/ghostty/config.ghostty",
            "font-family = Boundary Font\n#",
            8 * 1024 * 1024,
            Some("Boundary Font"),
            None,
        ),
        (
            ".config/alacritty/alacritty.toml",
            "[font.normal]\nfamily = 'Boundary Font'\n#",
            8 * 1024 * 1024,
            Some("Boundary Font"),
            None,
        ),
        (".config/slate/current", "nord", 4096, None, Some("nord")),
    ] {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join(relative);
        let mut content = prefix.as_bytes().to_vec();
        content.resize(limit, b' ');
        write(&path, &content);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let before = tree_snapshot::tree(td.path());
        probe(td.path(), font, theme);
        assert_eq!(tree_snapshot::tree(td.path()), before);
        content.push(b' ');
        write(&path, &content);
        let before = tree_snapshot::tree(td.path());
        probe(td.path(), None, None);
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}

#[test]
fn startup_detection_uses_custom_xdg_and_alacritty_selection_without_initialization() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("home");
    let custom = td.path().join("custom-xdg");
    let env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(home.as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(custom.as_os_str().to_owned()),
        _ => None,
    })
    .unwrap();
    let paths = [
        custom.join("alacritty/alacritty.toml"),
        custom.join("alacritty.toml"),
        home.join(".config/alacritty/alacritty.toml"),
        home.join(".alacritty.toml"),
    ];
    let fonts = ["XDG nested", "XDG direct", "Home config", "Home dotfile"];
    for (path, font) in paths.iter().zip(fonts) {
        write(path, format!("[font.normal]\nfamily = '{font}'\n"));
    }
    write(&home.join(".config/slate/current"), "dracula\n");
    write(&env.managed_file("current"), "nord\n");
    for (path, font) in paths.iter().zip(fonts) {
        let before = tree_snapshot::tree(td.path());
        let wizard = Wizard::with_env(&env).unwrap();
        assert_eq!(wizard.get_context().current_font.as_deref(), Some(font));
        assert_eq!(wizard.get_context().current_theme.as_deref(), Some("nord"));
        assert_eq!(tree_snapshot::tree(td.path()), before);
        // A malformed higher-priority file must not select a lower-priority
        // Alacritty config that the adapter would never use.
        write(path, "[invalid TOML");
        assert_eq!(
            Wizard::with_env(&env).unwrap().get_context().current_font,
            None
        );
        fs::remove_file(path).unwrap();
    }
    assert!(!env.slate_cache_dir().exists());
    let isolated = SlateEnv::with_home(home);
    assert_eq!(
        Wizard::with_env(&isolated)
            .unwrap()
            .get_context()
            .current_theme
            .as_deref(),
        Some("dracula")
    );
}

#[test]
fn startup_detection_keeps_valid_literal_fonts_and_skips_control_data() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let path = env.xdg_config_home().join("alacritty/alacritty.toml");
    for font in [
        "  Literal 字体  ",
        "Quote\"Mono",
        r"Back\slash Mono",
        "O'Connor Mono",
    ] {
        for content in [
            format!("[font.normal]\nfamily = {}\n", toml_edit::Value::from(font)),
            format!(
                "font = {{ normal = {{ family = {} }} }}\n",
                toml_edit::Value::from(font)
            ),
            format!("font.normal.family = {}\n", toml_edit::Value::from(font)),
        ] {
            write(&path, content);
            let before = tree_snapshot::tree(td.path());
            assert_eq!(
                Wizard::with_env(&env)
                    .unwrap()
                    .get_context()
                    .current_font
                    .as_deref(),
                Some(font)
            );
            assert_eq!(tree_snapshot::tree(td.path()), before);
        }
    }
    let oversized = "x".repeat(257);
    for font in [
        "Bad\x1b[31m",
        "Bad\0font",
        "Bad\nfont",
        "Bad\u{2028}font",
        "Bad\u{2029}font",
        "   ",
        oversized.as_str(),
    ] {
        write(
            &path,
            format!("[font.normal]\nfamily = {}\n", toml_edit::Value::from(font)),
        );
        assert_eq!(
            Wizard::with_env(&env).unwrap().get_context().current_font,
            None
        );
    }
    for theme in [
        "nord\x1b[31m",
        "nord\0",
        "nord\nother",
        "nord\u{2028}other",
        "nord\u{2029}other",
        "  ",
    ] {
        write(&env.managed_file("current"), theme);
        let before = tree_snapshot::tree(td.path());
        assert_eq!(
            Wizard::with_env(&env).unwrap().get_context().current_theme,
            None
        );
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
    write(&env.managed_file("current"), "my-unknown-theme\n");
    assert_eq!(
        Wizard::with_env(&env)
            .unwrap()
            .get_context()
            .current_theme
            .as_deref(),
        Some("my-unknown-theme")
    );
}

#[test]
fn startup_detection_preserves_dotfile_links_and_isolated_profile_boundaries() {
    for relative in [
        ".config/ghostty/config.ghostty",
        ".config/alacritty/alacritty.toml",
        ".config/slate/current",
    ] {
        for directory_link in [false, true] {
            let td = tempfile::tempdir().unwrap();
            let home = td.path().join("home");
            let outside = td.path().join("home-outside");
            let env = SlateEnv::with_home(home.clone());
            let entry = home.join(relative);
            let content = if relative.contains("ghostty") {
                "font-family = Linked Font\n"
            } else if relative.contains("alacritty") {
                "[font.normal]\nfamily = 'Linked Font'\n"
            } else {
                "nord\n"
            };
            for target_dir in [home.join("dotfiles"), outside] {
                let target = target_dir.join(entry.file_name().unwrap());
                write(&target, content);
                let link = if directory_link {
                    entry.parent().unwrap()
                } else {
                    entry.as_path()
                };
                fs::create_dir_all(link.parent().unwrap()).unwrap();
                symlink(
                    if directory_link {
                        target_dir.as_path()
                    } else {
                        target.as_path()
                    },
                    link,
                )
                .unwrap();
                let before = tree_snapshot::tree(td.path());
                let wizard = Wizard::with_env(&env).unwrap();
                let within = target_dir.starts_with(&home);
                assert_eq!(
                    wizard.get_context().current_font.as_deref(),
                    (within && !relative.ends_with("current")).then_some("Linked Font")
                );
                assert_eq!(
                    wizard.get_context().current_theme.as_deref(),
                    (within && relative.ends_with("current")).then_some("nord")
                );
                // Ordinary, non-isolated profiles can keep dotfiles elsewhere.
                let ordinary =
                    SlateEnv::from_vars(|key| (key == "HOME").then(|| home.as_os_str().to_owned()))
                        .unwrap();
                let wizard = Wizard::with_env(&ordinary).unwrap();
                assert_eq!(
                    wizard.get_context().current_font.as_deref(),
                    (!relative.ends_with("current")).then_some("Linked Font")
                );
                assert_eq!(
                    wizard.get_context().current_theme.as_deref(),
                    relative.ends_with("current").then_some("nord")
                );
                assert_eq!(tree_snapshot::tree(td.path()), before);
                fs::remove_file(link).unwrap();
            }
        }
    }
}
