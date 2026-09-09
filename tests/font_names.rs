//! Font-name serialization through real adapter writers, not string snapshots.
//! Native Ghostty parsing is opt-in and only loads a private generated config.
use slate_cli::adapter::{
    AlacrittyAdapter, FontAdapter, GhosttyAdapter, KittyAdapter, ToolAdapter,
};
use slate_cli::config::ConfigManager;
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::process::Command;
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

#[test]
fn alacritty_font_names_round_trip_in_font_and_theme_files() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let entry = env.xdg_config_home().join("alacritty/alacritty.toml");
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, "# user entry\n").unwrap();
    let config = ConfigManager::with_env(&env).unwrap();
    let themes = ThemeRegistry::new().unwrap();
    for family in [
        "JetBrainsMono Nerd Font",
        "Quote\"Mono",
        r"Back\slash Mono",
        "O'Connor Mono",
        "字形 Mono",
        "family=Mono",
        "# Literal; $Mono",
    ] {
        AlacrittyAdapter::apply_font_only(&env, family).unwrap();
        config.set_current_font(family).unwrap();
        AlacrittyAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .unwrap();
        for file in ["font.toml", "colors.toml"] {
            let text =
                fs::read_to_string(env.config_dir().join("managed/alacritty").join(file)).unwrap();
            let parsed: toml::Value =
                toml::from_str(&text).unwrap_or_else(|error| panic!("{file}, {family:?}: {error}"));
            assert_eq!(parsed["font"]["normal"]["family"].as_str(), Some(family));
            assert_eq!(parsed["font"].as_table().unwrap().len(), 1);
        }
    }
}

