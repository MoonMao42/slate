//! Read-only production CLI cases. Confirmed mutation is tested with injected
//! installers in unit tests, never with a real package manager on this host.
use slate_cli::env::SlateEnv;
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

#[path = "support/tree.rs"]
mod tree_snapshot;

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.user_local_bin()).unwrap();
    for id in ["btop", "brew", "apt-get", "sudo", "curl"] {
        let path = env.user_local_bin().join(id);
        fs::write(
            &path,
            "#!/bin/sh\nprintf called > \"$SLATE_HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
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
        .timeout(Duration::from_secs(5));
    command
}

#[test]
fn install_preview_and_unconfirmed_request_are_readonly_without_a_saved_theme() {
    let (home, env) = fixture();
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_RECOVERY",
    )
    .unwrap();
    let before = tree_snapshot::tree(home.path());
    for id in ["btop", "starship", "yazi"] {
        let output = command(&env)
            .args(["tools", "install", id, "--dry-run", "--json"])
            .output()
            .unwrap();
        if !output.status.success() {
            // Yazi intentionally has no automatic apt route.
            assert!(cfg!(target_os = "linux") && id == "yazi");
            assert!(String::from_utf8_lossy(&output.stderr).contains("no apt mapping"));
            continue;
        }
        let review: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(review["schema_version"], 1);
        assert_eq!(review["tool"], id);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_RECOVERY"));
        let unconfirmed = command(&env)
            .args(["tools", "install", id])
            .output()
            .unwrap();
        if review["action"] == "already_detected" {
            assert!(unconfirmed.status.success());
            assert!(String::from_utf8_lossy(&unconfirmed.stdout)
                .contains("No installation or upgrade will be attempted"));
        } else {
            assert_eq!(review["action"], "install_missing");
            assert!(review["route"].is_string());
            assert!(!unconfirmed.status.success());
            assert!(String::from_utf8_lossy(&unconfirmed.stderr).contains("requires --yes"));
        }
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn install_detected_tool_with_yes_never_reinstalls_or_creates_a_writer() {
    let (home, env) = fixture();
    let before = tree_snapshot::tree(home.path());
    command(&env)
        .args(["tools", "install", "btop", "--yes"])
        .assert()
        .success();
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert!(!env.slate_cache_dir().exists());
}

#[test]
fn install_invalid_ids_and_flag_combinations_fail_before_profile_or_installer_io() {
    let (home, env) = fixture();
    let before = tree_snapshot::tree(home.path());
    for args in [
        vec!["tools", "install", "ghostty", "--yes"],
        vec!["tools", "install", "nvim", "--yes"],
        vec!["tools", "install", "BAD\x1b[31m\n", "--yes"],
        vec!["tools", "install", "btop", "--json"],
        vec!["tools", "install", "btop", "--dry-run", "--yes"],
    ] {
        for with_home in [true, false] {
            let mut command = command(&env);
            if !with_home {
                command.env_remove("HOME").env_remove("SLATE_HOME");
            }
            let output = command.args(&args).assert().failure().get_output().clone();
            assert!(!output.stderr.contains(&0x1b));
        }
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}
