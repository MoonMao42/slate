use super::*;
use std::{
    fs,
    io::{Cursor, Write},
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::PathBuf,
};
use zip::{write::SimpleFileOptions, ZipWriter};

const FONT: &[u8] = b"\0\x01\0\0private-fixture-not-a-real-font";

struct Fixture {
    _temp: tempfile::TempDir,
    env: SlateEnv,
    curl: PathBuf,
}
impl Fixture {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let curl = root.join("private curl");
        let script = format!("#!/bin/sh\nfixture_dir=${{0%/*}}\nprintf '%s\\n' \"$@\" > \"$fixture_dir/args\"\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '--output' ]; then shift; destination=$1; fi\n  shift\ndone\nprintf '%s' \"$destination\" > \"$fixture_dir/destination\"\n{body}\n");
        fs::write(&curl, script).unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let fixture = Self {
            _temp: temp,
            env: SlateEnv::with_home(home),
            curl,
        };
        fixture.archive(&[("nested/A.ttf", FONT)]);
        fixture
    }
    fn archive(&self, entries: &[(&str, &[u8])]) {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        fs::write(
            self.curl.parent().unwrap().join("input.zip"),
            writer.finish().unwrap().into_inner(),
        )
        .unwrap();
    }
    fn target(&self) -> PathBuf {
        crate::platform::fonts::user_font_dir(&self.env)
    }
    fn install(&self, limits: Limits, max_file: u64) -> Result<files::Report> {
        install_with("JetBrainsMono", &self.curl, &self.env, limits, max_file)
    }
    fn assert_temp_removed(&self) {
        let bytes = fs::read(self.curl.parent().unwrap().join("destination")).unwrap();
        let output = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        assert!(
            !output.parent().unwrap().exists(),
            "download staging not removed"
        );
    }
}

#[test]
fn font_install_download_private_pipeline_uses_fixed_https_arguments_and_preserves_retries() {
    let f = Fixture::new("/bin/cp \"$fixture_dir/input.zip\" \"$destination\"");
    assert_eq!(
        f.install(LIMITS, archive::MAX_ARCHIVE_BYTES).unwrap(),
        files::Report {
            installed: 1,
            unchanged: 0
        }
    );
    assert_eq!(fs::read(f.target().join("A.ttf")).unwrap(), FONT);
    f.assert_temp_removed();
    assert_eq!(
        f.install(LIMITS, archive::MAX_ARCHIVE_BYTES).unwrap(),
        files::Report {
            installed: 0,
            unchanged: 1
        }
    );
    let args = fs::read_to_string(f.curl.parent().unwrap().join("args")).unwrap();
    let args: Vec<_> = args.lines().collect();
    assert_eq!(args[0], "--disable");
    for pair in [
        ["--proto", "=https"],
        ["--proto-redir", "=https"],
        ["--max-redirs", "5"],
        ["--max-filesize", "536870912"],
    ] {
        assert!(args.windows(2).any(|window| window == pair));
    }
    assert_eq!(
        *args.last().unwrap(),
        "https://github.com/ryanoasis/nerd-fonts/releases/latest/download/JetBrainsMono.zip"
    );
    assert!(!args.contains(&"--retry-all-errors"));
    f.assert_temp_removed();
}

#[test]
fn font_install_download_network_and_archive_failures_never_publish_fonts_or_native_output() {
    for (body, reason) in [
        ("/bin/cp \"$fixture_dir/input.zip\" \"$destination\"\nprintf 'PRIVATE_NATIVE_SECRET\\033[31m' >&2\nexit 7", "download failed"),
        ("exit 0", "no readable file"),
        ("printf 'PRIVATE_NATIVE_SECRET' > \"$destination\"", "supported ZIP"),
    ] {
        let f = Fixture::new(body);
        let error = f.install(LIMITS, archive::MAX_ARCHIVE_BYTES).unwrap_err().to_string();
        assert!(error.contains(reason), "{error}");
        assert!(!error.contains("PRIVATE_NATIVE_SECRET") && !error.contains('\u{1b}'));
        assert!(!f.target().exists());
        f.assert_temp_removed();
    }
    let f = Fixture::new("/bin/cp \"$fixture_dir/input.zip\" \"$destination\"");
    f.archive(&[("A.ttf", FONT), ("Z.ttf", b"invalid sfnt")]);
    assert!(f.install(LIMITS, archive::MAX_ARCHIVE_BYTES).is_err());
    assert!(!f.target().join("A.ttf").exists());
    f.assert_temp_removed();
}

#[test]
fn font_install_download_output_and_lifetime_are_bounded_without_real_network() {
    for (body, reason, limits) in [
        (
            "exec /usr/bin/yes PRIVATE_NATIVE_SECRET",
            "output limit",
            Limits {
                timeout: Duration::from_secs(3),
                max_output: 1024,
            },
        ),
        (
            "exec /bin/sleep 5",
            "timed out",
            Limits {
                timeout: Duration::from_secs(2),
                max_output: 1024,
            },
        ),
    ] {
        let f = Fixture::new(body);
        let error = f
            .install(limits, archive::MAX_ARCHIVE_BYTES)
            .unwrap_err()
            .to_string();
        assert!(error.contains(reason), "{error}");
        assert!(!error.contains("PRIVATE_NATIVE_SECRET"));
        assert!(!f.target().exists());
        f.assert_temp_removed();
    }
}

#[test]
fn font_install_download_child_file_limit_catches_unknown_size_and_preserves_parent_limit() {
    let f = Fixture::new("exec /bin/dd if=/dev/zero of=\"$destination\" bs=1024 count=16");
    let destination = f.curl.parent().unwrap().join("bounded.zip");
    let before = unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit), 0);
        limit
    };
    assert!(fetch(
        &f.curl,
        "https://example.invalid/fixture",
        &destination,
        LIMITS,
        4096
    )
    .is_err());
    assert!(fs::metadata(&destination).unwrap().len() <= 4096);
    unsafe {
        let mut after = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut after), 0);
        assert_eq!(
            (before.rlim_cur, before.rlim_max),
            (after.rlim_cur, after.rlim_max)
        );
    }
}

#[test]
fn font_install_download_paths_are_not_lossy_and_unknown_assets_never_start_curl() {
    // Check argument bytes without asking APFS to create an invalid UTF-8 name.
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new("printf '%s' \"$destination\" > \"$fixture_dir/raw-path\"");
    let destination = f
        .curl
        .parent()
        .unwrap()
        .join(std::ffi::OsString::from_vec(b"raw-\xff.zip".to_vec()));
    fetch(
        &f.curl,
        "https://example.invalid/fixture",
        &destination,
        LIMITS,
        archive::MAX_ARCHIVE_BYTES,
    )
    .unwrap();
    assert_eq!(
        fs::read(f.curl.parent().unwrap().join("raw-path")).unwrap(),
        destination.as_os_str().as_bytes()
    );
    fs::remove_file(f.curl.parent().unwrap().join("args")).unwrap();
    assert!(install_with(
        "../../not-catalog",
        &f.curl,
        &f.env,
        LIMITS,
        archive::MAX_ARCHIVE_BYTES
    )
    .is_err());
    assert!(!f.curl.parent().unwrap().join("args").exists());
    assert!(!f.target().exists());
}
