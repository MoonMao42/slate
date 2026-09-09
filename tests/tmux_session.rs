#![cfg(feature = "has-tmux")]

use slate_cli::adapter::{TmuxAdapter, ToolAdapter};
use slate_cli::cli::theme_apply::ThemeApplyCoordinator;
use slate_cli::config::begin_restore_point_baseline_with_env;
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Server {
    socket: PathBuf,
}

impl Server {
    fn start(socket: PathBuf, config: &Path, home: &Path) -> Self {
        let server = Self { socket };
        let output = server
            .command()
            .arg("-f")
            .arg(config)
            .args(["new-session", "-d", "-s", "slate-test", "/bin/sleep", "60"])
            .env("HOME", home)
            .env("SHELL", "/bin/sh")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        server
    }

    fn command(&self) -> Command {
        let mut command = Command::new("tmux");
        command.arg("-S").arg(&self.socket).env_remove("TMUX");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = self.command().arg("-N").args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn option(&self, name: &str) -> String {
        String::from_utf8(self.run(&["show-options", "-gqv", name]).stdout)
            .unwrap()
            .trim()
            .to_string()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Only this fixture's explicit socket may be stopped.
        let _ = self.command().args(["-N", "kill-server"]).output();
    }
}

#[test]
fn tmux_native_active_window_colors_preserve_layout_and_user_options_across_themes() {
    let td = TempDir::new_in("/tmp").unwrap();
    let server = Server::start(
        td.path().join("palette.sock"),
        Path::new("/dev/null"),
        td.path(),
    );
    server.run(&["set-option", "-g", "status-left", "PRIVATE_STATUS"]);
    server.run(&["set-option", "-g", "prefix", "C-a"]);
    let panes = server
        .run(&[
            "list-panes",
            "-F",
            "#{pane_id}:#{pane_width}:#{pane_height}",
        ])
        .stdout;
    let colors = td.path().join("colors.conf");
    for theme in ThemeRegistry::new().unwrap().all() {
        fs::write(&colors, TmuxAdapter::render_tmux_colors(theme)).unwrap();
        server.run(&["source-file", colors.to_str().unwrap()]);
        let output = server.run(&["show-options", "-gwv", "window-status-current-style"]);
        let style = String::from_utf8(output.stdout).unwrap();
        let fields: Vec<_> = style.trim().split([',', ' ']).collect();
        let fg = fields
            .iter()
            .find_map(|field| field.strip_prefix("fg="))
            .expect("native foreground");
        let bg = fields
            .iter()
            .find_map(|field| field.strip_prefix("bg="))
            .expect("native background");
        assert_eq!(bg, theme.palette.blue);
        assert!(
            slate_cli::wcag::contrast_hex(fg, bg) >= 4.5,
            "{}: {style}",
            theme.id
        );
        assert!(fields.contains(&"bold"));
        let mode =
            String::from_utf8(server.run(&["show-options", "-gwv", "mode-style"]).stdout).unwrap();
        let fields: Vec<_> = mode.trim().split([',', ' ']).collect();
        let fg = fields
            .iter()
            .find_map(|field| field.strip_prefix("fg="))
            .unwrap();
        let bg = fields
            .iter()
            .find_map(|field| field.strip_prefix("bg="))
            .unwrap();
        assert_eq!(bg, theme.palette.black);
        assert!(
            slate_cli::wcag::contrast_hex(fg, bg) >= 4.5,
            "{} mode: {mode}",
            theme.id
        );
        assert_eq!(server.option("status-left"), "PRIVATE_STATUS");
        assert_eq!(server.option("prefix"), "C-a");
        assert_eq!(
            server
                .run(&[
                    "list-panes",
                    "-F",
                    "#{pane_id}:#{pane_width}:#{pane_height}"
                ])
                .stdout,
            panes
        );
    }
}

#[test]
fn tmux_session_reload_targets_the_current_server_without_replaying_user_config() {
    // macOS Unix socket paths must fit within 104 bytes.
    let td = TempDir::new_in("/tmp").unwrap();
    let socket = td.path().join("active.sock");
    let config_root = td.path().join("config 中文 space's [draft] * ? \\");
    let env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(td.path().as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(config_root.as_os_str().to_owned()),
        "TMUX" => Some(format!("{},123,0", socket.display()).into()),
        _ => None,
    })
    .unwrap();
    let config = config_root.join("tmux/tmux.conf");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "set -g @slate-user-marker original\n").unwrap();
    let baseline = begin_restore_point_baseline_with_env(&env).unwrap();
    assert!(baseline
        .entries
        .iter()
        .any(|entry| entry.tool_key == "tmux" && entry.original_path == config));
    let registry = ThemeRegistry::new().unwrap();
    let first = registry.get("catppuccin-mocha").unwrap();
    TmuxAdapter.apply_theme_with_env(first, &env).unwrap();
    assert!(!td.path().join(".tmux.conf").exists());

    let active = Server::start(socket.clone(), &config, td.path());
    assert!(active
        .option("status-style")
        .contains(&first.palette.background));
    assert_eq!(active.option("@slate-user-marker"), "original");
    let other = Server::start(
        td.path().join("other.sock"),
        Path::new("/dev/null"),
        td.path(),
    );
    other.run(&["set-option", "-g", "status-style", "bg=red"]);
    let other_style = other.option("status-style");

    let content = fs::read_to_string(&config)
        .unwrap()
        .replace("@slate-user-marker original", "@slate-user-marker changed");
    fs::write(&config, content).unwrap();
    let selected = registry.get("nord").unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .apply_to_tools(selected, &["tmux".into()])
        .unwrap();
    report.ensure_no_failures().unwrap();
    assert!(
        report.reload_warnings.is_empty(),
        "{:?}",
        report.reload_warnings
    );
    assert!(active
        .option("status-style")
        .contains(&selected.palette.background));
    assert_eq!(
        active.option("@slate-user-marker"),
        "original",
        "user config was replayed"
    );
    assert_eq!(other.option("status-style"), other_style);

    drop(active);
    assert!(
        TmuxAdapter.reload_with_env(&env).is_err(),
        "missing server must be reported"
    );
    let probe = Command::new("tmux")
        .args(["-N", "-S"])
        .arg(socket)
        .arg("list-sessions")
        .output()
        .unwrap();
    assert!(
        !probe.status.success(),
        "theme reload unexpectedly started a server"
    );
}
