use super::*;

#[test]
fn tmux_missing_default_socket_is_narrowly_classified() {
    let missing = b"error connecting to /tmp/private/default (No such file or directory)\n";
    assert!(missing_default_socket(b"", missing));
    assert!(!missing_default_socket(b"partial output", missing));
    for stderr in [
        "error connecting to /tmp/private/default (Permission denied)",
        "error connecting to /tmp/private/default (Connection refused)",
        "source-file: No such file or directory",
        "error connecting to relative (No such file or directory)",
        "other failure\nerror connecting to /tmp/default (No such file or directory)",
    ] {
        assert!(!missing_default_socket(b"", stderr.as_bytes()), "{stderr}");
    }
    assert!(!missing_default_socket(b"", &[0xff]));
}

#[test]
#[ignore = "requires explicit SLATE_TEST_TMUX; uses a nonexistent private socket and never starts a server"]
fn tmux_native_missing_server_is_idle_only_for_default_target() {
    let binary = std::env::var_os("SLATE_TEST_TMUX").expect("explicit native binary");
    let root = tempfile::TempDir::new_in("/tmp").unwrap();
    let socket = root.path().join("missing.sock");
    for default_server in [true, false] {
        let mut command = Command::new(&binary);
        command
            .env_clear()
            .env("LC_ALL", "C")
            .args(["-N", "-S"])
            .arg(&socket)
            .args(["-f", "/dev/null", "source-file", "/dev/null"]);
        let error = run_reload(
            &mut command,
            crate::platform::process_output::Limits {
                timeout: std::time::Duration::from_secs(3),
                max_output: 4096,
            },
            default_server,
        )
        .unwrap_err();
        assert_eq!(
            matches!(error, SlateError::NoDefaultTmuxServer),
            default_server
        );
        assert!(!socket.exists());
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
#[ignore = "private subprocess entry; invoked by tmux_reload_uses_injected_home_fallback_binary"]
fn tmux_reload_fallback_child() {
    assert_eq!(std::env::var("SLATE_TMUX_RELOAD_FIXTURE").unwrap(), "1");
    let env = SlateEnv::from_process().unwrap();
    assert!(!env.session().is_isolated());
    let detected = detection::detect_tool_presence_with_env("tmux", &env);
    assert!(!detected.in_path);
    assert!(TmuxAdapter.is_installed_with_env(&env).unwrap());
    TmuxAdapter.reload_with_env(&env).unwrap();
}

#[test]
fn tmux_reload_uses_injected_home_fallback_binary() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let binary = home.path().join(".local/bin/tmux");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(
        &binary,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/reload-argv\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", home.path().join("empty-path"))
        .env("SLATE_TMUX_RELOAD_FIXTURE", "1")
        .args([
            "--ignored",
            "--exact",
            "adapter::tmux::tests::tmux_reload_fallback_child",
        ])
        .timeout(std::time::Duration::from_secs(8))
        .assert()
        .success();
    let args = std::fs::read_to_string(home.path().join("reload-argv")).unwrap();
    let lines: Vec<_> = args.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "-N");
    assert_eq!(lines[1], "source-file");
    assert!(lines[2].ends_with("/managed/tmux/colors.conf"));
    assert!(!home.path().join(".tmux.conf").exists());
}

#[test]
fn tmux_reload_limits_report_partial_state_without_replaying_native_output() {
    use crate::platform::process_output::Limits;
    use std::time::Duration;
    for (body, expected) in [
        ("printf 'PRIVATE\\033[2J' >&2; exit 7", "exit status: 7"),
        ("while :; do :; done", "deadline"),
        ("while :; do printf PRIVATE_OUTPUT; done", "output limit"),
    ] {
        let mut command = Command::new("/bin/sh");
        command.env_clear().args(["-c", body]);
        let error = run_reload(
            &mut command,
            Limits {
                timeout: Duration::from_millis(100),
                max_output: 1024,
            },
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("Some options may already have applied"));
        assert!(!error.contains("PRIVATE"));
        assert!(!error.contains('\x1b'));
    }
    let mut success = Command::new("/bin/sh");
    success.env_clear().args(["-c", "exit 0"]);
    run_reload(
        &mut success,
        Limits {
            timeout: Duration::from_secs(1),
            max_output: 1024,
        },
        false,
    )
    .unwrap();
}

#[test]
fn tmux_unrepresentable_source_paths_fail_before_config_writes() {
    use std::os::unix::ffi::OsStringExt;
    for name in [
        b"PRIVATE\npath".to_vec(),
        b"PRIVATE\tpath".to_vec(),
        b"PRIVATE\x1bpath".to_vec(),
        b"PRIVATE\xffpath".to_vec(),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(std::ffi::OsString::from_vec(name));
        let error = literal_source_path(&home).unwrap_err().to_string();
        assert!(error.contains("UTF-8 without control characters"));
        assert!(!error.contains("PRIVATE"));
        if home.to_str().is_none() {
            continue;
        } // macOS rejects non-UTF-8 directory names.
        std::fs::create_dir(&home).unwrap();
        let env = SlateEnv::with_home(home.clone());
        let themes = crate::theme::ThemeRegistry::new().unwrap();
        assert!(TmuxAdapter
            .apply_theme_with_env(themes.get("nord").unwrap(), &env)
            .is_err());
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
    }
}

#[test]
fn tmux_invalid_existing_markers_do_not_write_palette_or_initialize_profile() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let path = env.tmux_config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let malformed = format!("{}\nPRIVATE_CONFIG\n", marker_block::START);
    std::fs::write(&path, &malformed).unwrap();
    let theme = crate::theme::ThemeRegistry::new().unwrap();
    assert!(TmuxAdapter
        .apply_theme_with_env(theme.get("nord").unwrap(), &env)
        .is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
    assert!(!env.config_dir().exists());
}

#[test]
fn tmux_palette_sync_preserves_user_config_and_only_initializes_its_fragment() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let path = env.tmux_config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let personal = "set -g prefix C-a\nbind-key r display-message personal\n";
    std::fs::write(&path, personal).unwrap();
    let themes = crate::theme::ThemeRegistry::new().unwrap();
    let theme = themes.get("nord").unwrap();
    TmuxAdapter.apply_theme_with_env(theme, &env).unwrap();
    let managed = env.managed_file("managed/tmux/colors.conf");
    std::fs::set_permissions(&managed, std::fs::Permissions::from_mode(0o640)).unwrap();
    let inode = std::fs::metadata(&managed).unwrap().ino();
    TmuxAdapter.apply_theme_with_env(theme, &env).unwrap();
    assert_eq!(std::fs::metadata(&managed).unwrap().ino(), inode);
    assert_eq!(std::fs::metadata(&managed).unwrap().mode() & 0o777, 0o640);
    assert_eq!(
        marker_block::strip_managed_blocks(&std::fs::read_to_string(path).unwrap()),
        personal
    );
    assert_eq!(std::fs::read_dir(env.config_dir()).unwrap().count(), 1);
    assert!(!env.managed_file("current").exists());
    assert!(!env.managed_file("config.toml").exists());
    assert!(!env.zshrc_path().exists());
}

