//! Only private shell/download fixtures; never execute an upstream installer,
//! downloaded binary, actual curl, Homebrew or a font/cache operation.
use super::*;
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    time::Instant,
};

struct Fixture {
    temp: tempfile::TempDir,
    env: SlateEnv,
    curl: PathBuf,
}

impl Fixture {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home with spaces");
        fs::create_dir(&home).unwrap();
        let curl = temp.path().join("private curl");
        let fixture = Self {
            temp,
            env: SlateEnv::with_home(home),
            curl,
        };
        fixture.download_body("/bin/cat \"$fixture_dir/script\"");
        fs::write(fixture.temp.path().join("script"), format!(
            "#!/bin/sh\nset -eu\nfixture_dir={}\nprintf '%s\\n' \"$@\" > \"$fixture_dir/install-args\"\n[ \"$#\" = 3 ] && [ \"$1\" = -y ] && [ \"$2\" = -b ] || exit 91\nif read -r unexpected; then exit 92; fi\n{body}\n",
            quote(fixture.temp.path())
        )).unwrap();
        fixture
    }
    fn download_body(&self, body: &str) {
        fs::write(&self.curl, format!(
            "#!/bin/sh\nfixture_dir=${{0%/*}}\nprintf '%s\\n' \"$@\" > \"$fixture_dir/curl-args\"\nif read -r unexpected; then exit 93; fi\n{body}\n"
        )).unwrap();
        fs::set_permissions(&self.curl, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn binary(&self) -> PathBuf {
        self.env.user_local_bin().join("starship")
    }
    fn seed(&self) {
        fs::create_dir_all(self.env.user_local_bin()).unwrap();
        fs::write(self.binary(), b"original executable").unwrap();
        fs::set_permissions(self.binary(), fs::Permissions::from_mode(0o711)).unwrap();
    }
    fn run(&self) -> Result<()> {
        install_with(&self.env, &self.curl, DOWNLOAD_LIMITS, INSTALL_LIMITS)
    }
    fn source(&self) -> file_read::Source {
        file_read::read(&self.binary(), MAX_BINARY_BYTES, Links::Reject)
            .unwrap()
            .unwrap()
    }
    fn staged_dir(&self) -> PathBuf {
        let args = fs::read_to_string(self.temp.path().join("install-args")).unwrap();
        let lines: Vec<_> = args.lines().collect();
        assert_eq!(&lines[..2], &["-y", "-b"]);
        PathBuf::from(lines[2])
    }
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

const VALID: &str =
    "printf 'private new executable' > \"$3/starship\"\n/bin/chmod 711 \"$3/starship\"";

#[test]
fn starship_local_stages_then_atomically_publishes_with_controlled_mode() {
    for existing in [false, true] {
        let f = Fixture::new(VALID);
        if existing {
            f.seed();
        }
        let original = existing.then(|| f.source());
        f.run().unwrap();
        let current = f.source();
        assert_eq!(current.bytes, b"private new executable");
        assert_eq!(current.mode, Some(0o755));
        if let Some(original) = original {
            assert_ne!(current.identity, original.identity);
        }
        let stage = f.staged_dir();
        assert_ne!(stage, f.env.user_local_bin());
        assert!(
            !stage.exists(),
            "owned staging directory must be cleaned up"
        );
        assert_eq!(fs::read_dir(f.env.user_local_bin()).unwrap().count(), 1);
        let args = fs::read_to_string(f.temp.path().join("curl-args")).unwrap();
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            vec![
                "--disable",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--globoff",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-redirs",
                "5",
                "--connect-timeout",
                "10",
                "--max-time",
                "60",
                INSTALL_URL,
            ]
        );
    }
    assert_eq!(DOWNLOAD_LIMITS.timeout, Duration::from_secs(75));
    assert_eq!(DOWNLOAD_LIMITS.max_output, 1024 * 1024);
    assert_eq!(INSTALL_LIMITS.timeout, Duration::from_secs(600));
    assert_eq!(INSTALL_LIMITS.max_output, 2 * 1024 * 1024);
}

#[test]
fn starship_local_preflight_rejects_unsafe_targets_before_download() {
    for case in [
        "local-link",
        "local-file",
        "bin-link",
        "bin-file",
        "binary-link",
        "binary-dir",
        "oversized",
    ] {
        let f = Fixture::new(VALID);
        let outside = f.temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let local = f.env.home().join(".local");
        match case {
            "local-link" => symlink(&outside, &local).unwrap(),
            "local-file" => fs::write(&local, "keep").unwrap(),
            _ => {
                fs::create_dir(&local).unwrap();
                match case {
                    "bin-link" => symlink(&outside, f.env.user_local_bin()).unwrap(),
                    "bin-file" => fs::write(f.env.user_local_bin(), "keep").unwrap(),
                    _ => {
                        fs::create_dir(f.env.user_local_bin()).unwrap();
                        match case {
                            "binary-link" => symlink(outside.join("missing"), f.binary()).unwrap(),
                            "binary-dir" => fs::create_dir(f.binary()).unwrap(),
                            "oversized" => fs::File::create(f.binary())
                                .unwrap()
                                .set_len(MAX_BINARY_BYTES + 1)
                                .unwrap(),
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }
        assert!(f.run().is_err(), "{case}");
        assert!(!f.temp.path().join("curl-args").exists(), "{case}");
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
}

#[test]
fn starship_local_rejects_invalid_staged_files_and_keeps_original() {
    for body in [
        "exit 0",
        ": > \"$3/starship\"; /bin/chmod 755 \"$3/starship\"",
        "printf data > \"$3/starship\"; /bin/chmod 644 \"$3/starship\"",
        "/bin/ln -s \"$fixture_dir/script\" \"$3/starship\"",
        "/bin/mkdir \"$3/starship\"",
        "/usr/bin/mkfifo \"$3/starship\"",
        "/bin/ln \"$fixture_dir/oversized\" \"$3/starship\"",
        "/bin/rmdir \"$3\"; /bin/ln -s \"$fixture_dir\" \"$3\"",
    ] {
        let f = Fixture::new(body);
        fs::File::create(f.temp.path().join("oversized"))
            .unwrap()
            .set_len(MAX_BINARY_BYTES + 1)
            .unwrap();
        f.seed();
        let original = f.source();
        assert!(f.run().is_err(), "{body}");
        assert!(f.source() == original, "{body}");
        assert!(!f.staged_dir().exists());
    }
}

#[test]
fn starship_local_rechecks_the_live_target_after_the_staged_installer() {
    let f = Fixture::new(&format!(
        "{VALID}\nprintf 'external edit during install' > \"$fixture_dir/home with spaces/.local/bin/starship\""
    ));
    f.seed();
    let error = f.run().unwrap_err();
    assert!(error.to_string().contains("target file changed"));
    assert_eq!(f.source().bytes, b"external edit during install");
    assert!(!f.staged_dir().exists());
}

#[test]
fn starship_local_known_installer_failure_preserves_target_and_omits_output() {
    let f = Fixture::new("printf PRIVATE_NATIVE_SECRET >&2; exit 7");
    f.seed();
    let original = f.source();
    let error = f.run().unwrap_err();
    assert!(super::super::installation_fallback_allowed(&error));
    assert!(error.to_string().contains("exit code 7"));
    assert!(!error.to_string().contains("PRIVATE_NATIVE_SECRET"));
    assert!(f.source() == original);
}

#[test]
fn starship_local_interrupted_installer_stops_fallback_and_keeps_original() {
    for (body, reason) in [
        ("exec /bin/sleep 8", "wait limit"),
        ("/bin/sleep 8 &\nexit 0", "wait limit"),
        ("exec /usr/bin/yes PRIVATE_NATIVE_SECRET", "output limit"),
        ("kill -TERM \"$$\"", "signal"),
    ] {
        let f = Fixture::new(body);
        f.seed();
        let original = f.source();
        let started = Instant::now();
        let error = install_with(
            &f.env,
            &f.curl,
            DOWNLOAD_LIMITS,
            Limits {
                timeout: Duration::from_secs(1),
                max_output: 1024,
            },
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(matches!(error, SlateError::StarshipInstallUncertain(_)));
        assert!(!super::super::installation_fallback_allowed(&error));
        assert!(!super::super::tool_install::should_try_local_starship_fallback(&error));
        assert!(error.to_string().contains(reason));
        assert!(!error.to_string().contains("PRIVATE_NATIVE_SECRET"));
        assert!(f.source() == original);
        assert!(!f.staged_dir().exists());
    }
}

#[test]
fn starship_local_download_failure_never_runs_installer_or_creates_local_bin() {
    for body in [
        "printf PRIVATE_NATIVE_SECRET >&2; exit 7",
        "exit 0",
        "exec /usr/bin/yes PRIVATE_NATIVE_SECRET",
        "exec /bin/sleep 8",
    ] {
        let f = Fixture::new(VALID);
        f.download_body(body);
        let error = install_with(
            &f.env,
            &f.curl,
            Limits {
                timeout: Duration::from_secs(1),
                max_output: 1024,
            },
            INSTALL_LIMITS,
        )
        .unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_NATIVE_SECRET"));
        assert!(!f.temp.path().join("install-args").exists());
        assert!(!f.env.home().join(".local").exists());
    }
    let f = Fixture::new(VALID);
    assert!(install_with(
        &f.env,
        &f.temp.path().join("missing curl"),
        DOWNLOAD_LIMITS,
        INSTALL_LIMITS
    )
    .is_err());
    assert!(!f.env.home().join(".local").exists());
}

#[test]
fn starship_local_target_rechecks_file_directory_and_home_identity() {
    for case in [
        "content",
        "mode",
        "same-bytes-replacement",
        "directory-replacement",
        "home-alias",
    ] {
        let f = Fixture::new(VALID);
        f.seed();
        let alias = f.temp.path().join("home alias");
        symlink(f.env.home(), &alias).unwrap();
        let env = SlateEnv::with_home(alias.clone());
        let mut target = target::Target::capture(&env).unwrap();
        match case {
            "content" => fs::write(f.binary(), "external content").unwrap(),
            "mode" => fs::set_permissions(f.binary(), fs::Permissions::from_mode(0o700)).unwrap(),
            "same-bytes-replacement" => {
                let replacement = f.env.user_local_bin().join("replacement");
                fs::write(&replacement, "original executable").unwrap();
                fs::set_permissions(&replacement, fs::Permissions::from_mode(0o711)).unwrap();
                fs::rename(replacement, f.binary()).unwrap();
            }
            "directory-replacement" => {
                fs::rename(f.env.user_local_bin(), f.env.home().join("saved-bin")).unwrap();
                fs::create_dir(f.env.user_local_bin()).unwrap();
            }
            "home-alias" => {
                fs::remove_file(&alias).unwrap();
                let other = f.temp.path().join("other home");
                fs::create_dir(&other).unwrap();
                symlink(other, alias).unwrap();
            }
            _ => unreachable!(),
        }
        let before = file_read::read(&f.binary(), MAX_BINARY_BYTES, Links::Reject).unwrap();
        assert!(target.publish(&env, b"new binary").is_err(), "{case}");
        assert!(file_read::read(&f.binary(), MAX_BINARY_BYTES, Links::Reject).unwrap() == before);
    }
    let f = Fixture::new(VALID);
    let alias = f.temp.path().join("home alias");
    symlink(f.env.home(), &alias).unwrap();
    let env = SlateEnv::with_home(alias);
    let mut target = target::Target::capture(&env).unwrap();
    target.publish(&env, b"new binary").unwrap();
    assert_eq!(f.source().bytes, b"new binary");
}
