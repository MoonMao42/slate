//! Public picker commit with private external programs, no real picker/desktop.
use slate_cli::{
    cli::set::silent_commit_apply,
    config::{self, ConfigManager},
    detection::{detect_tool_presence_with_env, ToolEvidence},
    env::SlateEnv,
    opacity::OpacityPreset,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

fn program(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
#[ignore = "only run inside a private-profile parent with a deadline"]
fn picker_commit_private_child() {
    let env = SlateEnv::from_process().unwrap();
    let blocked = std::env::var("SLATE_PICKER_COMMIT_CASE").unwrap() == "blocked";
    // Assert the highest-risk native calls resolve to our fixtures before apply.
    for tool in ["bat", "nvim"] {
        let found = detect_tool_presence_with_env(tool, &env);
        assert_eq!(
            found.evidence,
            Some(ToolEvidence::Executable(env.home().join("bin").join(tool)))
        );
    }
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config
        .set_current_opacity_preset(OpacityPreset::Solid)
        .unwrap();
    let result = silent_commit_apply(
        &env,
        "catppuccin-mocha",
        OpacityPreset::Frosted,
        "obsolete-display-name-must-not-be-a-recovery-source",
        OpacityPreset::Clear,
    );
    if blocked {
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("Picker selection failed while applying opacity"),
            "{error}"
        );
        assert!(
            error.contains("Automatic file recovery could not complete"),
            "{error}"
        );
        assert!(
            error.contains("no theme was regenerated as a fallback"),
            "{error}"
        );
        assert!(!error.contains("obsolete-display-name"));
        let points = config::list_restore_points_with_env(&env).unwrap();
        assert_eq!(points.len(), 1);
        assert!(error.contains(&format!("slate restore {} --dry-run", points[0].id)));
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );
    } else {
        result.unwrap();
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Frosted
        );
    }
    assert_eq!(
        config.get_current_theme().unwrap().as_deref(),
        Some("catppuccin-mocha")
    );
    assert_eq!(fs::read(env.home().join("bat.calls")).unwrap(), b"x");
    assert_eq!(fs::read(env.home().join("nvim.calls")).unwrap(), b"x");
}

#[test]
fn picker_commit_public_path_uses_one_theme_apply_and_retains_failed_recovery() {
    for case in ["success", "blocked"] {
        let td = tempfile::tempdir().unwrap();
        for name in [
            "ghostty",
            "alacritty",
            "kitty",
            "starship",
            "batcat",
            "delta",
            "eza",
            "lazygit",
            "fastfetch",
            "tmux",
            "opencode",
            "zsh",
            "brew",
            "defaults",
        ] {
            program(td.path(), name, "exit 0");
        }
        program(
            td.path(),
            "nvim",
            "printf x >> \"$HOME/nvim.calls\"\nprintf 'NVIM v0.8.0\\n'",
        );
        // bat runs after the coordinator's mandatory checkpoint. With no native
        // terminal entry configs, blur.conf is first validated by opacity. The
        // fault is a directory in this test home, never an external target.
        program(td.path(), "bat", "printf x >> \"$HOME/bat.calls\"\nif [ \"$SLATE_PICKER_COMMIT_CASE\" = blocked ]; then /bin/mkdir -p \"$HOME/.config/slate/managed/ghostty/blur.conf\"; fi\nexit 0");
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "picker_commit_private_child",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", td.path().join("bin"))
            .env("NO_COLOR", "1")
            .env("SLATE_PICKER_COMMIT_CASE", case)
            .current_dir(td.path())
            .timeout(Duration::from_secs(12))
            .assert()
            .success();
    }
}
