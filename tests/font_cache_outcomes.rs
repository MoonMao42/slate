//! Real CLI with a private, non-catalog filename fixture. It cannot fall back to
//! a real catalog install if discovery fails. No native font engine/cache runs.
use slate_cli::{config::ConfigManager, env::SlateEnv};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

#[test]
fn font_cache_existing_font_cli_never_claims_a_refresh_or_launches_installers() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let font_dir = slate_cli::platform::fonts::user_font_dir(&env);
    fs::create_dir_all(&font_dir).unwrap();
    let bytes = b"\0\x01\0\0private-font-discovery-fixture";
    let font_file = font_dir.join("SlateCacheFixtureNerdFont-Regular.ttf");
    fs::write(&font_file, bytes).unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for name in ["fc-cache", "curl", "brew"] {
        let path = bin.join(name);
        fs::write(
            &path,
            "#!/bin/sh\nprintf 'unexpected' > \"$HOME/native-command-ran\"\nexit 90\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = assert_cmd::Command::cargo_bin("slate")
        .unwrap()
        .env_clear()
        .env("HOME", temp.path())
        .env("SLATE_HOME", temp.path())
        .env("PATH", bin)
        .env("SHELL", "/bin/bash")
        .env("NO_COLOR", "1")
        .args(["font", "SlateCacheFixture Nerd Font"])
        .timeout(Duration::from_secs(10))
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("No font-cache refresh was requested"),
        "{stdout}\n{stderr}"
    );
    assert!(!stdout.contains("completed successfully") && !stdout.contains("Slate refreshed"));
    assert!(!stderr.contains("Downloading") && !stderr.contains("downloaded"));
    assert!(!temp.path().join("native-command-ran").exists());
    assert_eq!(fs::read(font_file).unwrap(), bytes);
    assert_eq!(
        ConfigManager::with_env(&env)
            .unwrap()
            .get_current_font()
            .unwrap()
            .as_deref(),
        Some("SlateCacheFixture Nerd Font")
    );
}