#[test]
fn tmux_mode_selection_keeps_readable_accents_and_repairs_low_contrast() {
    let mut changed = Vec::new();
    for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
        let fg = readable_foreground(theme, &theme.palette.black, &theme.palette.blue);
        assert!(
            crate::wcag::contrast_hex(fg, &theme.palette.black) >= 4.5,
            "{}",
            theme.id
        );
        if crate::wcag::contrast_hex(&theme.palette.blue, &theme.palette.black) >= 4.5 {
            assert_eq!(fg, theme.palette.blue);
        } else {
            changed.push(theme.id.clone());
        }
        assert!(TmuxAdapter::render_tmux_colors(theme).contains(&format!(
            "set -g mode-style \"bg={} fg={}\"",
            theme.palette.black, fg
        )));
    }
    assert!(!changed.is_empty());
    eprintln!("Corrected mode selection contrast: {changed:?}");
}

#[test]
fn tmux_active_window_text_is_readable_without_changing_other_styles() {
    let mut corrected = Vec::new();
    for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
        let foreground = active_window_foreground(theme);
        let contrast = crate::wcag::contrast_hex(foreground, &theme.palette.blue);
        assert!(contrast >= 4.5, "{} contrast {contrast}", theme.id);
        let previous = crate::wcag::contrast_hex(&theme.palette.foreground, &theme.palette.blue);
        if previous >= 4.5 {
            assert_eq!(foreground, theme.palette.foreground);
        } else {
            corrected.push(theme.id.clone());
        }
        let rendered = TmuxAdapter::render_tmux_colors(theme);
        assert!(rendered.contains(&format!(
            "set -g window-status-current-style \"bg={} fg={} bold\"",
            theme.palette.blue, foreground
        )));
        assert_eq!(
            rendered
                .lines()
                .filter(|line| line.starts_with("set -g "))
                .count(),
            7
        );
        assert!(!rendered.contains("bind-key"));
        assert!(!rendered.contains("status-left"));
    }
    assert!(!corrected.is_empty());
    eprintln!(
        "Improved active-window contrast for {} themes: {corrected:?}",
        corrected.len()
    );
}
use crate::theme::Palette;

