#![cfg(target_os = "macos")]
//! Full share CLI with private HOME/PATH/TMPDIR and fake capture/magick scripts.
//! No real screen, native image tool, system bus or desktop preferences are used.
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};
#[path = "fixtures/share_image.rs"]
mod fixture;

fn program(root: &Path, name: &str, body: &str) {
    let path = root.join("bin").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn profile(root: &Path) {
    fs::create_dir(root.join("tmp")).unwrap();
    fs::write(root.join("fixture.png"), fixture::PNG).unwrap();
    fs::write(root.join("slate-share.png"), b"old capture\n").unwrap();
    program(root, "screencapture", "for arg in \"$@\"; do output=\"$arg\"; done\nprintf '%s\\n' \"$output\" >> \"$HOME/capture.paths\"\ncase \"$SHARE_CAPTURE_CASE\" in\ncancel) exit 0;;\npartial) printf PRIVATE > \"$output\"; exit 1;;\ninvalid) printf PRIVATE > \"$output\"; exit 0;;\nesac\n/bin/cp \"$HOME/fixture.png\" \"$output\"");
    program(root, "magick", "printf x >> \"$HOME/magick.calls\"\ninput=${1#png:}\nfor arg in \"$@\"; do output=${arg#png:}; done\nif [ \"$SHARE_CAPTURE_CASE\" = watermark-failed ]; then printf PRIVATE > \"$input\"; printf PRIVATE > \"$output\"; exit 23; fi\n[ \"$input\" != \"$output\" ] || exit 31\n/bin/cp \"$input\" \"$output\"");
}

fn command(root: &Path, case: &str) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("HOME", root)
        .env("SLATE_HOME", root)
        .env("PATH", root.join("bin"))
        .env("TMPDIR", root.join("tmp"))
        .env("NO_COLOR", "1")
        .env("SHARE_CAPTURE_CASE", case)
        .current_dir(root)
        .arg("share")
        .timeout(Duration::from_secs(15));
    cmd
}

fn preserved(root: &Path) {
    assert_eq!(
        fs::read(root.join("slate-share.png")).unwrap(),
        b"old capture\n"
    );
    assert_eq!(fs::read(root.join("fixture.png")).unwrap(), fixture::PNG);
    assert_eq!(fs::read_dir(root.join("tmp")).unwrap().count(), 0);
    for path in fs::read_to_string(root.join("capture.paths"))
        .unwrap()
        .lines()
    {
        assert!(!Path::new(path).exists());
        assert!(Path::new(path).starts_with(root.join("tmp")));
    }
}

#[test]
fn share_capture_cli_repeated_exports_preserve_old_images_and_watermark_failure() {
    let td = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(td.path()).unwrap();
    profile(&root);
    for (case, index) in [("success", 2), ("watermark-failed", 3)] {
        let output = command(&root, case).assert().success().get_output().clone();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stdout.contains("slate://v1/"));
        assert!(
            stdout.contains(&format!("slate-share-{index}.png")),
            "{stdout}"
        );
        assert_eq!(
            stderr.matches("warning: watermark").count(),
            usize::from(case == "watermark-failed"),
            "{stderr}"
        );
        assert!(!stderr.contains("PRIVATE"));
        let saved = root.join(format!("slate-share-{index}.png"));
        assert_eq!(fs::read(&saved).unwrap(), fixture::PNG);
        assert_eq!(
            fs::metadata(saved).unwrap().permissions().mode() & 0o777,
            0o600
        );
        preserved(&root);
    }
    assert_eq!(fs::read(root.join("magick.calls")).unwrap(), b"xx");
}

#[test]
fn share_capture_cli_cancel_partial_and_invalid_results_never_claim_a_saved_image() {
    for case in ["cancel", "partial", "invalid"] {
        let td = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(td.path()).unwrap();
        profile(&root);
        let assertion = command(&root, case).assert();
        let output = if case == "invalid" {
            assertion.failure()
        } else {
            assertion.success()
        }
        .get_output()
        .clone();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("slate://v1/"));
        assert!(!stdout.contains("Saved to"), "{stdout}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE"));
        assert!(!root.join("slate-share-2.png").exists());
        assert!(!root.join("magick.calls").exists());
        preserved(&root);
    }
}
