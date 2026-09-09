use assert_cmd::Command;
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn isolated(home: &std::path::Path) -> Command {
    let binary = std::env::var_os("SLATE_DOCTOR_TEST_BINARY").unwrap_or_else(|| {
        assert_cmd::cargo::cargo_bin!("slate")
            .as_os_str()
            .to_owned()
    });
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .timeout(std::time::Duration::from_secs(5));
    command
}

#[test]
fn kitty_doctor_checks_continuations_without_running_kitty_or_writing_files() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let home = TempDir::new().unwrap();
    let env = slate_cli::env::SlateEnv::with_home(home.path().to_owned());
    let config = env.xdg_config_home().join("kitty/kitty.conf");
    let managed = env.managed_file("managed/kitty/theme.conf");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::create_dir_all(managed.parent().unwrap()).unwrap();
    fs::create_dir(home.path().join("bin")).unwrap();
    let kitty = home.path().join("bin/kitty");
    fs::write(
        &kitty,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_KITTY\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(&kitty, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&managed, "foreground #ffffff\n").unwrap();
    for (include, connected) in [
        (
            format!(
                "include {}/\n\\theme.conf\n",
                managed.parent().unwrap().display()
            ),
            true,
        ),
        (
            format!("include {}\n\\-different\n", managed.display()),
            false,
        ),
    ] {
        fs::write(
            &config,
            format!(
                "{include}allow_remote_control socket-\n\\only\nlisten_on unix:/fixture/kitty\n"
            ),
        )
        .unwrap();
        let before = tree_snapshot::tree(home.path());
        let output = isolated(home.path())
            .args(["doctor", "kitty", "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let checks = report["checks"].as_array().unwrap();
        assert_eq!(
            checks
                .iter()
                .any(|check| check["message"] == "Slate's loader reference is present"),
            connected
        );
        assert!(!checks
            .iter()
            .any(|check| check["message"] == "Live reload is not confirmed by this file"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn doctor_help_lists_registered_targets_without_loading_a_profile() {
    let home = TempDir::new().unwrap();
    // Help is available even if the profile root cannot be a directory.
    let broken = home.path().join("not-a-directory");
    std::fs::write(&broken, "PRIVATE_CONTENT").unwrap();
    let before = tree_snapshot::tree(home.path());
    for flag in ["--help", "-h"] {
        let output = isolated(&broken)
            .args(["doctor", flag])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("default: ghostty"));
        for target in slate_cli::cli::doctor::TARGETS {
            assert!(text.contains(target), "help omitted {target}: {text}");
        }
        assert!(!text.contains("PRIVATE_CONTENT"));
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn doctor_json_reports_each_target_without_creating_files() {
    let home = TempDir::new().unwrap();
    for target in [
        "kitty",
        "alacritty",
        "nvim",
        "zsh",
        "bash",
        "fish",
        "opencode",
        "opacity",
    ] {
        let output = isolated(home.path())
            .args(["doctor", target, "--json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["target"], target);
        assert!(report.get("font_inventory").is_none());
        assert!(report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["status"] == "warning"));
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }
}

#[test]
fn doctor_alacritty_uses_effective_imports_and_omits_private_parse_errors() {
    let home = TempDir::new().unwrap();
    let env = slate_cli::env::SlateEnv::with_home(home.path().to_owned());
    let path = env.xdg_config_home().join("alacritty/alacritty.toml");
    let managed = env.managed_file("managed/alacritty/colors.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir_all(managed.parent().unwrap()).unwrap();
    std::fs::write(&managed, "# PRIVATE_CONTENT\nnot valid [").unwrap();
    for (content, connected) in [
        (
            format!(
                "import = []\n[general]\nimport = ['{}']\n",
                managed.display()
            ),
            false,
        ),
        (
            format!(
                "import = ['{}']\n[general]\nimport = []\n",
                managed.display()
            ),
            true,
        ),
        (format!("import = ['{}', 42]\n", managed.display()), false),
        ("private_key = 'PRIVATE_CONTENT'\nnot valid [".into(), false),
    ] {
        std::fs::write(&path, content).unwrap();
        let before = tree_snapshot::tree(home.path());
        let output = isolated(home.path())
            .args(["doctor", "alacritty", "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains("PRIVATE_CONTENT"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONTENT"));
        let report: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["message"] == "Slate's loader reference is present"),
            connected
        );
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn doctor_integration_fifo_reads_are_bounded_and_read_only() {
    for (target, config) in [
        ("alacritty", ".config/alacritty/alacritty.toml"),
        ("kitty", ".config/kitty/kitty.conf"),
        ("nvim", ".config/nvim/init.lua"),
        ("zsh", ".zshrc"),
        ("opencode", ".config/opencode/tui.json"),
        ("opacity", ".config/slate/current-opacity"),
    ] {
        let home = TempDir::new().unwrap();
        let path = home.path().join(config);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let before = tree_snapshot::tree(home.path());
        let output = isolated(home.path())
            .args(["doctor", target, "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["status"] == "error"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}
