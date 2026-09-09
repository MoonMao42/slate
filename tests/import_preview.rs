use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod tree_snapshot;
use tree_snapshot::tree;

fn command(home: Option<&Path>) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("PATH", "")
        .timeout(Duration::from_secs(5));
    if let Some(home) = home {
        command.env("SLATE_HOME", home).env("HOME", home);
    }
    command
}

fn preview(home: Option<&Path>, uri: &str) -> serde_json::Value {
    let output = command(home)
        .args(["import", uri, "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.contains(&0x1b));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn import_preview_explains_requested_settings_without_claiming_local_font_availability() {
    let home = TempDir::new().unwrap();
    let before = tree(home.path());
    let uri = "slate://nord/Font-From-Another-Machine/FROSTED/s,f";
    let report = preview(Some(home.path()), uri);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["scope"], "requested_settings");
    assert_eq!(report["theme"], "nord");
    assert_eq!(report["font"], "Font-From-Another-Machine");
    assert_eq!(report["opacity"], "frosted");
    assert_eq!(
        report["tools"],
        serde_json::json!({
            "starship": true, "highlighting": false, "fastfetch": true
        })
    );
    let notes = report["notes"].to_string();
    assert!(notes.contains("not a diff") && notes.contains("not checked"));
    assert!(notes.contains("does not automatically roll back"));
    assert_eq!(
        preview(None, uri),
        report,
        "preview required HOME or local font discovery"
    );
    let output = command(Some(home.path()))
        .env("NVIM_APPNAME", "/invalid")
        .args(["import", uri, "--dry-run"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("no changes made") && text.contains("Zsh highlighting: disable"));
    assert!(!text.contains("Config imported successfully") && !text.contains('\x1b'));
    let unchanged = preview(Some(home.path()), "slate://none/none/none/none");
    for field in ["theme", "font", "opacity"] {
        assert!(unchanged[field].is_null());
    }
    assert_eq!(
        unchanged["tools"],
        serde_json::json!({
            "starship": false, "highlighting": false, "fastfetch": false
        })
    );
    assert_eq!(tree(home.path()), before);
}

#[test]
fn import_preview_ignores_locks_recovery_and_unreadable_settings_without_launching_tools() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    fs::create_dir_all(env.config_dir()).unwrap();
    let path = std::ffi::CString::new(
        env.managed_file("config.toml")
            .as_os_str()
            .as_encoded_bytes(),
    )
    .unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        b"PRIVATE_RECOVERY",
    )
    .unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for name in [
        "fc-list",
        "fc-cache",
        "curl",
        "brew",
        "nvim",
        "osascript",
        "system_profiler",
    ] {
        let path = bin.join(name);
        fs::write(
            &path,
            b"#!/bin/sh\nprintf 'unexpected tool' > \"$HOME/tool-called\"\nexit 97\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = tree(home.path());
    let output = command(Some(home.path()))
        .env("PATH", &bin)
        .args([
            "import",
            "slate://nord/jetbrains-mono/clear/s,h,f",
            "--dry-run",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["font"], "jetbrains-mono");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_RECOVERY"));
    // Real imports remain writers and must still refuse the held lock.
    command(Some(home.path()))
        .args(["import", "slate://none/none/solid/none"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("still running"));
    assert_eq!(tree(home.path()), before);
    drop(guard);
}

#[test]
fn invalid_imports_fail_before_config_sound_and_writer_lock_initialization() {
    let home = TempDir::new().unwrap();
    let before = tree(home.path());
    for uri in [
        "not-a-share-code".to_owned(),
        "slate://none/none/solid".to_owned(),
        "slate://unknown-theme/none/solid/none".to_owned(),
        "slate://nord/jetbrains-mono/invalid/s".to_owned(),
        "slate://nord/jetbrains-mono/solid/s,s".to_owned(),
        "slate://none//none/none".to_owned(),
        "slate://none/none/solid/s,unknown".to_owned(),
        "slate://none/none/solid/s,\x1b[2J".to_owned(),
        format!("slate://none/{}/solid/none", "x".repeat(2048)),
    ] {
        for dry_run in [false, true] {
            let mut command = command(Some(home.path()));
            command.args(["import", &uri]);
            if dry_run {
                command.arg("--dry-run");
            }
            let output = command.assert().failure().get_output().clone();
            assert!(!output.stderr.contains(&0x1b));
            assert!(output.stderr.len() < 1500);
            assert_eq!(tree(home.path()), before, "invalid input wrote files");
        }
    }
    // Font resolution is required for application (not for preview), but a
    // missing font must still fail before creating the profile or writer lock.
    command(Some(home.path()))
        .args(["import", "slate://nord/Definitely-Not-A-Font/solid/none"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
    assert_eq!(tree(home.path()), before);
    command(None)
        .env("NVIM_APPNAME", "/invalid")
        .args(["import", "invalid"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("URI must start with slate://"));
}

#[test]
fn import_preview_flags_and_early_stdout_closure_never_apply_settings() {
    let home = TempDir::new().unwrap();
    let before = tree(home.path());
    let uri = "slate://none/none/frosted/none";
    command(Some(home.path()))
        .args(["import", uri, "--json"])
        .assert()
        .failure()
        .code(2);
    assert_eq!(tree(home.path()), before);

    let (consumer, writer) = UnixStream::pair().unwrap();
    drop(consumer);
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("PATH", "")
        .env("SLATE_HOME", home.path())
        .args(["import", uri, "--dry-run", "--json"])
        .stdout(Stdio::from(std::os::fd::OwnedFd::from(writer)))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("preview did not exit after stdout closed");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "{stderr}");
    assert!(stderr.is_empty());
    assert_eq!(tree(home.path()), before);
}

#[test]
fn import_application_keeps_none_settings_and_disables_omitted_tool_flags() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = slate_cli::config::ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config.set_current_font("Fixture Mono").unwrap();
    config
        .set_current_opacity_preset(slate_cli::opacity::OpacityPreset::Solid)
        .unwrap();
    config.set_starship_enabled(true).unwrap();
    config.set_zsh_highlighting_enabled(true).unwrap();
    config.enable_fastfetch_autorun().unwrap();

    command(Some(home.path()))
        .args(["--quiet", "import", "slate://none/none/frosted/none"])
        .assert()
        .success();
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    assert_eq!(
        config.get_current_font().unwrap().as_deref(),
        Some("Fixture Mono")
    );
    assert_eq!(
        config.get_current_opacity().unwrap().as_deref(),
        Some("frosted")
    );
    assert!(!config.is_starship_enabled().unwrap());
    assert!(!config.is_zsh_highlighting_enabled().unwrap());
    assert!(!config.has_fastfetch_autorun().unwrap());
    assert!(env.managed_file("managed/ghostty/opacity.conf").is_file());
}
