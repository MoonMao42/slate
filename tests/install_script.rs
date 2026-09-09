//! Offline installer checks: all downloads and install targets are private fixtures.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

const ASSET: &str = "slate-cli-x86_64-unknown-linux-gnu.tar.xz";
const PREFIX: &str = "slate-cli-x86_64-unknown-linux-gnu";
const OLD: &[u8] = b"previous working slate\n";
const NEW: &[u8] =
    b"#!/bin/sh\nprintf 'must not execute during installation' > \"$HOME/executed\"\n";

fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Fixture {
    directory: TempDir,
    bin: PathBuf,
    downloads: PathBuf,
    destination: PathBuf,
    work: PathBuf,
}

impl Fixture {
    fn new(root_layout: bool) -> Self {
        let directory = TempDir::new().unwrap();
        let bin = directory.path().join("tool bin");
        let downloads = directory.path().join("downloads");
        let destination = directory.path().join("install bin");
        let work = directory.path().join("temporary files");
        for path in [&bin, &downloads, &destination, &work] {
            fs::create_dir(path).unwrap();
        }
        script(
            &bin.join("curl"),
            r#"#!/bin/sh
url=
out=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift; out=$1 ;;
    https://*) url=$1 ;;
  esac
  shift
done
case "$url" in
  *.sha256) [ "$FAIL_DOWNLOAD" != checksum ] || exit 22 ;;
  *) [ "$FAIL_DOWNLOAD" != archive ] || exit 22 ;;
esac
exec /bin/cp "$DOWNLOADS/${url##*/}" "$out"
"#,
        );
        script(
            &bin.join("install"),
            r#"#!/bin/sh
if [ "$FAIL_INSTALL" = 1 ]; then
  for target do :; done
  printf 'partial copy' > "$target"
  exit 1
fi
if [ "$FAIL_INSTALL" = signal ]; then
  for target do :; done
  printf 'interrupted copy' > "$target"
  kill -TERM "$PPID"
  exit 143
fi
exec /usr/bin/install "$@"
"#,
        );
        script(
            &bin.join("mv"),
            r#"#!/bin/sh
[ "$FAIL_RENAME" != 1 ] || exit 1
if [ "$FAIL_RENAME" = after ]; then
  /bin/mv "$@" || exit 1
  exit 1
fi
exec /bin/mv "$@"
"#,
        );
        script(
            &bin.join("sudo"),
            "#!/bin/sh\nprintf 'unexpected privilege escalation' > \"$HOME/sudo-called\"\nexit 97\n",
        );
        fs::write(destination.join("slate"), OLD).unwrap();
        fs::set_permissions(destination.join("slate"), fs::Permissions::from_mode(0o751)).unwrap();
        let fixture = Self {
            directory,
            bin,
            downloads,
            destination,
            work,
        };
        let payload = fixture.directory.path().join("payload");
        let member = if root_layout {
            "slate".to_owned()
        } else {
            format!("{PREFIX}/slate")
        };
        fs::create_dir_all(payload.join(&member).parent().unwrap()).unwrap();
        fs::write(payload.join(&member), NEW).unwrap();
        fixture.archive(&[&member]);
        fixture
    }

    fn archive(&self, members: &[&str]) {
        let status = Command::new("/usr/bin/tar")
            .env("COPYFILE_DISABLE", "1")
            .args(["-cJf"])
            .arg(self.downloads.join(ASSET))
            .arg("-C")
            .arg(self.directory.path().join("payload"))
            .args(members)
            .status()
            .unwrap();
        assert!(status.success());
        self.refresh_checksum();
    }

    fn refresh_checksum(&self) {
        let mut command = if Path::new("/usr/bin/shasum").exists() {
            let mut command = Command::new("/usr/bin/shasum");
            command.args(["-a", "256"]);
            command
        } else {
            Command::new("/usr/bin/sha256sum")
        };
        let output = command.arg(self.downloads.join(ASSET)).output().unwrap();
        assert!(output.status.success());
        let hash = String::from_utf8(output.stdout).unwrap();
        self.checksum(&format!(
            "{}  {ASSET}\n",
            hash.split_whitespace().next().unwrap()
        ));
    }

    fn checksum(&self, content: &str) {
        fs::write(self.downloads.join(format!("{ASSET}.sha256")), content).unwrap();
    }

    fn command(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::new("/bin/sh");
        command
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("HOME", self.directory.path())
            .env("TMPDIR", &self.work)
            .env("SLATE_INSTALL_DIR", &self.destination)
            .env("SLATE_OS_OVERRIDE", "Linux")
            .env("SLATE_ARCH_OVERRIDE", "x86_64")
            .env("SLATE_VERSION", "v0.4.0")
            .env("DOWNLOADS", &self.downloads)
            .env("FAIL_DOWNLOAD", "")
            .env("FAIL_INSTALL", "0")
            .env("FAIL_RENAME", "0")
            .timeout(Duration::from_secs(10));
        command
    }

    fn assert_clean(&self) {
        assert_eq!(fs::read_dir(&self.work).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&self.destination).unwrap().count(), 1);
        assert!(!self.directory.path().join("executed").exists());
        assert!(!self.directory.path().join("sudo-called").exists());
    }

    fn assert_old(&self) {
        assert_eq!(fs::read(self.destination.join("slate")).unwrap(), OLD);
        assert_eq!(
            fs::metadata(self.destination.join("slate"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o751
        );
        self.assert_clean();
    }
}

#[test]
fn install_script_upgrades_both_supported_archive_layouts_without_running_binary() {
    for root_layout in [false, true] {
        let fixture = Fixture::new(root_layout);
        let old_inode = fixture.directory.path().join("old-inode");
        fs::hard_link(fixture.destination.join("slate"), &old_inode).unwrap();
        fixture.command().assert().success();
        assert_eq!(fs::read(fixture.destination.join("slate")).unwrap(), NEW);
        assert_eq!(fs::read(old_inode).unwrap(), OLD);
        assert_eq!(
            fs::metadata(fixture.destination.join("slate"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        fixture.assert_clean();
    }
    let fixture = Fixture::new(false);
    let outside = fixture.directory.path().join("unrelated-file");
    fs::write(&outside, OLD).unwrap();
    symlink(
        &outside,
        fixture.directory.path().join("payload/unrelated-link"),
    )
    .unwrap();
    fixture.archive(&[&format!("./{PREFIX}/slate"), "unrelated-link"]);
    fixture.command().assert().success();
    assert_eq!(fs::read(fixture.destination.join("slate")).unwrap(), NEW);
    assert_eq!(fs::read(outside).unwrap(), OLD);
    fixture.assert_clean();
}

#[test]
fn install_script_preserves_previous_binary_on_copy_and_rename_failure() {
    for (failure, value) in [
        ("FAIL_INSTALL", "1"),
        ("FAIL_INSTALL", "signal"),
        ("FAIL_RENAME", "1"),
    ] {
        let fixture = Fixture::new(false);
        fixture.command().env(failure, value).assert().failure();
        fixture.assert_old();
    }
    // A command can report failure after the atomic rename has happened.
    // Do not falsely promise that the old version is still at the destination.
    let fixture = Fixture::new(false);
    let output = fixture
        .command()
        .env("FAIL_RENAME", "after")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("could not confirm the final replacement")
    );
    assert_eq!(fs::read(fixture.destination.join("slate")).unwrap(), NEW);
    fixture.assert_clean();
}

#[test]
fn install_script_rejects_download_and_checksum_failures_before_replacing_binary() {
    for case in [
        "archive-download",
        "checksum-download",
        "mismatch",
        "malformed",
        "wrong-asset",
        "extra-record",
        "hasher-failure",
    ] {
        let fixture = Fixture::new(false);
        let record = fs::read_to_string(fixture.downloads.join(format!("{ASSET}.sha256"))).unwrap();
        let hash = record.split_whitespace().next().unwrap();
        let mut command = fixture.command();
        match case {
            "archive-download" => {
                command.env("FAIL_DOWNLOAD", "archive");
            }
            "checksum-download" => {
                command.env("FAIL_DOWNLOAD", "checksum");
            }
            "mismatch" => fixture.checksum(&format!("{}\n", "0".repeat(64))),
            "malformed" => fixture.checksum("not a checksum\n"),
            "wrong-asset" => fixture.checksum(&format!("{hash}  another-release.tar.xz\n")),
            "extra-record" => fixture.checksum(&format!("{record}{record}")),
            "hasher-failure" => script(
                &fixture.bin.join("shasum"),
                &format!("#!/bin/sh\nprintf '%s\\n' '{hash}'\nexit 1\n"),
            ),
            _ => unreachable!(),
        }
        command.assert().failure();
        fixture.assert_old();
    }
    // cargo-dist may use a bare digest; hexadecimal case is not significant.
    let fixture = Fixture::new(true);
    let record = fs::read_to_string(fixture.downloads.join(format!("{ASSET}.sha256"))).unwrap();
    fixture.checksum(&format!(
        "{}\n",
        record.split_whitespace().next().unwrap().to_uppercase()
    ));
    fixture.command().assert().success();
    assert_eq!(fs::read(fixture.destination.join("slate")).unwrap(), NEW);
    fixture.assert_clean();
}

#[test]
fn install_script_rejects_ambiguous_empty_linked_and_unexpected_archive_binaries() {
    for case in [
        "ambiguous",
        "duplicate",
        "empty",
        "symlink",
        "hardlink",
        "missing",
        "corrupt",
    ] {
        let fixture = Fixture::new(false);
        let payload = fixture.directory.path().join("payload");
        let member = format!("{PREFIX}/slate");
        let outside = fixture.directory.path().join("outside-file");
        fs::write(&outside, OLD).unwrap();
        match case {
            "ambiguous" => {
                fs::write(payload.join("slate"), NEW).unwrap();
                fixture.archive(&[&member, "slate"]);
            }
            "duplicate" => fixture.archive(&[&member, &member]),
            "empty" => {
                fs::write(payload.join(&member), []).unwrap();
                fixture.archive(&[&member]);
            }
            "symlink" => {
                fs::remove_file(payload.join(&member)).unwrap();
                symlink(&outside, payload.join(&member)).unwrap();
                fixture.archive(&[&member]);
            }
            "hardlink" => {
                fs::hard_link(payload.join(&member), payload.join("original")).unwrap();
                fixture.archive(&["original", &member]);
            }
            "missing" => {
                fs::create_dir(payload.join("unexpected")).unwrap();
                fs::write(payload.join("unexpected/slate"), NEW).unwrap();
                fixture.archive(&["unexpected/slate"]);
            }
            "corrupt" => {
                fs::write(fixture.downloads.join(ASSET), b"not a tar archive").unwrap();
                fixture.refresh_checksum();
            }
            _ => unreachable!(),
        }
        fixture.command().assert().failure();
        fixture.assert_old();
        assert_eq!(fs::read(outside).unwrap(), OLD);
    }
}

#[test]
fn install_script_preserves_existing_symlinks_and_directories() {
    for directory in [false, true] {
        let fixture = Fixture::new(false);
        let destination = fixture.destination.join("slate");
        fs::remove_file(&destination).unwrap();
        let protected = fixture.directory.path().join("package-manager-binary");
        fs::write(&protected, OLD).unwrap();
        if directory {
            fs::create_dir(&destination).unwrap();
            fs::write(destination.join("keep"), OLD).unwrap();
        } else {
            symlink(&protected, &destination).unwrap();
        }
        fixture.command().assert().failure();
        if directory {
            assert_eq!(fs::read(destination.join("keep")).unwrap(), OLD);
        } else {
            assert_eq!(fs::read_link(destination).unwrap(), protected);
        }
        assert_eq!(fs::read(protected).unwrap(), OLD);
        fixture.assert_clean();
    }
}

#[test]
fn install_script_validates_override_names_before_creating_files() {
    for (name, value) in [
        ("SLATE_BIN_NAME", "../escape"),
        ("SLATE_BIN_NAME", "--help"),
        ("SLATE_APP_NAME", "../../archive"),
    ] {
        let fixture = Fixture::new(false);
        fixture.command().env(name, value).assert().failure();
        fixture.assert_old();
    }
}
