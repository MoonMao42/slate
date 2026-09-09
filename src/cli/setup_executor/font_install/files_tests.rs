use super::*;
use std::os::unix::{
    ffi::OsStringExt,
    fs::{symlink, MetadataExt},
};

// Header-bearing bytes only: never installed into a real font directory or
// interpreted by a font engine. These tests verify the file handoff contract.
const FONT: &[u8] = b"\0\x01\0\0private-fixture-not-a-real-font";
const OTHER: &[u8] = b"OTTOdifferent-private-fixture-font";

fn font_paths_fixture() -> Fixture {
    let mut f = Fixture::new();
    let home = f.env.home().join("profile");
    let data = f.env.home().join("字 external data/missing");
    fs::create_dir(&home).unwrap();
    f.env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(home.as_os_str().to_owned()),
        "XDG_DATA_HOME" => Some(data.as_os_str().to_owned()),
        _ => None,
    })
    .unwrap();
    f.target = data.join("fonts");
    f
}

fn install_linux(f: &Fixture) -> Result<Report> {
    install_for_backend(
        &f.source,
        &f.env,
        crate::platform::fonts::FontPlatformBackend::Fontconfig,
        |_| Ok(()),
    )
}

#[test]
fn font_paths_publish_outside_home_without_migrating_old_fonts_and_retry_identically() {
    let f = font_paths_fixture();
    f.font("A.ttf", FONT);
    let old = f.env.home().join(".local/share/fonts");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("old.ttf"), OTHER).unwrap();
    assert_eq!(
        install_linux(&f).unwrap(),
        Report {
            installed: 1,
            unchanged: 0
        }
    );
    assert!(!f.target.starts_with(f.env.home()));
    for directory in [
        &f.target,
        f.env.xdg_data_home(),
        f.env.xdg_data_home().parent().unwrap(),
    ] {
        assert_eq!(fs::metadata(directory).unwrap().mode() & 0o777, 0o700);
    }
    let before = fs::metadata(f.target.join("A.ttf")).unwrap();
    assert_eq!(before.mode() & 0o777, 0o644);
    assert_eq!(
        install_linux(&f).unwrap(),
        Report {
            installed: 0,
            unchanged: 1
        }
    );
    let after = fs::metadata(f.target.join("A.ttf")).unwrap();
    assert_eq!(
        (before.ino(), before.mode(), before.modified().unwrap()),
        (after.ino(), after.mode(), after.modified().unwrap())
    );
    assert_eq!(fs::read(old.join("old.ttf")).unwrap(), OTHER);
    assert!(!old.join("A.ttf").exists());
}

#[test]
fn font_paths_publish_resolves_explicit_root_alias_and_keeps_existing_permissions() {
    let f = font_paths_fixture();
    f.font("A.ttf", FONT);
    let actual = f.source.parent().unwrap().join("real-data");
    fs::create_dir(&actual).unwrap();
    fs::set_permissions(&actual, fs::Permissions::from_mode(0o750)).unwrap();
    fs::create_dir_all(f.env.xdg_data_home().parent().unwrap()).unwrap();
    symlink(&actual, f.env.xdg_data_home()).unwrap();
    assert_eq!(install_linux(&f).unwrap().installed, 1);
    assert_eq!(fs::read(actual.join("fonts/A.ttf")).unwrap(), FONT);
    assert_eq!(fs::metadata(&actual).unwrap().mode() & 0o777, 0o750);
    assert!(fs::symlink_metadata(f.env.xdg_data_home())
        .unwrap()
        .is_symlink());
}

#[test]
fn font_paths_publish_rejects_linked_or_non_directory_managed_suffix() {
    for kind in ["link", "dangling", "file"] {
        let f = font_paths_fixture();
        f.font("A.ttf", FONT);
        fs::create_dir_all(f.env.xdg_data_home()).unwrap();
        match kind {
            "link" => symlink(&f.source, &f.target).unwrap(),
            "dangling" => symlink(f.source.join("absent"), &f.target).unwrap(),
            "file" => fs::write(&f.target, b"keep").unwrap(),
            _ => unreachable!(),
        }
        let before = fs::symlink_metadata(&f.target).unwrap();
        assert!(install_linux(&f).is_err(), "{kind}");
        let after = fs::symlink_metadata(&f.target).unwrap();
        assert_eq!((before.ino(), before.mode()), (after.ino(), after.mode()));
        assert_eq!(fs::read(f.source.join("A.ttf")).unwrap(), FONT);
        assert!(!f.env.home().join(".local").exists());
    }
}

