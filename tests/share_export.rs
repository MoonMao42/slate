use slate_cli::{
    config::{ConfigManager, ConfigWriteGuard},
    env::SlateEnv,
    opacity::OpacityPreset,
};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod tree_snapshot;
use tree_snapshot::tree;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("PATH", "")
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(5));
    command
}

fn export(home: &Path) -> String {
    let output = command(home)
        .args(["export", "--raw"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let output = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.lines().count(), 1);
    assert!(output.ends_with('\n') && output.is_ascii() && !output.contains('\x1b'));
    output.trim_end().to_owned()
}

fn preview(home: &Path, uri: &str) -> serde_json::Value {
    let output = command(home)
        .args(["import", uri, "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}

fn config(home: &Path) -> ConfigManager {
    let env = SlateEnv::with_home(home.to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config
        .set_current_opacity_preset(OpacityPreset::Frosted)
        .unwrap();
    config.set_starship_enabled(true).unwrap();
    config.set_zsh_highlighting_enabled(false).unwrap();
    config.enable_fastfetch_autorun().unwrap();
    config
}

#[test]
fn share_export_round_trips_font_text_without_changing_legacy_percent_semantics() {
    let home = TempDir::new().unwrap();
    let config = config(home.path());
    for font in [
        "JetBrains Mono Nerd Font",
        "字体 / Mono %20 + 'quoted'",
        "none",
        "%2F Family",
    ] {
        config.set_current_font(font).unwrap();
        let before = tree(home.path());
        let uri = export(home.path());
        assert!(uri.starts_with("slate://v1/nord/"));
        let report = preview(home.path(), &uri);
        assert_eq!(report["font"], font);
        assert_eq!(report["theme"], "nord");
        assert_eq!(report["opacity"], "frosted");
        assert_eq!(
            report["tools"],
            serde_json::json!({"starship":true,"highlighting":false,"fastfetch":true})
        );
        assert_eq!(tree(home.path()), before);
    }
    let maximum_font = "字".repeat(85); // 255 UTF-8 bytes before percent-encoding.
    config.set_current_font(&maximum_font).unwrap();
    assert_eq!(
        preview(home.path(), &export(home.path()))["font"],
        maximum_font
    );
    for (uri, expected) in [
        ("slate://nord/Family%20Mono/solid/s", "Family%20Mono"),
        ("slate://v1/nord/Family%20Mono/solid/s", "Family Mono"),
        ("slate://v1/nord/Family%2520Mono/solid/s", "Family%20Mono"),
        ("slate://nord/Family+Mono/solid/s", "Family+Mono"),
        ("slate://v1/nord/Family%2bMono/solid/s", "Family+Mono"),
    ] {
        assert_eq!(preview(home.path(), uri)["font"], expected);
    }
}

#[test]
fn share_export_remains_read_only_for_empty_profiles_and_active_or_pending_writers() {
    let home = TempDir::new().unwrap();
    let before = tree(home.path());
    assert_eq!(export(home.path()), "slate://v1/none/none/none/s,h");
    assert_eq!(tree(home.path()), before);

    let config = config(home.path());
    config.set_current_font("Private Fixture Font").unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        b"PRIVATE_RECOVERY_RECORD",
    )
    .unwrap();
    let before = tree(home.path());
    let expected = export(home.path());
    for args in [&["export"][..], &["export", "--raw"][..]] {
        let output = command(home.path())
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(&expected) && !text.contains("PRIVATE_RECOVERY_RECORD"));
        assert_eq!(tree(home.path()), before);
    }
    drop(guard);
    assert_eq!(export(home.path()), expected);
    assert_eq!(tree(home.path()), before);
}

#[test]
fn share_export_rejects_unsafe_or_invalid_sources_without_disclosing_their_contents() {
    for (file, kind) in [
        ("current", "fifo"),
        ("current-font", "fifo"),
        ("current-opacity", "fifo"),
        ("config.toml", "fifo"),
        ("autorun-fastfetch", "fifo"),
        ("current-font", "symlink"),
        ("current-font", "oversized"),
        ("current-font", "utf8"),
        ("current-font", "control"),
        ("current", "unknown"),
        ("current", "sentinel"),
        ("current-opacity", "unknown"),
        ("current-opacity", "sentinel"),
        ("config.toml", "directory"),
        ("config.toml", "oversized"),
        ("config.toml", "syntax"),
        ("config.toml", "bad-tools"),
        ("config.toml", "bad-flag"),
        ("autorun-fastfetch", "symlink"),
    ] {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        let path = env.managed_file(file);
        let private = home.path().join("private-file");
        fs::write(&private, b"PRIVATE_SECRET").unwrap();
        match kind {
            "fifo" => {
                let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "symlink" => symlink(&private, &path).unwrap(),
            "directory" => fs::create_dir(&path).unwrap(),
            "oversized" => fs::write(
                &path,
                vec![b'x'; if file == "config.toml" { 262145 } else { 4097 }],
            )
            .unwrap(),
            "utf8" => fs::write(&path, [255, 254]).unwrap(),
            "control" => fs::write(&path, b"PRIVATE_SECRET\x1b[2J").unwrap(),
            "unknown" => fs::write(&path, b"PRIVATE_SECRET").unwrap(),
            "sentinel" => fs::write(&path, b"none").unwrap(),
            "syntax" => fs::write(&path, b"[PRIVATE_SECRET").unwrap(),
            "bad-tools" => fs::write(&path, b"tools = 'PRIVATE_SECRET'\n").unwrap(),
            "bad-flag" => fs::write(&path, b"[tools]\nstarship = 'PRIVATE_SECRET'\n").unwrap(),
            _ => unreachable!(),
        }
        let before = tree(home.path());
        let output = command(home.path())
            .args(["export", "--raw"])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("Cannot export"), "{file}/{kind}: {error}");
        assert!(!error.contains("PRIVATE_SECRET") && !error.contains('\x1b'));
        assert_eq!(tree(home.path()), before, "{file}/{kind} changed files");
    }
}

#[test]
fn share_export_printed_preview_command_passes_literal_arguments_to_the_shell() {
    let home = TempDir::new().unwrap();
    let config = config(home.path());
    config
        .set_current_font(r#"Mono$(printf injected > "$HOME/injected")' `printf no` / 100%"#)
        .unwrap();
    let before = tree(home.path());
    let uri = export(home.path());
    assert!(!uri.contains(['\'', '"', '$', '`', ' ', '(', ')']));
    let output = command(home.path())
        .arg("export")
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("slate import "))
        .unwrap();
    assert_eq!(tree(home.path()), before);
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let stub = bin.join("slate");
    fs::write(
        &stub,
        b"#!/bin/sh\nprintf '%s\\n' \"$#\" \"$@\" > \"$CAPTURE\"\n",
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let capture = home.path().join("arguments");
    assert_cmd::Command::new("/bin/sh")
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", home.path())
        .env("CAPTURE", &capture)
        .args(["-c", line])
        .timeout(Duration::from_secs(5))
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        format!("3\nimport\n{uri}\n--dry-run\n")
    );
    assert!(!home.path().join("injected").exists());
}

#[test]
fn share_v1_rejects_malformed_encoding_and_decoded_controls_before_writing() {
    let home = TempDir::new().unwrap();
    let before = tree(home.path());
    for uri in [
        "slate://v1/nord/Mono%/solid/s",
        "slate://v1/nord/Mono%0/solid/s",
        "slate://v1/nord/Mono%GG/solid/s",
        "slate://v1/nord/Mono%FF/solid/s",
        "slate://v1/nord/Mono%00/solid/s",
        "slate://v1/nord/Mono%1B/solid/s",
        "slate://v1/nord/Mono%0A/solid/s",
        "slate://v1/nord/Mono Font/solid/s",
        "slate://v1/nord/Mono+Font/solid/s",
        "slate://v2/nord/Mono/solid/s",
    ] {
        for preview in [false, true] {
            let mut command = command(home.path());
            command.args(["import", uri]);
            if preview {
                command.arg("--dry-run");
            }
            let output = command.assert().failure().get_output().clone();
            assert!(!output.stderr.contains(&0x1b));
            assert_eq!(tree(home.path()), before);
        }
    }
}
