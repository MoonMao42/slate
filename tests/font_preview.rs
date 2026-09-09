//! Read-only real CLI previews; only a synthetic non-catalog family is ever
//! applied. Private homes, executable tripwires and isolated sessions throughout.
use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::{
    fs,
    os::{
        fd::OwnedFd,
        unix::{
            fs::{symlink, MetadataExt, PermissionsExt},
            net::UnixStream,
        },
    },
    path::Path,
    process::Stdio,
    time::Duration,
};

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod snapshot;
const FAMILY: &str = "SlatePreviewFixture Nerd Font";

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}
fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    write(
        &slate_cli::platform::fonts::user_font_dir(&env)
            .join("SlatePreviewFixtureNerdFont-Regular.ttf"),
        b"\0\x01\0\0private discovery fixture",
    );
    for tool in [
        "curl",
        "brew",
        "fc-cache",
        "fc-list",
        "fc-match",
        "ghostty",
        "kitten",
        "osascript",
    ] {
        let path = env.home().join("bin").join(tool);
        write(
            &path,
            "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nexit 92\n",
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (temp, env)
}
fn command(env: &SlateEnv) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("SLATE_HOME", env.home())
        .env("PATH", env.home().join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(10));
    command
}
fn preview(env: &SlateEnv) -> serde_json::Value {
    let output = command(env)
        .args(["font", FAMILY, "--dry-run", "--json"])
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
    assert!(!text.contains("PRIVATE_CONTENT"));
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["execution_readiness_checked"], false);
    report
}