#[test]
fn font_paths_publish_stops_when_configured_root_alias_is_retargeted() {
    let f = font_paths_fixture();
    f.font("A.ttf", FONT);
    f.font("B.ttf", OTHER);
    let actual = f.source.parent().unwrap().join("actual-data");
    let replacement = f.source.parent().unwrap().join("replacement-data");
    fs::create_dir(&actual).unwrap();
    fs::create_dir(&replacement).unwrap();
    fs::write(replacement.join("keep"), b"foreign").unwrap();
    fs::create_dir_all(f.env.xdg_data_home().parent().unwrap()).unwrap();
    symlink(&actual, f.env.xdg_data_home()).unwrap();
    let error = install_for_backend(
        &f.source,
        &f.env,
        crate::platform::fonts::FontPlatformBackend::Fontconfig,
        |count| {
            if count == 1 {
                fs::remove_file(f.env.xdg_data_home()).unwrap(); // This fixture's own link only.
                symlink(&replacement, f.env.xdg_data_home()).unwrap();
            }
            Ok(())
        },
    )
    .unwrap_err()
    .to_string();
    // The operation conservatively retains its already-published copy when the
    // target check changes; it must not undo through the retargeted alias.
    assert!(error.contains("cleanup was incomplete"), "{error}");
    assert!(error.contains("A.ttf"), "{error}");
    assert_eq!(fs::read(actual.join("fonts/A.ttf")).unwrap(), FONT);
    assert!(!actual.join("fonts/B.ttf").exists());
    assert!(!replacement.join("fonts").exists());
    assert_eq!(fs::read(replacement.join("keep")).unwrap(), b"foreign");
}

