//! Prompt layout changes use disposable profiles. Preview must never execute
//! Starship or the custom commands inside a user's configuration.
use slate_cli::{
    config::{get_restore_point_with_env, list_restore_points_with_env},
    env::SlateEnv,
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    time::Duration,
};

#[path = "support/tree.rs"]
mod tree_snapshot;

fn seed(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    seed(&env.managed_file("current"), "nord\n");
    seed(
        &env.managed_file("config.toml"),
        "# Keep\n[tools]\nstarship=false\n",
    );
    seed(&env.xdg_config_home().join("starship.toml"), "# My prompt\nformat = '$directory'\n[custom.keep]\ncommand = 'echo fg:crust'\nwhen = false\n");
    let trap = env.user_local_bin().join("starship");
    seed(
        &trap,
        "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(trap, fs::Permissions::from_mode(0o755)).unwrap();
    (home, env)
}

fn command(env: &SlateEnv) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("SLATE_HOME", env.home())
        .env("PATH", env.user_local_bin())
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(6));
    command
}

#[test]
fn localized_catalog_does_not_read_broken_preferences_or_run_tools() {
    let (home, env) = fixture();
    seed(&env.managed_file("config.toml"), "[broken");
    let before = tree_snapshot::tree(home.path());
    for (language, title) in [("zh-CN", "提示符样式"), ("en", "Prompt layouts")] {
        let output = command(&env)
            .args(["prompt", "--list", "--language", language])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with(title), "{text}");
        for style in slate_cli::config::prompt::PromptStyle::ALL {
            assert!(text.contains(&format!("{} — ", style.id())));
            assert!(text.contains(style.sample()));
        }
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    command(&env)
        .args(["prompt", "focus", "--yes", "--language", "zh-CN"])
        .assert()
        .failure();
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn prompt_catalog_and_change_review_are_read_only_without_native_probes() {
    let (home, env) = fixture();
    let before = tree_snapshot::tree(home.path());
    let catalog = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .args(["prompt", "--list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let catalog: serde_json::Value = serde_json::from_slice(&catalog).unwrap();
    assert_eq!(
        catalog["styles"].as_array().unwrap().len(),
        slate_cli::config::prompt::PromptStyle::ALL.len()
    );
    for args in [
        vec!["prompt"],
        vec!["prompt", "--list"],
        vec!["prompt", "compact", "--dry-run"],
    ] {
        command(&env).args(args).assert().success();
    }
    let preview = command(&env)
        .args(["prompt", "minimal", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
    assert_eq!(preview["style"], "minimal");
    assert_eq!(preview["theme"], "nord");
    assert_eq!(preview["changes"].as_array().unwrap().len(), 3);
    command(&env).args(["prompt", "compact"]).assert().code(1);
    for args in [
        vec!["prompt", "unknown"],
        vec!["prompt", "--yes"],
        vec!["prompt", "compact", "--json"],
        vec!["prompt", "compact", "--dry-run", "--yes"],
    ] {
        command(&env).args(args).assert().failure();
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn prompt_cli_saves_all_layouts_without_changing_activation_theme_or_other_tools() {
    let (home, env) = fixture();
    let primary = env.xdg_config_home().join("starship.toml");
    fs::set_permissions(&primary, fs::Permissions::from_mode(0o640)).unwrap();
    let original = fs::read(&primary).unwrap();
    let mut first_point = None;
    let styles = slate_cli::config::prompt::PromptStyle::ALL;
    for id in styles.map(|style| style.id()) {
        command(&env)
            .args(["prompt", id, "--yes"])
            .assert()
            .success();
        if first_point.is_none() {
            first_point = Some(list_restore_points_with_env(&env).unwrap()[0].id.clone());
        }
        let status = command(&env)
            .args(["status", "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let status: serde_json::Value = serde_json::from_slice(&status).unwrap();
        assert_eq!(status["prompt_style"], id);
        let config = fs::read_to_string(env.managed_file("config.toml")).unwrap();
        assert!(config.contains("starship=false"), "{config}");
        let doc: toml_edit::DocumentMut = fs::read_to_string(&primary).unwrap().parse().unwrap();
        assert_eq!(
            doc["custom"]["keep"]["command"].as_str(),
            Some("echo fg:crust")
        );
        assert_eq!(
            doc["format"].as_str().unwrap().contains("$line_break"),
            !matches!(id, "compact" | "focus" | "branch")
        );
        assert_eq!(
            fs::metadata(&primary).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read(env.managed_file("current")).unwrap(), b"nord\n");
        assert!(!env.managed_file("managed/shell").exists());
        assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
    }
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), styles.len());
    let inode = fs::metadata(&primary).unwrap().ino();
    command(&env)
        .args(["prompt", styles.last().unwrap().id(), "--yes"])
        .assert()
        .success();
    assert_eq!(fs::metadata(&primary).unwrap().ino(), inode);
    assert_eq!(
        list_restore_points_with_env(&env).unwrap().len(),
        styles.len()
    );
    // Restore the point actually returned by the first operation. Captures can
    // share a timestamp; inventory tie order is not an operation sequence.
    let earliest = first_point.unwrap();
    assert_eq!(
        get_restore_point_with_env(&env, &earliest)
            .unwrap()
            .entries
            .len(),
        3
    );
    assert!(slate_cli::config::execute_restore_with_env(&env, &earliest)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(primary).unwrap(), original);
}

#[test]
fn prompt_invalid_sources_and_backup_paths_fail_before_configuration_writes() {
    for kind in [
        "toml",
        "utf8",
        "fifo",
        "symlink",
        "backup",
        "theme",
        "preferences",
    ] {
        let (home, env) = fixture();
        let primary = env.xdg_config_home().join("starship.toml");
        match kind {
            "toml" => seed(&primary, "[PRIVATE_INVALID\n"),
            "utf8" => seed(&primary, [255, 254]),
            "fifo" => {
                fs::remove_file(&primary).unwrap();
                let path = std::ffi::CString::new(primary.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "symlink" => {
                fs::remove_file(&primary).unwrap();
                std::os::unix::fs::symlink(home.path().join("missing"), &primary).unwrap();
            }
            "backup" => seed(&env.slate_cache_dir().join("backups"), "PRIVATE_INVALID"),
            "theme" => seed(&env.managed_file("current"), "PRIVATE_INVALID"),
            "preferences" => seed(&env.managed_file("config.toml"), "[PRIVATE_INVALID\n"),
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(home.path());
        let output = command(&env)
            .args(["prompt", "minimal", "--yes"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_INVALID"));
        assert_eq!(tree_snapshot::tree(home.path()), before, "{kind}");
    }
}

#[test]
#[ignore = "explicit local Starship path required; only renders disposable profiles"]
fn classic_native_prompt_distinguishes_ssh_and_command_status_without_icon_fonts() {
    let binary =
        std::env::var_os("SLATE_STARSHIP_BINARY").expect("provide the native binary explicitly");
    let (home, env) = fixture();
    let directory = home.path().join("demo-project");
    fs::create_dir(&directory).unwrap();
    seed(&env.xdg_config_home().join("starship.toml"), "");
    let native_command = || {
        let mut native = assert_cmd::Command::new(&binary);
        native
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", "")
            .env(
                "STARSHIP_CONFIG",
                env.xdg_config_home().join("starship.toml"),
            )
            .env("STARSHIP_CACHE", home.path().join("starship-cache"))
            // Fish consumes rendered text, unlike Bash's escaped PS1 string.
            .env("STARSHIP_SHELL", "fish")
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .current_dir(&directory)
            .timeout(Duration::from_secs(5));
        native
    };
    for theme in ["nord", "catppuccin-latte"] {
        seed(&env.managed_file("current"), theme);
        command(&env)
            .args(["prompt", "classic", "--yes"])
            .assert()
            .success();
        // Starship resolves the OS user; USER is not a portable override. Read
        // the independent module, using only the same disposable config.
        let username = native_command()
            .args(["module", "username"])
            .assert()
            .success()
            .stderr("")
            .get_output()
            .stdout
            .clone();
        let username = String::from_utf8(username).unwrap();
        let username = console::strip_ansi_codes(&username);
        assert!(!username.trim().is_empty());
        for (ssh, status) in [(false, "0"), (true, "0"), (false, "1"), (true, "1")] {
            let mut native = native_command();
            if ssh {
                // Only environment detection, no SSH client or network connection.
                native.env("SSH_CONNECTION", "127.0.0.1 12345 127.0.0.1 22");
            }
            let output = native
                .args(["prompt", "--status", status, "--terminal-width", "120"])
                .timeout(Duration::from_secs(5))
                .assert()
                .success()
                .get_output()
                .clone();
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let colored = String::from_utf8(output.stdout).unwrap();
            let text = console::strip_ansi_codes(&colored);
            assert!(text.trim_start().starts_with(username.trim()), "{text:?}");
            assert!(text.contains("demo-project"));
            assert_eq!(text.contains('@'), ssh, "{text:?}");
            assert!(text.trim_end().ends_with('$'), "{text:?}");
            assert_eq!(text.trim().lines().count(), 2, "{text:?}");
            // Host/user names may be Unicode; only the preset's own symbols
            // are ASCII (also checked across all palettes in the unit tests).
            assert_eq!(text.trim().lines().last(), Some("$"));
            let themes = slate_cli::theme::ThemeRegistry::new().unwrap();
            let palette = &themes.get(theme).unwrap().palette;
            let color = if status == "0" {
                &palette.green
            } else {
                &palette.red
            };
            let (r, g, b) =
                slate_cli::adapter::palette_renderer::PaletteRenderer::hex_to_rgb(color).unwrap();
            assert!(
                colored.contains(&format!("38;2;{r};{g};{b}m$")),
                "status {status} should recolor the literal dollar: {colored:?}"
            );
        }
    }
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
}

#[test]
#[ignore = "explicit local Starship path required; never loads a real user profile"]
fn prompt_native_starship_renders_all_layouts() {
    let binary =
        std::env::var_os("SLATE_STARSHIP_BINARY").expect("provide the native binary explicitly");
    let (home, env) = fixture();
    let directory = home.path().join("demo-project");
    fs::create_dir(&directory).unwrap();
    // No personal custom modules enter the native renderer.
    seed(&env.xdg_config_home().join("starship.toml"), "");
    for theme in ["nord", "catppuccin-latte"] {
        seed(&env.managed_file("current"), theme);
        for style in slate_cli::config::prompt::PromptStyle::ALL.map(|style| style.id()) {
            command(&env)
                .args(["prompt", style, "--yes"])
                .assert()
                .success();
            let output = assert_cmd::Command::new(&binary)
                .env_clear()
                .env("HOME", home.path())
                .env("PATH", "")
                .env(
                    "STARSHIP_CONFIG",
                    env.xdg_config_home().join("starship.toml"),
                )
                .env("STARSHIP_CACHE", home.path().join("starship-cache"))
                .env("STARSHIP_SHELL", "bash")
                .env("TERM", "xterm-256color")
                .env("USER", "demo")
                .env("LANG", "en_US.UTF-8")
                .current_dir(&directory)
                .args(["prompt", "--status", "0", "--terminal-width", "120"])
                .timeout(Duration::from_secs(5))
                .assert()
                .success()
                .get_output()
                .clone();
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains("demo-project"), "{text:?}");
            assert_eq!(
                text.contains('\n'),
                !matches!(style, "compact" | "focus"),
                "{style}: {text:?}"
            );
            assert!(
                text.contains("\x1b["),
                "{style} should render palette colors"
            );
            let registry = slate_cli::theme::ThemeRegistry::new().unwrap();
            let palette = &registry.get(theme).unwrap().palette;
            let (channel, color) = if style == "rainbow" {
                (48, &palette.red)
            } else {
                (38, &palette.blue)
            };
            let (r, g, b) =
                slate_cli::adapter::palette_renderer::PaletteRenderer::hex_to_rgb(color).unwrap();
            assert!(
                text.contains(&format!("{channel};2;{r};{g};{b}")),
                "{style} must use {theme}'s actual palette, not default ANSI colors"
            );
        }
    }
}