#[test]
fn font_suggestions_cli_and_json_preview_are_advisory_and_preserve_the_profile() {
    let (_temp, env) = fixture();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let before = snapshot::tree(env.home());
    let request = "SlatPreviewFixture Nerd Font";
    let output = command(&env)
        .args(["font", request, "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["file_plan_complete"], false);
    assert_eq!(report["resolved_family"], serde_json::Value::Null);
    assert_eq!(report["installation"], "not_planned");
    assert!(report["files"].as_array().unwrap().is_empty());
    let reason = report["blocker"]["reason"].as_str().unwrap();
    assert!(
        reason.contains(FAMILY) && reason.contains("observed candidate"),
        "{reason}"
    );
    assert_eq!(snapshot::tree(env.home()), before);
    let output = command(&env)
        .args(["font", request])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains(FAMILY) && error.contains("Suggestions do not select or install"),
        "{error}"
    );
    assert!(!error.contains("Downloading") && !error.contains("Pre-font recovery"));
    assert_eq!(snapshot::tree(env.home()), before);
    assert!(!env.home().join("tool-was-launched").exists());
}

#[test]
fn font_preview_cli_file_actions_match_actual_application_and_identical_repeat() {
    let (_temp, env) = fixture();
    write(&env.managed_file("current-font"), "Old Mono");
    write(
        &env.xdg_config_home().join("alacritty/alacritty.toml"),
        "# PRIVATE_CONTENT\n[font.normal]\nfamily = 'Old Mono'\n",
    );
    let before = snapshot::tree(env.home());
    let report = preview(&env);
    assert_eq!(snapshot::tree(env.home()), before);
    assert_eq!(report["file_plan_complete"], true);
    assert_eq!(report["installation"], "not_requested");
    assert_eq!(report["pre_font_checkpoint"], "would_create");
    assert_eq!(report["terminal_reload"], "session_suppressed");
    let text = command(&env)
        .args(["font", FAMILY, "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&text).contains("Font change preview — no changes made"));
    assert!(!String::from_utf8_lossy(&text).contains("PRIVATE_CONTENT"));
    assert_eq!(snapshot::tree(env.home()), before);
    command(&env)
        .args(["--quiet", "font", FAMILY])
        .assert()
        .success();
    let after = snapshot::tree(env.home());
    let actions = report["files"].as_array().unwrap();
    assert_eq!(
        actions.last().unwrap()["path"],
        env.managed_file("current-font").to_str().unwrap()
    );
    for file in actions {
        let path = Path::new(file["path"].as_str().unwrap());
        match file["action"].as_str().unwrap() {
            "create" => {
                assert!(!before.contains_key(path));
                assert!(after.contains_key(path));
            }
            "update" => {
                assert!(before.contains_key(path));
                assert_ne!(before.get(path), after.get(path));
            }
            "unchanged" => assert_eq!(before.get(path), after.get(path)),
            "preserve_absent" => {
                assert!(!before.contains_key(path) && !after.contains_key(path));
            }
            action => panic!("unexpected action {action}"),
        }
        if let Some(count) = file["after_bytes"].as_u64() {
            assert_eq!(fs::metadata(path).unwrap().len(), count);
        }
    }
    for (path, item) in &after {
        if !fs::symlink_metadata(path).unwrap().is_dir()
            && before.get(path) != Some(item)
            && !path.starts_with(env.slate_cache_dir())
        {
            assert!(
                actions
                    .iter()
                    .any(|file| Path::new(file["path"].as_str().unwrap()) == path),
                "unplanned config write: {}",
                path.display()
            );
        }
    }
    let meta = fs::metadata(env.managed_file("current-font")).unwrap();
    let repeat = preview(&env);
    assert_eq!(repeat["pre_font_checkpoint"], "not_needed");
    assert!(repeat["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|file| matches!(
            file["action"].as_str(),
            Some("unchanged" | "preserve_absent")
        )));
    assert_eq!(snapshot::tree(env.home()), after);
    assert_eq!(
        fs::metadata(env.managed_file("current-font"))
            .unwrap()
            .ino(),
        meta.ino()
    );
    assert!(!env.home().join("tool-was-launched").exists());
}

#[test]
fn font_preview_cli_reads_during_contention_and_leaves_pending_recovery_untouched() {
    let (_temp, env) = fixture();
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT invalid journal",
    );
    let before = snapshot::tree(env.home());
    let report = preview(&env);
    assert_eq!(report["file_plan_complete"], true);
    command(&env)
        .args(["--auto", "--quiet", "font", FAMILY, "--dry-run"])
        .assert()
        .success();
    assert_eq!(snapshot::tree(env.home()), before);
    drop(guard);
    assert_eq!(preview(&env)["file_plan_complete"], true);
    assert_eq!(snapshot::tree(env.home()), before);
}

#[test]
fn font_preview_cli_blockers_are_readonly_content_free_and_never_partial_plans() {
    for issue in ["toml", "fifo", "symlink", "large", "backup"] {
        let (_temp, env) = fixture();
        match issue {
            "toml" => write(
                &env.xdg_config_home().join("alacritty/alacritty.toml"),
                "PRIVATE_CONTENT = [",
            ),
            "backup" => write(&env.slate_cache_dir().join("backups"), "PRIVATE_CONTENT"),
            _ => {
                let path = env.managed_file("managed/shell/env.fish");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                match issue {
                    "fifo" => {
                        let path =
                            std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                    }
                    "symlink" => {
                        write(&env.home().join("original"), "PRIVATE_CONTENT");
                        symlink(env.home().join("original"), path).unwrap();
                    }
                    "large" => fs::File::create(path)
                        .unwrap()
                        .set_len(8 * 1024 * 1024 + 1)
                        .unwrap(),
                    _ => unreachable!(),
                }
            }
        }
        let before = snapshot::tree(env.home());
        let report = preview(&env);
        assert_eq!(report["file_plan_complete"], false, "{issue}");
        assert_eq!(report["blocker"]["stage"], "configuration");
        assert_eq!(report["installation"], "not_planned");
        assert_eq!(report["pre_font_checkpoint"], "not_planned");
        assert!(report["files"].as_array().unwrap().is_empty());
        assert_eq!(snapshot::tree(env.home()), before, "{issue}");
    }
}

#[test]
fn font_preview_cli_partial_inventory_retains_known_family_and_closed_output_is_harmless() {
    use assert_cmd::assert::OutputAssertExt;
    let (_temp, env) = fixture();
    let fonts = slate_cli::platform::fonts::user_font_dir(&env);
    symlink(fonts.join("absent.ttf"), fonts.join("broken.ttf")).unwrap();
    let before = snapshot::tree(env.home());
    let report = preview(&env);
    assert_eq!(report["scan_complete"], false);
    assert_eq!(report["file_plan_complete"], true);
    assert!(!report["scan_issues"].as_array().unwrap().is_empty());
    let (writer, reader) = UnixStream::pair().unwrap();
    drop(reader);
    let fd: OwnedFd = writer.into();
    let mut closed = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    closed
        .env_clear()
        .env("HOME", env.home())
        .env("SLATE_HOME", env.home())
        .env("PATH", env.home().join("bin"))
        .args(["font", FAMILY, "--dry-run", "--json"])
        .stdout(Stdio::from(fd))
        .stderr(Stdio::piped());
    redirected_output::run(&mut closed)
        .assert()
        .success()
        .stderr("");
    assert_eq!(snapshot::tree(env.home()), before);
}

#[test]
fn font_preview_cli_requires_a_named_noninteractive_inspection() {
    let (_temp, env) = fixture();
    let before = snapshot::tree(env.home());
    for args in [
        vec!["font", "--dry-run"],
        vec!["font", FAMILY, "--json"],
        vec!["font", "--dry-run", "--list"],
        vec!["font", FAMILY, "--dry-run", "--list"],
        vec!["font", "bad\nfamily", "--dry-run"],
    ] {
        command(&env).args(args).assert().failure();
        assert_eq!(snapshot::tree(env.home()), before);
    }
    let help = command(&env)
        .args(["font", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&help).contains("--dry-run"));
}