struct Fixture {
    _temp: tempfile::TempDir,
    env: SlateEnv,
    source: PathBuf,
    target: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(temp.path()).unwrap();
        let source = home.join("source");
        fs::create_dir(&source).unwrap();
        let env = SlateEnv::with_home(home);
        let target = crate::platform::fonts::user_font_dir(&env);
        Self {
            _temp: temp,
            env,
            source,
            target,
        }
    }
    fn font(&self, relative: &str, bytes: &[u8]) {
        let path = self.source.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn existing(&self, name: &str, bytes: &[u8]) {
        fs::create_dir_all(&self.target).unwrap();
        fs::write(self.target.join(name), bytes).unwrap();
    }
    fn entries(&self) -> Vec<String> {
        if !self.target.exists() {
            return Vec::new();
        }
        let mut names: Vec<_> = fs::read_dir(&self.target)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
    fn install(&self) -> Result<Report> {
        install(&self.source, &self.env)
    }
}

#[test]
fn font_install_files_complete_family_and_identical_retry_preserve_existing_metadata() {
    let f = Fixture::new();
    f.font("nested/A.ttf", FONT);
    f.font("B.OTF", OTHER);
    f.font("C.ttc", b"ttcfprivate-collection-fixture");
    f.font("D.OTC", b"ttcfprivate-opentype-collection-fixture");
    f.font("LICENSE.txt", b"not a font");
    assert_eq!(
        f.install().unwrap(),
        Report {
            installed: 4,
            unchanged: 0
        }
    );
    assert_eq!(f.entries(), ["A.ttf", "B.OTF", "C.ttc", "D.OTC"]);
    for name in f.entries() {
        assert_eq!(
            fs::metadata(f.target.join(name)).unwrap().mode() & 0o777,
            0o644
        );
    }
    let path = f.target.join("A.ttf");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    let before = fs::metadata(&path).unwrap();
    assert_eq!(
        f.install().unwrap(),
        Report {
            installed: 0,
            unchanged: 4
        }
    );
    let after = fs::metadata(&path).unwrap();
    assert_eq!(
        (before.ino(), before.mode(), before.modified().unwrap()),
        (after.ino(), after.mode(), after.modified().unwrap())
    );
    assert_eq!(fs::read(path).unwrap(), FONT);
    assert_eq!(fs::read(f.source.join("nested/A.ttf")).unwrap(), FONT);
}

#[test]
fn font_install_files_destination_conflicts_abort_the_whole_preflight() {
    for kind in [
        "different",
        "link",
        "dangling",
        "directory",
        "fifo",
        "hardlink",
    ] {
        let f = Fixture::new();
        f.font("A.ttf", FONT);
        f.font("Z.ttf", FONT);
        fs::create_dir_all(&f.target).unwrap();
        let unrelated = f.env.home().join("unrelated");
        fs::write(&unrelated, b"SECRET EXISTING FILE").unwrap();
        let conflict = f.target.join("Z.ttf");
        match kind {
            "different" => fs::write(&conflict, b"SECRET EXISTING FILE").unwrap(),
            "link" => symlink(&unrelated, &conflict).unwrap(),
            "dangling" => symlink(f.env.home().join("absent"), &conflict).unwrap(),
            "directory" => fs::create_dir(&conflict).unwrap(),
            "fifo" => {
                use std::os::unix::ffi::OsStrExt;
                let path = std::ffi::CString::new(conflict.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "hardlink" => fs::hard_link(&unrelated, &conflict).unwrap(),
            _ => unreachable!(),
        }
        let before = fs::symlink_metadata(&conflict).unwrap();
        let error = f.install().unwrap_err().to_string();
        assert!(!error.contains("SECRET"), "{error}");
        let after = fs::symlink_metadata(&conflict).unwrap();
        assert_eq!((before.ino(), before.mode()), (after.ino(), after.mode()));
        assert_eq!(f.entries(), ["Z.ttf"]);
        assert_eq!(fs::read(unrelated).unwrap(), b"SECRET EXISTING FILE");
    }
}

#[test]
fn font_install_files_unsafe_sources_are_rejected_without_partial_fonts() {
    for kind in [
        "empty",
        "invalid",
        "oversized",
        "file_link",
        "loop",
        "fifo",
        "duplicate",
        "control",
        "deep",
    ] {
        let f = Fixture::new();
        f.font("A.ttf", FONT);
        let bad = f.source.join("Z.ttf");
        match kind {
            "empty" => fs::write(bad, []).unwrap(),
            "invalid" => fs::write(bad, "SECRET invalid font").unwrap(),
            "oversized" => fs::File::create(bad)
                .unwrap()
                .set_len(MAX_FONT_BYTES + 1)
                .unwrap(),
            "file_link" => symlink(f.source.join("A.ttf"), bad).unwrap(),
            "loop" => symlink(&f.source, f.source.join("loop")).unwrap(),
            "fifo" => {
                use std::os::unix::ffi::OsStrExt;
                let path = std::ffi::CString::new(bad.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "duplicate" => f.font("nested/a.TTF", OTHER),
            "control" => f.font("bad\nname.ttf", FONT),
            "deep" => f.font(&format!("{}Z.ttf", "d/".repeat(MAX_DEPTH + 1)), FONT),
            _ => unreachable!(),
        }
        let error = f.install().unwrap_err().to_string();
        assert!(!error.contains("SECRET"), "{error}");
        assert!(f.entries().is_empty(), "{kind}");
        assert_eq!(fs::read(f.source.join("A.ttf")).unwrap(), FONT);
    }
}

#[test]
fn font_install_files_mid_batch_failure_rolls_back_only_new_additions() {
    let f = Fixture::new();
    f.font("A.ttf", FONT);
    f.font("B.ttf", OTHER);
    f.font("Z.ttf", FONT);
    f.existing("Z.ttf", FONT);
    f.existing("unrelated.txt", b"keep me");
    let error = install_with(&f.source, &f.env, |count| {
        if count == 1 {
            Err(failure("injected publish failure"))
        } else {
            Ok(())
        }
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("1 new file(s) rolled back"), "{error}");
    assert_eq!(f.entries(), ["Z.ttf", "unrelated.txt"]);
    assert_eq!(fs::read(f.target.join("Z.ttf")).unwrap(), FONT);
}

#[test]
fn font_install_files_late_destination_arrivals_are_not_overwritten_or_removed() {
    for same in [false, true] {
        let f = Fixture::new();
        f.font("A.ttf", FONT);
        f.font("B.ttf", OTHER);
        let result = install_with(&f.source, &f.env, |count| {
            if count == 1 {
                fs::write(
                    f.target.join("B.ttf"),
                    if same { OTHER } else { b"outside edit" },
                )
                .unwrap();
            }
            Ok(())
        });
        if same {
            assert_eq!(
                result.unwrap(),
                Report {
                    installed: 1,
                    unchanged: 1
                }
            );
            assert_eq!(f.entries(), ["A.ttf", "B.ttf"]);
        } else {
            assert!(result.unwrap_err().to_string().contains("rolled back"));
            assert_eq!(f.entries(), ["B.ttf"]);
            assert_eq!(fs::read(f.target.join("B.ttf")).unwrap(), b"outside edit");
        }
    }
}

#[test]
fn font_install_files_rollback_retains_observed_external_edits_and_reports_paths() {
    for change in ["in_place", "replaced", "source_changed", "source_missing"] {
        let f = Fixture::new();
        f.font("A.ttf", FONT);
        f.font("B.ttf", OTHER);
        let output = f.target.join("A.ttf");
        let error = install_with(&f.source, &f.env, |count| {
            if count == 1 {
                match change {
                    "in_place" => fs::write(&output, b"external user edit").unwrap(),
                    "replaced" => {
                        fs::rename(&output, f.env.home().join("kept-font")).unwrap();
                        fs::write(&output, FONT).unwrap();
                    }
                    "source_changed" => fs::write(f.source.join("A.ttf"), OTHER).unwrap(),
                    "source_missing" => fs::remove_file(f.source.join("A.ttf")).unwrap(),
                    _ => unreachable!(),
                }
                return Err(failure("injected publish failure"));
            }
            Ok(())
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("cleanup was incomplete"), "{error}");
        assert!(error.contains("A.ttf"), "{error}");
        assert!(output.is_file());
        assert_eq!(f.entries(), ["A.ttf"]);
        if change == "in_place" {
            assert_eq!(fs::read(output).unwrap(), b"external user edit");
        }
    }
}

#[test]
fn font_install_files_target_links_and_replacement_stop_before_external_writes() {
    let f = Fixture::new();
    f.font("A.ttf", FONT);
    let outside = f.env.home().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::create_dir_all(f.target.parent().unwrap()).unwrap();
    symlink(&outside, &f.target).unwrap();
    assert!(f.install().unwrap_err().to_string().contains("linked"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

    let f = Fixture::new();
    f.font("A.ttf", FONT);
    f.font("B.ttf", OTHER);
    let retained = f.env.home().join("retained-font-directory");
    let mut substituted_files = Vec::new();
    let error = install_with(&f.source, &f.env, |count| {
        if count == 1 {
            fs::rename(&f.target, &retained).unwrap();
            fs::create_dir(&f.target).unwrap();
            fs::write(f.target.join("A.ttf"), b"outside edit").unwrap();
            let scratch = fs::read_dir(&retained)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.is_dir())
                .unwrap();
            let replacement = f.target.join(scratch.file_name().unwrap());
            fs::create_dir(&replacement).unwrap();
            for entry in fs::read_dir(scratch).unwrap() {
                let path = replacement.join(entry.unwrap().file_name());
                fs::write(&path, b"unrelated private file").unwrap();
                substituted_files.push(path);
            }
        }
        Ok(())
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("cleanup was incomplete"), "{error}");
    assert_eq!(fs::read(f.target.join("A.ttf")).unwrap(), b"outside edit");
    assert!(!f.target.join("B.ttf").exists());
    assert_eq!(fs::read(retained.join("A.ttf")).unwrap(), FONT);
    assert!(!substituted_files.is_empty());
    for path in substituted_files {
        assert_eq!(fs::read(path).unwrap(), b"unrelated private file");
    }
}

#[test]
fn font_install_files_count_and_byte_budgets_reject_excess_without_large_allocations() {
    assert_eq!(
        account_bytes(MAX_TOTAL_BYTES - 1, 1).unwrap(),
        MAX_TOTAL_BYTES
    );
    assert!(account_bytes(MAX_TOTAL_BYTES, 1).is_err());
    assert!(account_bytes(u64::MAX, 1).is_err());
    let f = Fixture::new();
    for index in 0..=MAX_FONTS {
        f.font(&format!("{index}.ttf"), FONT);
    }
    assert!(f
        .install()
        .unwrap_err()
        .to_string()
        .contains("font-count limit"));
    assert!(f.entries().is_empty());
    // APFS cannot create arbitrary non-UTF-8 names; validate the pathname logic
    // without trying to materialize such a filename on that filesystem.
    assert!(!is_font(&PathBuf::from(std::ffi::OsString::from_vec(
        b"name.\xff".to_vec()
    ))));
}
