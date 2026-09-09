//! Local developer updates use trusted fixture executables, never host Slate.
#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    time::Duration,
};
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod snapshot;

const OLD: &str = "#!/bin/sh\nprintf 'slate old\\n'\n";
const NEW: &str = "#!/bin/sh\nprintf 'slate new\\n'\n";

struct Fixture {
    home: TempDir,
    candidate: PathBuf,
    bin: PathBuf,
    backups: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let candidate = home.path().join("candidate 中文");
        let bin = home.path().join("local bin");
        let backups = home.path().join("binary backups");
        fs::create_dir(&bin).unwrap();
        fs::write(bin.join("slate"), OLD).unwrap();
        fs::set_permissions(bin.join("slate"), fs::Permissions::from_mode(0o751)).unwrap();
        fs::write(&candidate, NEW).unwrap();
        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            home,
            candidate,
            bin,
            backups,
        }
    }

    fn command(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::new("/bin/bash");
        command
            .env_clear()
            .env("HOME", self.home.path())
            .env("PATH", "/usr/bin:/bin")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/install-dev.sh"
            ))
            .arg(&self.candidate)
            .arg(&self.bin)
            .arg(&self.backups)
            .timeout(Duration::from_secs(8));
        command
    }
}

#[test]
fn developer_install_preserves_old_bytes_and_mode_and_repeat_creates_no_backup() {
    let fixture = Fixture::new();
    fixture.command().assert().success();
    assert_eq!(fs::read(fixture.bin.join("slate")).unwrap(), NEW.as_bytes());
    let backups: Vec<_> = fs::read_dir(&fixture.backups).unwrap().collect();
    assert_eq!(backups.len(), 1);
    let saved = backups[0].as_ref().unwrap().path().join("slate");
    assert_eq!(fs::read(&saved).unwrap(), OLD.as_bytes());
    assert_eq!(
        fs::metadata(saved).unwrap().permissions().mode() & 0o777,
        0o751
    );
    let before = snapshot::tree(fixture.home.path());
    let output = fixture
        .command()
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&output).contains("Already installed:"));
    assert_eq!(snapshot::tree(fixture.home.path()), before);
}

#[test]
fn developer_install_reports_copy_and_rename_failures_without_false_success() {
    for failure in ["copy", "rename_before", "rename_after"] {
        let fixture = Fixture::new();
        let tools = fixture.home.path().join("fixture tools");
        fs::create_dir(&tools).unwrap();
        let (name, body) = match failure {
            "copy" => (
                "cp",
                "#!/bin/sh\nfor target do :; done\nprintf partial > \"$target\"\nexit 41\n",
            ),
            "rename_before" => ("mv", "#!/bin/sh\nexit 42\n"),
            "rename_after" => ("mv", "#!/bin/sh\n/bin/mv \"$@\" || exit 43\nexit 42\n"),
            _ => unreachable!(),
        };
        let stub = tools.join(name);
        fs::write(&stub, body).unwrap();
        fs::set_permissions(stub, fs::Permissions::from_mode(0o755)).unwrap();
        let output = fixture
            .command()
            .env("PATH", format!("{}:/usr/bin:/bin", tools.display()))
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Installed:"));
        let errors = String::from_utf8_lossy(&output.stderr);
        let backup = fs::read_dir(&fixture.backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("slate");
        if failure == "copy" {
            assert!(errors.contains("Backup was not verified"));
            assert!(errors.contains("replacement was not attempted"));
            assert_eq!(fs::read(backup).unwrap(), b"partial");
        } else {
            assert!(errors.contains("Replacement was not confirmed"));
            assert!(errors.contains("Verified previous binary:"));
            assert!(errors.contains("target may already have changed"));
            assert_eq!(fs::read(backup).unwrap(), OLD.as_bytes());
        }
        assert_eq!(
            fs::read(fixture.bin.join("slate")).unwrap(),
            if failure == "rename_after" {
                NEW.as_bytes()
            } else {
                OLD.as_bytes()
            }
        );
        assert_eq!(fs::read_dir(&fixture.bin).unwrap().count(), 1);
    }
}

#[test]
fn developer_install_refuses_failed_candidates_links_busy_writer_and_unusable_backup() {
    for failure in ["version", "candidate_link", "target_link", "busy", "backup"] {
        let mut fixture = Fixture::new();
        match failure {
            "version" => fs::write(&fixture.candidate, "#!/bin/sh\nexit 7\n").unwrap(),
            "candidate_link" => {
                let link = fixture.home.path().join("candidate link");
                symlink(&fixture.candidate, &link).unwrap();
                fixture.candidate = link;
            }
            "target_link" => {
                let outside = fixture.home.path().join("old target");
                fs::rename(fixture.bin.join("slate"), &outside).unwrap();
                symlink(outside, fixture.bin.join("slate")).unwrap();
            }
            "busy" => fs::create_dir(fixture.bin.join(".slate-dev-install.lock")).unwrap(),
            "backup" => fs::write(&fixture.backups, "not a directory").unwrap(),
            _ => unreachable!(),
        }
        let before = snapshot::tree(fixture.home.path());
        fixture.command().assert().failure();
        assert_eq!(snapshot::tree(fixture.home.path()), before, "{failure}");
    }
}
