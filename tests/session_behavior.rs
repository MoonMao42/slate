use assert_cmd::Command;
use slate_cli::detection::{TerminalKind, TerminalProfile};
use slate_cli::session::SessionContext;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[test]
fn ssh_and_tmux_capabilities_describe_the_actual_session() {
    let remote = SessionContext::from_vars(|key| match key {
        "SSH_CONNECTION" => Some("client connection".into()),
        _ => None,
    });
    let profile = TerminalProfile::from_env_vars(Some("ghostty"), Some("xterm-256color"))
        .with_session(remote);
    assert!(profile.display_name().contains("SSH"));
    assert!(!profile.supports_opacity() && !profile.supports_blur());
    assert!(profile.font_selection_is_manual());
    assert!(!profile.watcher_shell_autostart_supported());
    assert!(profile.feature_summary().live_preview.contains("inline"));
    assert_eq!(
        slate_cli::platform::capabilities::terminal_capability_report(&profile).backend,
        "ssh"
    );

    let local_tmux =
        SessionContext::from_vars(|key| (key == "TMUX").then(|| "/tmp/session,123,0".into()));
    let profile = TerminalProfile::from_env_vars(Some("tmux"), Some("tmux-256color"))
        .with_session(local_tmux);
    assert_eq!(profile.kind(), TerminalKind::Unknown);
    assert_eq!(profile.compatibility_label(), "tmux session");
    assert!(profile.feature_summary().reload.contains("tmux server"));
}

#[test]
fn partial_theme_failure_returns_failure_instead_of_a_success_message() {
    let td = TempDir::new().unwrap();
    let bin = td.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let executable = bin.join("alacritty");
    fs::write(&executable, "#!/bin/sh\nprintf 'alacritty 0.15.1\\n'\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let config = td.path().join(".config/alacritty/alacritty.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "[broken TOML\n").unwrap();
    for args in [
        &["--quiet", "theme", "catppuccin-mocha"][..],
        &["theme", "catppuccin-mocha"][..],
    ] {
        let output = Command::cargo_bin("slate")
            .unwrap()
            .env("SLATE_HOME", td.path())
            .env("PATH", &bin)
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("incomplete") && stderr.contains("alacritty"),
            "{stderr}"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Theme switched"));
        assert_eq!(fs::read_to_string(&config).unwrap(), "[broken TOML\n");
    }
}
