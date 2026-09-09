use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

struct Fixture {
    _temp: tempfile::TempDir,
    env: SlateEnv,
    binary: PathBuf,
    directory: PathBuf,
}
impl Fixture {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("字 private home");
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(temp.path().join("private config").into_os_string()),
            "XDG_CACHE_HOME" => Some(temp.path().join("private cache").into_os_string()),
            _ => None,
        })
        .unwrap();
        let directory = user_font_dir_for_backend(&env, FontPlatformBackend::Fontconfig);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("sentinel.ttf"),
            b"not read by a native font engine",
        )
        .unwrap();
        let binary = temp.path().join("private fc-cache");
        fs::write(&binary, format!("#!/bin/sh\nfixture_dir=${{0%/*}}\nprintf '%s\\0' \"$HOME\" \"$XDG_CONFIG_HOME\" \"$XDG_CACHE_HOME\" \"$XDG_DATA_HOME\" \"$@\" > \"$fixture_dir/arguments\"\n{body}\n")).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _temp: temp,
            env,
            binary,
            directory,
        }
    }
    fn run(&self, limits: Limits) -> FontCacheRefresh {
        refresh_with(
            FontPlatformBackend::Fontconfig,
            &self.env,
            || Some(self.binary.clone()),
            limits,
        )
    }
    fn marker(&self) -> PathBuf {
        self.binary.parent().unwrap().join("arguments")
    }
    fn assert_font_unchanged(&self) {
        assert_eq!(
            fs::read(self.directory.join("sentinel.ttf")).unwrap(),
            b"not read by a native font engine"
        );
    }
}

#[test]
fn font_cache_command_targets_the_selected_profile_and_one_font_directory() {
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new("exit 0");
    assert_eq!(f.run(REFRESH_LIMITS), FontCacheRefresh::Refreshed);
    let raw = fs::read(f.marker()).unwrap();
    let args: Vec<_> = raw
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .collect();
    let directory = fs::canonicalize(&f.directory).unwrap();
    assert_eq!(
        args,
        [
            f.env.home().as_os_str().as_bytes(),
            f.env.xdg_config_home().as_os_str().as_bytes(),
            f.env.cache_dir().as_os_str().as_bytes(),
            f.env.xdg_data_home().as_os_str().as_bytes(),
            b"--force",
            b"--error-on-no-fonts",
            b"--",
            directory.as_os_str().as_bytes()
        ]
    );
    f.assert_font_unchanged();
    assert!(
        !f.env.cache_dir().exists(),
        "fixture must not invoke the real cache builder"
    );
}

