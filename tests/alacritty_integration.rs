//! Actual adapter writes in disposable profiles; no terminal is launched.
use slate_cli::adapter::{AlacrittyAdapter, ApplyOutcome, SkipReason, ToolAdapter};
use slate_cli::config::ConfigManager;
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn parse(path: &Path) -> toml::Value {
    toml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn alacritty_applies_in_the_effective_import_list_without_migrating_user_config() {
    let themes = ThemeRegistry::new().unwrap();
    for input in [
        "# root\nimport = ['base.toml', 'override.toml']\n[general]\n# inactive\nimport = ['inactive.toml']\n",
        "# root\nimport = ['base.toml', 'override.toml']\n",
        "[general]\n# imports\nimport = ['base.toml', 'override.toml']\n",
        "general = { import = ['base.toml', 'override.toml'], live_config_reload = false }\n",
        "general.import = ['base.toml', 'override.toml']\n",
        "general = { live_config_reload = false }\n",
        "# no imports yet\n",
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("alacritty/alacritty.toml");
        let source = format!("{input}[font.normal]\nfamily = 'User Mono'\nstyle = 'Medium'\n");
        write(&path, &source);
        let original = parse(&path);
        // An isolated profile can still discover system fonts. Pin the choice
        // so this test does not depend on fonts installed on the test machine.
        ConfigManager::with_env(&env).unwrap().set_current_font("Prepared Mono").unwrap();
        assert!(matches!(AlacrittyAdapter.apply_theme_with_env(themes.get("nord").unwrap(), &env).unwrap(), ApplyOutcome::Applied { .. }));
        let after = parse(&path);
        let mut expected_font = original["font"].clone();
        expected_font["normal"].as_table_mut().unwrap().remove("family");
        assert_eq!(after["font"], expected_font);
        let original_imports = original.get("import").or_else(|| original.get("general").and_then(|g| g.get("import")));
        let active = after.get("import").or_else(|| after.get("general").and_then(|g| g.get("import"))).unwrap().as_array().unwrap();
        let mut expected = original_imports.and_then(toml::Value::as_array).cloned().unwrap_or_default();
        expected.push(toml::Value::String(env.managed_file("managed/alacritty/colors.toml").display().to_string()));
        expected.push(toml::Value::String(env.managed_file("managed/alacritty/opacity.toml").display().to_string()));
        assert_eq!(active, &expected);
        if original.get("import").is_some() { assert_eq!(after.get("general"), original.get("general")); }
        for comment in input.lines().filter(|line| line.starts_with('#')) { assert!(fs::read_to_string(&path).unwrap().contains(comment)); }
        let bytes = fs::read(&path).unwrap();
        let meta = fs::metadata(&path).unwrap();
        AlacrittyAdapter.apply_theme_with_env(themes.get("catppuccin-mocha").unwrap(), &env).unwrap();
        let second = fs::metadata(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!((second.ino(), second.modified().unwrap()), (meta.ino(), meta.modified().unwrap()));
        assert_eq!(second.permissions().mode() & 0o777, 0o640);
    }
}

#[test]
fn alacritty_font_changes_preserve_style_size_and_other_font_faces() {
    let themes = ThemeRegistry::new().unwrap();
    for font in [
        "[font]\nsize = 14\n[font.normal]\nfamily = 'User Mono'\n# style survives\nstyle = 'Medium'\n[font.bold]\nfamily = 'User Bold'\n",
        "font = { normal = { family = 'User Mono', style = 'Medium' }, size = 14, bold = { family = 'User Bold' } }\n",
        "font.normal.family = 'User Mono'\nfont.normal.style = 'Medium'\nfont.size = 14\nfont.bold.family = 'User Bold'\n",
    ] {
        for theme_apply in [false, true] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let path = env.xdg_config_home().join("alacritty/alacritty.toml");
            write(&path, font);
            let mut expected_font = parse(&path)["font"].clone();
            expected_font["normal"].as_table_mut().unwrap().remove("family");
            if theme_apply {
                ConfigManager::with_env(&env).unwrap().set_current_font("New Mono").unwrap();
                AlacrittyAdapter.apply_theme_with_env(themes.get("nord").unwrap(), &env).unwrap();
            } else {
                AlacrittyAdapter::apply_font_only(&env, "New Mono").unwrap();
            }
            assert_eq!(parse(&path)["font"], expected_font);
            if font.contains("# style survives") { assert!(fs::read_to_string(&path).unwrap().contains("# style survives")); }
            let file = if theme_apply { "colors.toml" } else { "font.toml" };
            assert_eq!(parse(&env.managed_file(&format!("managed/alacritty/{file}")))["font"]["normal"]["family"].as_str(), Some("New Mono"));
        }
    }
}

// The parent runs this probe with a five-second deadline so a regression to a
// blocking FIFO read cannot stall the test runner. Normal discovery is a no-op.
#[test]
fn alacritty_invalid_input_probe() {
    let Some(home) = std::env::var_os("SLATE_ALACRITTY_PROBE_HOME") else {
        return;
    };
    let env = SlateEnv::with_home(home.into());
    let themes = ThemeRegistry::new().unwrap();
    for error in [
        AlacrittyAdapter::apply_font_only(&env, "New Mono").unwrap_err(),
        AlacrittyAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .unwrap_err(),
    ] {
        assert!(!error.to_string().contains("PRIVATE_CONTENT"));
    }
}

#[test]
fn alacritty_rejects_invalid_inputs_without_writes_or_blocking() {
    for case in [
        "toml",
        "utf8",
        "general",
        "array",
        "element",
        "font",
        "fifo",
        "directory",
        "link",
        "dangling",
        "large",
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("alacritty/alacritty.toml");
        write(&env.managed_file("current-font"), "New Mono\n");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        match case {
            "toml" => write(&path, "private_key = 'PRIVATE_CONTENT'\n[broken TOML"),
            "utf8" => write(&path, b"# PRIVATE_CONTENT\xff"),
            "general" => write(&path, "general = 42\n"),
            "array" => write(&path, "import = 'PRIVATE_CONTENT'\n"),
            "element" => write(&path, "[general]\nimport = ['PRIVATE_CONTENT', 42]\n"),
            "font" => write(&path, "[font]\nnormal = 42\n"),
            "fifo" => {
                let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&path).unwrap(),
            "link" | "dangling" => {
                let target = td.path().join("user-target");
                if case == "link" {
                    write(&target, "# PRIVATE_CONTENT\n");
                }
                symlink(target, &path).unwrap();
            }
            "large" => fs::File::create(&path)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(td.path());
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("PATH", td.path().join("bin"))
            .env("SLATE_ALACRITTY_PROBE_HOME", td.path())
            .args(["--exact", "alacritty_invalid_input_probe", "--nocapture"])
            .timeout(Duration::from_secs(5))
            .assert()
            .success();
        assert_eq!(tree_snapshot::tree(td.path()), before, "{case}");
    }
}

#[test]
fn alacritty_missing_entry_keeps_the_existing_skip_contract() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let themes = ThemeRegistry::new().unwrap();
    let before = tree_snapshot::tree(td.path());
    assert!(matches!(
        AlacrittyAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .unwrap(),
        ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig)
    ));
    assert_eq!(tree_snapshot::tree(td.path()), before);
    AlacrittyAdapter::apply_font_only(&env, "New Mono").unwrap();
    assert!(env.managed_file("managed/alacritty/font.toml").is_file());
    assert!(!env
        .xdg_config_home()
        .join("alacritty/alacritty.toml")
        .exists());
}
