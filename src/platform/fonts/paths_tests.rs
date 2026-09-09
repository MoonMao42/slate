use super::super::{font_search_paths_for_backend, user_font_dir_for_backend};
use super::*;
use std::os::unix::{ffi::OsStringExt, fs::symlink};

fn configured(home: &Path, data: &Path) -> SlateEnv {
    SlateEnv::from_vars(|key| match key {
        "HOME" => Some(home.as_os_str().to_owned()),
        "XDG_DATA_HOME" => Some(data.as_os_str().to_owned()),
        _ => None,
    })
    .unwrap()
}

#[test]
fn font_paths_data_home_capture_defaults_and_isolation() {
    let home = Path::new("/private-profile");
    for data in ["", "relative/data", "../data"] {
        let env = configured(home, Path::new(data));
        assert_eq!(env.xdg_data_home(), home.join(".local/share"));
        assert!(!env.xdg_data_home_overridden());
    }
    let env = SlateEnv::from_vars(|key| match key {
        "SLATE_HOME" => Some(home.into()),
        "HOME" => Some("/other-home".into()),
        "XDG_DATA_HOME" => Some("/external-data".into()),
        _ => None,
    })
    .unwrap();
    assert_eq!(env.xdg_data_home(), home.join(".local/share"));
    assert!(!env.xdg_data_home_overridden());
    let isolated = SlateEnv::with_home(home.into());
    assert_eq!(isolated.xdg_data_home(), env.xdg_data_home());
    assert!(!isolated.xdg_data_home_overridden());
}

#[test]
fn font_paths_data_home_snapshot_preserves_os_bytes_and_config_context() {
    // Path capture only: no non-UTF-8 filenames are created on APFS.
    let data = PathBuf::from(std::ffi::OsString::from_vec(
        b"/external data/\xff".to_vec(),
    ));
    let env = configured(Path::new("/profile"), &data);
    let config = crate::config::ConfigManager::from_env_paths(&env);
    drop(env);
    assert_eq!(config.environment().xdg_data_home(), data);
    assert!(config.environment().xdg_data_home_overridden());
}

#[test]
fn font_paths_custom_root_is_shared_by_linux_search_and_install_not_macos() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let home = base.join("home");
    fs::create_dir(&home).unwrap();
    let data = base.join("字 external data/missing");
    let env = configured(&home, &data);
    assert_eq!(
        install_directory(&env, FontPlatformBackend::Fontconfig).unwrap(),
        data.join("fonts")
    );
    let linux = font_search_paths_for_backend(&env, FontPlatformBackend::Fontconfig);
    assert_eq!(linux[0], data.join("fonts"));
    assert!(linux.contains(&home.join(".fonts")));
    assert!(!linux.contains(&home.join(".local/share/fonts")));
    let mac = font_search_paths_for_backend(&env, FontPlatformBackend::Macos);
    assert_eq!(mac[0], home.join("Library/Fonts"));
    assert!(!mac.contains(&data.join("fonts")));
    assert_eq!(
        install_directory(&env, FontPlatformBackend::Macos).unwrap(),
        mac[0]
    );
    assert!(
        !data.parent().unwrap().exists(),
        "path resolution must not create directories"
    );
}

#[test]
fn font_paths_explicit_root_alias_is_resolved_with_missing_suffix() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let actual = root.join("actual");
    fs::create_dir(&actual).unwrap();
    let alias = root.join("alias");
    symlink(&actual, &alias).unwrap();
    for suffix in ["", "not-yet/created"] {
        let data = alias.join(suffix);
        let env = configured(&root, &data);
        assert_eq!(
            install_directory(&env, FontPlatformBackend::Fontconfig).unwrap(),
            actual.join(suffix).join("fonts")
        );
        assert_eq!(
            user_font_dir_for_backend(&env, FontPlatformBackend::Fontconfig),
            data.join("fonts")
        );
    }
    assert!(!actual.join("not-yet").exists());
}

#[test]
fn font_paths_bad_configured_roots_fail_without_falling_back() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::write(root.join("file"), b"keep").unwrap();
    symlink(root.join("missing"), root.join("dangling")).unwrap();
    for suffix in [
        "file",
        "file/child",
        "dangling",
        "dangling/child",
        "missing/../escape",
    ] {
        let env = configured(&root, &root.join(suffix));
        assert!(
            install_directory(&env, FontPlatformBackend::Fontconfig).is_err(),
            "{suffix}"
        );
    }
    assert!(!root.join(".local").exists());
    assert!(!root.join("escape").exists());
    assert_eq!(fs::read(root.join("file")).unwrap(), b"keep");
}
