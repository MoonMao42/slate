//! Verify post-install cache outcomes using the actual no-clobber publisher,
//! private header-bearing files and injected cache results, never native fonts.
use super::*;
use std::{cell::Cell, fs};

#[test]
fn font_cache_post_install_warnings_keep_successful_files_and_identical_retries() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let font = b"\0\x01\0\0private-font-cache-fixture";
    fs::write(source.join("A.ttf"), font).unwrap();
    for expected in [
        FontCacheRefresh::NotNeeded,
        FontCacheRefresh::Refreshed,
        FontCacheRefresh::MissingDependency,
        FontCacheRefresh::Failed,
        FontCacheRefresh::CouldNotStart,
        FontCacheRefresh::TimedOut,
        FontCacheRefresh::OutputLimit,
        FontCacheRefresh::UnsafeDirectory,
    ] {
        let refreshed = Cell::new(0);
        let outcome = finish_file_install(files::install(&source, &env), || {
            refreshed.set(refreshed.get() + 1);
            expected
        })
        .unwrap();
        assert_eq!(outcome, expected);
        assert_eq!(refreshed.get(), 1);
        let directory = crate::platform::fonts::user_font_dir(&env);
        assert_eq!(fs::read(directory.join("A.ttf")).unwrap(), font);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    }
}

#[test]
fn font_cache_failed_publication_never_runs_the_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("A.ttf"), b"not a font").unwrap();
    assert!(
        finish_file_install(files::install(&source, &env), || panic!(
            "failed publication must not refresh caches"
        ))
        .is_err()
    );
    assert!(!crate::platform::fonts::user_font_dir(&env)
        .join("A.ttf")
        .exists());
}
