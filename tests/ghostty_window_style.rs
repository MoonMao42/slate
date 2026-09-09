//! Theme colors must not silently choose the user's native window layout.

use slate_cli::adapter::ghostty::GhosttyAdapter;
use slate_cli::adapter::ToolAdapter;
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    env: SlateEnv,
    entry: PathBuf,
    original_entry: String,
    nested: Option<(PathBuf, String)>,
    managed_theme: PathBuf,
}

fn fixture(style: Option<&str>, nested: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(root.path().to_owned());
    let entry = GhosttyAdapter
        .integration_config_path_with_env(&env)
        .unwrap();
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    let preference = style.map_or_else(String::new, |style| {
        format!("macos-titlebar-style = {style}\n")
    });
    let (original_entry, nested) = if nested {
        let path = entry.parent().unwrap().join("window-style.ghostty");
        let content = format!("# user-owned layout\n{preference}");
        fs::write(&path, &content).unwrap();
        (
            "# user-owned entry\nconfig-file = window-style.ghostty\n".to_owned(),
            Some((path, content)),
        )
    } else {
        (format!("# user-owned entry\n{preference}"), None)
    };
    fs::write(&entry, &original_entry).unwrap();
    let managed_theme = env.config_dir().join("managed/ghostty/theme.conf");
    fs::create_dir_all(managed_theme.parent().unwrap()).unwrap();
    // Upgrade a profile produced by earlier Slate versions, not only an empty
    // profile. The old managed override must disappear when colors regenerate.
    fs::write(&managed_theme, "macos-titlebar-style = transparent\n").unwrap();
    Fixture {
        _root: root,
        env,
        entry,
        original_entry,
        nested,
        managed_theme,
    }
}

fn assert_preferences_untouched(fixture: &Fixture) {
    let entry = fs::read_to_string(&fixture.entry).unwrap();
    assert!(entry.starts_with(&fixture.original_entry));
    if let Some((path, original)) = &fixture.nested {
        assert_eq!(fs::read_to_string(path).unwrap(), *original);
    }
    let managed = fs::read_to_string(&fixture.managed_theme).unwrap();
    assert!(
        !managed.contains("macos-titlebar-style"),
        "theme output overrides a window layout choice: {managed}"
    );
    assert!(!managed.contains("window-decoration"));
}

fn apply(fixture: &Fixture, theme: &str) {
    let registry = ThemeRegistry::new().unwrap();
    let theme = registry.get(theme).unwrap();
    assert!(matches!(
        GhosttyAdapter
            .apply_theme_with_env(theme, &fixture.env)
            .unwrap(),
        slate_cli::adapter::ApplyOutcome::Applied { .. }
    ));
}

#[test]
fn ghostty_window_style_survives_dark_light_switches_and_legacy_output() {
    for style in [
        None,
        Some("native"),
        Some("transparent"),
        Some("tabs"),
        Some("hidden"),
    ] {
        let fixture = fixture(style, false);
        for theme in ["catppuccin-mocha", "catppuccin-latte", "catppuccin-mocha"] {
            apply(&fixture, theme);
            assert_preferences_untouched(&fixture);
        }
        let before = fs::read(&fixture.entry).unwrap();
        apply(&fixture, "catppuccin-mocha");
        assert_eq!(fs::read(&fixture.entry).unwrap(), before);
    }
}

#[test]
fn ghostty_window_style_in_user_include_stays_user_owned() {
    let fixture = fixture(Some("tabs"), true);
    for theme in ["catppuccin-mocha", "catppuccin-latte"] {
        apply(&fixture, theme);
        assert_preferences_untouched(&fixture);
    }
}

#[cfg(target_os = "macos")]
fn validate_native_config(binary: &Path, fixture: &Fixture) {
    // Parser-only: no UI, app reload, shell or user's default config files.
    let output = std::process::Command::new(binary)
        .arg("+validate-config")
        .arg(format!("--config-file={}", fixture.entry.display()))
        .env("HOME", fixture._root.path())
        .env("XDG_CONFIG_HOME", fixture._root.path().join(".config"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Ghostty parser failed ({}): stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "unexpected Ghostty diagnostic output"
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires installed macOS Ghostty; runs its parser only, never its GUI"]
fn ghostty_window_style_native_parser_accepts_preserved_preferences() {
    let binary = Path::new("/Applications/Ghostty.app/Contents/MacOS/ghostty");
    assert!(
        binary.is_file(),
        "install Ghostty before running this opt-in check"
    );
    for nested in [false, true] {
        for style in [
            None,
            Some("native"),
            Some("transparent"),
            Some("tabs"),
            Some("hidden"),
        ] {
            let fixture = fixture(style, nested);
            for (theme, appearance, background) in [
                ("catppuccin-mocha", "dark", "#1e1e2e"),
                ("catppuccin-latte", "light", "#eff1f5"),
            ] {
                apply(&fixture, theme);
                assert_preferences_untouched(&fixture);
                let config = fs::read_to_string(&fixture.managed_theme).unwrap();
                for expected in [
                    format!("window-theme = {appearance}"),
                    format!("background = {background}"),
                ] {
                    assert!(
                        config.lines().any(|line| line == expected),
                        "missing {expected}"
                    );
                }
                validate_native_config(binary, &fixture);
            }
        }
    }
}