fn create_test_palette() -> Palette {
    Palette {
        foreground: "#ffffff".to_string(),
        background: "#000000".to_string(),
        cursor: None,
        selection_bg: None,
        selection_fg: None,
        brand_accent: "#7287fd".to_string(),
        black: "#000000".to_string(),
        red: "#ff0000".to_string(),
        green: "#00ff00".to_string(),
        yellow: "#ffff00".to_string(),
        blue: "#0000ff".to_string(),
        magenta: "#ff00ff".to_string(),
        cyan: "#00ffff".to_string(),
        white: "#ffffff".to_string(),
        bright_black: "#808080".to_string(),
        bright_red: "#ff6b6b".to_string(),
        bright_green: "#69ff69".to_string(),
        bright_yellow: "#ffff69".to_string(),
        bright_blue: "#6b69ff".to_string(),
        bright_magenta: "#ff69ff".to_string(),
        bright_cyan: "#69ffff".to_string(),
        bright_white: "#ffffff".to_string(),
        rosewater: None,
        flamingo: None,
        pink: None,
        mauve: None,
        lavender: None,
        text: None,
        subtext1: None,
        subtext0: None,
        overlay2: None,
        overlay1: None,
        overlay0: None,
        surface2: None,
        surface1: None,
        surface0: None,
        bg_dim: None,
        bg_darker: None,
        bg_darkest: None,
        extras: std::collections::HashMap::new(),
    }
}

fn create_test_theme() -> ThemeVariant {
    ThemeVariant {
        id: "test".to_string(),
        name: "Test Theme".to_string(),
        family: "Test".to_string(),
        palette: create_test_palette(),
        tool_refs: std::collections::HashMap::from([
            ("ghostty".to_string(), "test".to_string()),
            ("alacritty".to_string(), "test".to_string()),
            ("bat".to_string(), "test".to_string()),
            ("delta".to_string(), "test".to_string()),
            ("starship".to_string(), "test".to_string()),
            ("eza".to_string(), "test".to_string()),
            ("lazygit".to_string(), "test".to_string()),
            ("fastfetch".to_string(), "test".to_string()),
            ("tmux".to_string(), "test".to_string()),
            ("zsh_syntax_highlighting".to_string(), "test".to_string()),
        ]),
        appearance: crate::theme::ThemeAppearance::Dark,
        auto_pair: None,
    }
}

#[test]
fn test_tool_name() {
    let adapter = TmuxAdapter;
    assert_eq!(adapter.tool_name(), "tmux");
}

#[test]
fn test_apply_strategy() {
    let adapter = TmuxAdapter;
    assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
}

#[test]
fn test_render_tmux_colors() {
    let theme = create_test_theme();
    let output = TmuxAdapter::render_tmux_colors(&theme);

    // Verify all 7 elements are present
    assert!(output.contains("status-style"));
    assert!(output.contains("window-status-current-style"));
    assert!(output.contains("pane-border-style"));
    assert!(output.contains("pane-active-border-style"));
    assert!(output.contains("message-style"));
    assert!(output.contains("mode-style"));
    assert!(output.contains("message-command-style"));

    // Verify color values
    assert!(output.contains("#000000")); // background
    assert!(output.contains("#ffffff")); // foreground

    // Verify 7 set -g directives (count)
    let count = output.matches("set -g").count();
    assert_eq!(count, 7, "Expected 7 set -g directives, found {}", count);
}

#[test]
fn test_render_tmux_block() {
    let managed_path = PathBuf::from("/home/user/.config/slate/managed/tmux/colors.conf");
    let output = TmuxAdapter::render_tmux_block(&managed_path).unwrap();

    marker_block::validate_block_state(&output).unwrap();
    assert!(marker_block::strip_managed_blocks(&output).is_empty());
    assert!(output.contains(marker_block::START));
    assert!(output.contains(marker_block::END));
    assert!(output.contains("source-file"));
    assert!(output.contains(".config/slate/managed/tmux/colors.conf"));
}

#[test]
fn test_is_installed() {
    let adapter = TmuxAdapter;
    let _result = adapter.is_installed();
}