#[test]
fn font_write_boundaries_reject_control_text_without_modifying_files() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_font("Old Mono").unwrap();
    let before = tree_snapshot::tree(home.path());
    let long = "x".repeat(257);
    type Apply = fn(&SlateEnv, &str) -> slate_cli::error::Result<()>;
    let writers: [Apply; 4] = [
        FontAdapter::apply_font,
        GhosttyAdapter::apply_font_only,
        AlacrittyAdapter::apply_font_only,
        KittyAdapter::apply_font_only,
    ];
    for family in [
        "Mono\nbackground = ffffff",
        "Mono\rfont_size 99",
        "Mono\0name",
        "Mono\x1b[31m",
        "Mono\u{2028}other",
        "Mono\u{2029}other",
        "",
        "   ",
        &long,
    ] {
        for writer in writers {
            assert!(writer(&env, family).is_err(), "accepted {family:?}");
            assert_eq!(tree_snapshot::tree(home.path()), before);
        }
        assert!(config.set_current_font(family).is_err());
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    // Existing, manually malformed saved state must not leak into theme output
    // or silently select a different fallback font.
    for entry in ["ghostty/config.ghostty", "alacritty/alacritty.toml"] {
        let path = env.xdg_config_home().join(entry);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# user config\n").unwrap();
    }
    fs::write(env.managed_file("current-font"), "Bad\nfont_size 99\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    let themes = ThemeRegistry::new().unwrap();
    for adapter in [
        &GhosttyAdapter as &dyn ToolAdapter,
        &AlacrittyAdapter,
        &KittyAdapter,
    ] {
        assert!(adapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .is_err());
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn kitty_complex_names_are_one_literal_family_argument_in_both_write_paths() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    let themes = ThemeRegistry::new().unwrap();
    let tripwire = home.path().join("executed");
    for family in [
        "auto",
        "Quote\"Mono",
        "O'Connor Mono",
        r"Back\slash Mono",
        "family=Other style=Bold",
        "# Literal Mono",
        r#"Mono$(printf injected > "$FONT_NAME_TRIPWIRE")"#,
    ] {
        KittyAdapter::apply_font_only(&env, family).unwrap();
        config.set_current_font(family).unwrap();
        KittyAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .unwrap();
        for file in ["font.conf", "theme.conf"] {
            let text =
                fs::read_to_string(env.config_dir().join("managed/kitty").join(file)).unwrap();
            let line = text
                .lines()
                .find(|line| line.starts_with("font_family "))
                .unwrap();
            let specification = line.strip_prefix("font_family ").unwrap();
            assert!(specification.starts_with("family="));
            // Kitty 0.36+ uses shell-style tokenization and its own as_setting
            // serializer uses shlex.quote. Exercise that quote boundary with a
            // real shell, not a test decoder mirroring our renderer. This does
            // not claim native Kitty font selection/rendering was exercised.
            let output = Command::new("/bin/bash")
                .env_clear()
                .env("HOME", home.path())
                .env("PATH", "")
                .env("FONT_NAME_TRIPWIRE", &tripwire)
                .args(["--noprofile", "--norc", "-c"])
                .arg(format!("set -- {specification}; printf '%s\\0' \"$@\""))
                .output()
                .unwrap();
            assert!(output.status.success(), "{:?}", output.stderr);
            assert_eq!(output.stdout, format!("family={family}\0").as_bytes());
            assert!(!tripwire.exists());
        }
    }
    for family in ["JetBrainsMono Nerd Font", "字形 Mono", "Fira-Code_2.0"] {
        KittyAdapter::apply_font_only(&env, family).unwrap();
        assert_eq!(
            fs::read_to_string(env.managed_file("managed/kitty/font.conf")).unwrap(),
            format!("font_family {family}\n")
        );
    }
}

#[test]
fn native_ghostty_validates_literal_font_names_when_explicitly_enabled() {
    let Some(binary) = std::env::var_os("SLATE_TEST_GHOSTTY_BIN") else {
        eprintln!("Native Ghostty check skipped; set SLATE_TEST_GHOSTTY_BIN explicitly.");
        return;
    };
    assert!(std::path::Path::new(&binary).is_absolute());
    let home = tempfile::tempdir().unwrap();
    // Ghostty initializes a diagnostic cache even for CLI validation. Give it
    // a separate disposable cache, while checking the whole Slate profile for
    // unexpected writes. Do not use the host's diagnostic/cache directories.
    let native_cache = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    for family in [
        "Quote\"Mono",
        r"Back\slash Mono",
        "O'Connor Mono",
        "字形 Mono",
        "Family=Mono",
        "\"Outer Quotes\"",
    ] {
        GhosttyAdapter::apply_font_only(&env, family).unwrap();
        assert_eq!(
            fs::read_to_string(env.managed_file("managed/ghostty/font.conf")).unwrap(),
            format!("font-family = \"{family}\"\n")
        );
        let before = tree_snapshot::tree(home.path());
        let mut command = assert_cmd::Command::new(&binary);
        command
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", env.xdg_config_home())
            .env("XDG_CACHE_HOME", native_cache.path())
            .env("PATH", "")
            .arg("+validate-config")
            .arg(format!(
                "--config-file={}",
                env.managed_file("managed/ghostty/font.conf").display()
            ))
            .timeout(Duration::from_secs(5))
            .assert()
            .success()
            .stdout(predicates::str::is_empty());
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn font_cli_and_import_select_exact_installed_names_without_punctuation_collisions() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let font_dir = slate_cli::platform::fonts::user_font_dir(&env);
    fs::create_dir_all(&font_dir).unwrap();
    // Header-bearing discovery fixtures only; no native font engine reads them.
    for file in [
        "Twin\"MonoNerdFont-Regular.ttf",
        "TwinMonoNerdFont-Regular.ttf",
    ] {
        fs::write(font_dir.join(file), b"\0\x01\0\0private discovery fixture").unwrap();
    }
    let command = || {
        let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
        command
            .env_clear()
            .env("PATH", "")
            .env("SLATE_HOME", home.path())
            .env("HOME", home.path())
            .arg("--quiet")
            .timeout(Duration::from_secs(5));
        command
    };
    for family in ["TwinMono Nerd Font", "Twin\"Mono Nerd Font"] {
        command().args(["font", family]).assert().success();
        let config = ConfigManager::with_env(&env).unwrap();
        assert_eq!(config.get_current_font().unwrap().as_deref(), Some(family));
        let text = fs::read_to_string(env.managed_file("managed/alacritty/font.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(parsed["font"]["normal"]["family"].as_str(), Some(family));
    }
    let before = tree_snapshot::tree(home.path());
    command()
        .args(["font", "Twin Mono Nerd Font"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("ambiguous alias"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    command()
        .args(["import", "slate://v1/none/TwinMono%20Nerd%20Font/none/none"])
        .assert()
        .success();
    assert_eq!(
        ConfigManager::with_env(&env)
            .unwrap()
            .get_current_font()
            .unwrap()
            .as_deref(),
        Some("TwinMono Nerd Font")
    );
}