#[test]
fn font_cache_mac_skips_resolution_and_linux_reports_missing_or_unstartable_command() {
    let f = Fixture::new("exit 0");
    assert_eq!(
        refresh_with(
            FontPlatformBackend::Macos,
            &f.env,
            || panic!("macOS must not look up fc-cache"),
            REFRESH_LIMITS
        ),
        FontCacheRefresh::NotNeeded
    );
    assert!(!f.marker().exists());
    assert_eq!(
        refresh_with(
            FontPlatformBackend::Fontconfig,
            &f.env,
            || None,
            REFRESH_LIMITS
        ),
        FontCacheRefresh::MissingDependency
    );
    assert_eq!(
        refresh_with(
            FontPlatformBackend::Fontconfig,
            &f.env,
            || Some(f.binary.with_file_name("PRIVATE-MISSING")),
            REFRESH_LIMITS
        ),
        FontCacheRefresh::CouldNotStart
    );
    fs::set_permissions(&f.binary, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(f.run(REFRESH_LIMITS), FontCacheRefresh::CouldNotStart);
    assert!(!f.marker().exists());
    f.assert_font_unchanged();
}

#[test]
fn font_cache_failed_noisy_and_timed_out_commands_keep_fonts_and_omit_native_output() {
    for (body, expected) in [
        (
            "printf 'PRIVATE_NATIVE_SECRET\\033[2J' >&2; exit 7",
            FontCacheRefresh::Failed,
        ),
        (
            "exec /usr/bin/yes PRIVATE_NATIVE_SECRET",
            FontCacheRefresh::OutputLimit,
        ),
        ("exec /bin/sleep 8", FontCacheRefresh::TimedOut),
    ] {
        let f = Fixture::new(body);
        let outcome = f.run(Limits {
            timeout: Duration::from_secs(2),
            max_output: 1024,
        });
        assert_eq!(outcome, expected);
        let notice = activation_hint(outcome);
        assert!(outcome.needs_attention());
        assert!(!notice.contains("PRIVATE") && !notice.contains('\u{1b}'));
        assert!(
            notice.contains("installed fonts were kept") && notice.contains("partial cache writes")
        );
        f.assert_font_unchanged();
    }
}

#[test]
fn font_cache_unsafe_or_replaced_directories_do_not_launch_the_scanner() {
    for kind in ["missing", "file", "final-link", "ancestor-link"] {
        let f = Fixture::new("exit 0");
        let saved = f.binary.parent().unwrap().join("saved-fonts");
        fs::rename(&f.directory, &saved).unwrap();
        match kind {
            "missing" => {}
            "file" => fs::write(&f.directory, b"not a directory").unwrap(),
            "final-link" => symlink(&saved, &f.directory).unwrap(),
            "ancestor-link" => {
                let parent = f.directory.parent().unwrap();
                fs::remove_dir(parent).unwrap(); // Fixture's now-empty private directory only.
                symlink(f.binary.parent().unwrap(), parent).unwrap();
                fs::rename(&saved, f.binary.parent().unwrap().join("fonts")).unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            f.run(REFRESH_LIMITS),
            FontCacheRefresh::UnsafeDirectory,
            "{kind}"
        );
        assert!(!f.marker().exists());
    }
    // HOME itself may be a legitimate alias, as in macOS /var -> /private/var.
    let f = Fixture::new("exit 0");
    let alias = f.binary.parent().unwrap().join("home-alias");
    symlink(f.env.home(), &alias).unwrap();
    let env = SlateEnv::with_home(alias);
    assert_eq!(
        refresh_with(
            FontPlatformBackend::Fontconfig,
            &env,
            || Some(f.binary.clone()),
            REFRESH_LIMITS
        ),
        FontCacheRefresh::Refreshed
    );
}

#[test]
fn font_cache_hints_only_claim_completion_for_an_observed_success() {
    for outcome in [
        FontCacheRefresh::NotRequested,
        FontCacheRefresh::NotNeeded,
        FontCacheRefresh::Refreshed,
        FontCacheRefresh::MissingDependency,
        FontCacheRefresh::Failed,
        FontCacheRefresh::CouldNotStart,
        FontCacheRefresh::TimedOut,
        FontCacheRefresh::OutputLimit,
        FontCacheRefresh::UnsafeDirectory,
    ] {
        let hint = activation_hint(outcome);
        assert_eq!(
            hint.contains("completed successfully"),
            outcome == FontCacheRefresh::Refreshed
        );
        assert_eq!(
            outcome.needs_attention(),
            !matches!(
                outcome,
                FontCacheRefresh::NotRequested
                    | FontCacheRefresh::NotNeeded
                    | FontCacheRefresh::Refreshed
            )
        );
        assert!(!hint.contains("Slate refreshed"));
        assert!(!hint.chars().any(char::is_control));
    }
    assert!(activation_hint(FontCacheRefresh::NotRequested)
        .contains("No font-cache refresh was requested"));
    assert!(activation_hint(FontCacheRefresh::Refreshed).contains("rendering is not verified"));
}

#[test]
fn font_paths_cache_custom_data_root_and_alias_use_captured_environment() {
    use std::os::unix::ffi::OsStrExt;
    for aliased in [false, true] {
        let f = Fixture::new("exit 0");
        let actual = f.binary.parent().unwrap().join("字 external data");
        fs::create_dir_all(actual.join("fonts")).unwrap();
        let data = if aliased {
            let alias = actual.with_file_name("chosen alias");
            symlink(&actual, &alias).unwrap();
            alias
        } else {
            actual.clone()
        };
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(f.env.home().into()),
            "XDG_DATA_HOME" => Some(data.clone().into_os_string()),
            "XDG_CONFIG_HOME" => Some(f.env.xdg_config_home().into()),
            "XDG_CACHE_HOME" => Some(f.env.cache_dir().into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            refresh_with(
                FontPlatformBackend::Fontconfig,
                &env,
                || Some(f.binary.clone()),
                REFRESH_LIMITS
            ),
            FontCacheRefresh::Refreshed
        );
        let raw = fs::read(f.marker()).unwrap();
        let args: Vec<_> = raw
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .collect();
        assert_eq!(args.len(), 8);
        assert_eq!(args[0], env.home().as_os_str().as_bytes());
        assert_eq!(args[3], data.as_os_str().as_bytes());
        assert_eq!(
            args[7],
            fs::canonicalize(actual.join("fonts"))
                .unwrap()
                .as_os_str()
                .as_bytes()
        );
        f.assert_font_unchanged();
    }
}

#[test]
fn font_paths_cache_rejects_links_below_explicit_data_root() {
    let f = Fixture::new("exit 0");
    let data = f.binary.parent().unwrap().join("custom-data");
    fs::create_dir(&data).unwrap();
    symlink(&f.directory, data.join("fonts")).unwrap();
    let env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(f.env.home().into()),
        "XDG_DATA_HOME" => Some(data.clone().into_os_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(
        refresh_with(
            FontPlatformBackend::Fontconfig,
            &env,
            || Some(f.binary.clone()),
            REFRESH_LIMITS
        ),
        FontCacheRefresh::UnsafeDirectory
    );
    assert!(!f.marker().exists());
    f.assert_font_unchanged();
}
