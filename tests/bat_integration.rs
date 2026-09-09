//! Public bat apply uses private executables and profiles, never the host cache.
use slate_cli::{
    adapter::{ApplyOutcome, BatAdapter, ToolAdapter},
    detection::{detect_tool_presence_with_env, ToolEvidence},
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

#[path = "support/tree.rs"]
mod snapshot;

#[test]
#[ignore = "invoked by the parent with private paths and a deadline"]
fn bat_apply_private_child() {
    let root = PathBuf::from(std::env::var_os("SLATE_BAT_FIXTURE").unwrap());
    let home = root.join("injected 主题");
    let env = SlateEnv::with_home(home.clone());
    let adapter = BatAdapter;
    assert_eq!(
        adapter.integration_config_path_with_env(&env).unwrap(),
        home.join(".config/bat/config")
    );
    assert!(adapter.is_installed_with_env(&env).unwrap());
    assert_eq!(
        detect_tool_presence_with_env("bat", &env).evidence,
        Some(ToolEvidence::Executable(home.join(".local/bin/bat")))
    );
    let untouched = snapshot::tree(&root.join("ambient"));
    let registry = ThemeRegistry::new().unwrap();
    let theme = registry.all()[0];
    assert_eq!(
        adapter.apply_theme_with_env(theme, &env).unwrap(),
        ApplyOutcome::applied_needs_new_shell()
    );
    let fields = [
        home.join(".local/bin/bat"),
        home.clone(),
        home.join(".config"),
        home.join(".cache"),
        home.join(".config/bat"),
        home.join(".cache/bat"),
        home.join(".config/bat/config"),
        PathBuf::from("unset"),
        PathBuf::from("unset"),
        PathBuf::from("cache"),
        PathBuf::from("--build"),
    ];
    let expected: Vec<u8> = fields
        .iter()
        .flat_map(|field| {
            field
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .copied()
                .chain([0])
        })
        .collect();
    assert_eq!(fs::read(root.join("invocations")).unwrap(), expected);
    assert_eq!(snapshot::tree(&root.join("ambient")), untouched);
    assert_eq!(
        fs::read_dir(adapter.themes_dir(&env)).unwrap().count(),
        registry.all().len()
    );
    assert!(!home.join(".config/bat/config").exists());
    let before = snapshot::tree(&home);
    adapter.apply_theme_with_env(theme, &env).unwrap();
    assert_eq!(snapshot::tree(&home), before);
    assert_eq!(
        fs::read(root.join("invocations")).unwrap(),
        expected.repeat(2)
    );
}

#[test]
#[ignore = "invoked by the parent with private paths and a deadline"]
fn bat_custom_paths_private_child() {
    let root = PathBuf::from(std::env::var_os("SLATE_BAT_FIXTURE").unwrap());
    let cwd = std::env::current_dir().unwrap();
    let home = root.join("custom-home");
    let raw_name = if cfg!(target_os = "linux") {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"assets-\xff".to_vec())
    } else {
        std::ffi::OsString::from("assets 主题")
    };
    let env = SlateEnv::from_vars(|name| match name {
        "HOME" => Some(home.as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(root.join("custom-config").into_os_string()),
        "XDG_CACHE_HOME" => Some(root.join("custom-cache").into_os_string()),
        "BAT_CONFIG_DIR" => Some(raw_name.clone()),
        "BAT_CACHE_PATH" => Some("compiled cache".into()),
        "BAT_CONFIG_PATH" => Some("separate/batrc".into()),
        _ => None,
    })
    .unwrap();
    assert_eq!(env.bat_config_dir(), cwd.join(&raw_name));
    assert_eq!(env.bat_cache_dir(), cwd.join("compiled cache"));
    assert_eq!(
        BatAdapter.integration_config_path_with_env(&env).unwrap(),
        cwd.join("separate/batrc")
    );
    let bin = root.join("path/batcat");
    let presence = detect_tool_presence_with_env("bat", &env);
    assert!(presence.in_path);
    assert_eq!(
        presence.evidence,
        Some(ToolEvidence::Executable(bin.clone()))
    );
    let home_before = snapshot::tree(&home);
    let ambient_before = snapshot::tree(&root.join("ambient"));
    fs::create_dir_all(root.join("later-cwd")).unwrap();
    // This is an isolated child process: prove env cloning survives a cwd
    // change without mutating the parent test runner's environment or cwd.
    std::env::set_current_dir(root.join("later-cwd")).unwrap();
    let env = env.clone();
    let registry = ThemeRegistry::new().unwrap();
    BatAdapter
        .apply_theme_with_env(registry.all()[0], &env)
        .unwrap();
    let expected: Vec<u8> = [
        bin.as_path(),
        env.home(),
        env.xdg_config_home(),
        env.cache_dir(),
        env.bat_config_dir(),
        env.bat_cache_dir(),
        env.bat_config_path(),
    ]
    .iter()
    .flat_map(|p| p.as_os_str().as_encoded_bytes().iter().copied().chain([0]))
    .collect();
    assert_eq!(fs::read(root.join("invocations")).unwrap(), expected);
    assert_eq!(snapshot::tree(&home), home_before);
    assert_eq!(snapshot::tree(&root.join("ambient")), ambient_before);
    assert_eq!(fs::read_dir(root.join("later-cwd")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(BatAdapter.themes_dir(&env)).unwrap().count(),
        registry.all().len()
    );
    assert!(!env.bat_config_path().exists());
}

#[test]
fn bat_custom_paths_are_frozen_and_path_alias_beats_fallback() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    let path_bin = root.join("path/batcat");
    fs::create_dir_all(path_bin.parent().unwrap()).unwrap();
    fs::write(&path_bin, concat!(
        "#!/bin/sh\n[ \"$#\" -eq 2 ] && [ \"$1\" = cache ] && [ \"$2\" = --build ] || exit 91\n",
        "[ -z \"${BAT_THEME+x}\" ] && [ -z \"${BAT_OPTS+x}\" ] || exit 92\n",
        "printf '%s\\0' \"$0\" \"$HOME\" \"$XDG_CONFIG_HOME\" \"$XDG_CACHE_HOME\" ",
        "\"$BAT_CONFIG_DIR\" \"$BAT_CACHE_PATH\" \"$BAT_CONFIG_PATH\" > \"$SLATE_BAT_FIXTURE/invocations\"\n",
    )).unwrap();
    fs::set_permissions(&path_bin, fs::Permissions::from_mode(0o755)).unwrap();
    let fallback = root.join("custom-home/.local/bin/bat");
    fs::create_dir_all(fallback.parent().unwrap()).unwrap();
    fs::write(&fallback, "#!/bin/sh\nexit 93\n").unwrap();
    fs::set_permissions(&fallback, fs::Permissions::from_mode(0o755)).unwrap();
    let ambient = root.join("ambient");
    fs::create_dir_all(&ambient).unwrap();
    fs::write(ambient.join("sentinel"), "original").unwrap();
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("HOME", &ambient)
        .env("PATH", path_bin.parent().unwrap())
        .env("BAT_CONFIG_PATH", ambient.join("batrc"))
        .env("BAT_CONFIG_DIR", ambient.join("config"))
        .env("BAT_CACHE_PATH", ambient.join("cache"))
        .env("BAT_THEME", "host-theme")
        .env("BAT_OPTS", "host-options")
        .env("SLATE_BAT_FIXTURE", root)
        .current_dir(root)
        .args([
            "--exact",
            "bat_custom_paths_private_child",
            "--ignored",
            "--nocapture",
        ])
        .timeout(Duration::from_secs(7))
        .assert()
        .success();
}

#[test]
#[ignore = "invoked by the parent with private paths and a deadline"]
fn bat_failure_commit_private_child() {
    use slate_cli::{cli::theme_apply::ThemeApplyCoordinator, config::ConfigManager};
    let env = SlateEnv::from_process().unwrap();
    assert_eq!(
        detect_tool_presence_with_env("bat", &env).evidence,
        Some(ToolEvidence::Executable(env.home().join("bin/bat")))
    );
    let config = ConfigManager::with_env(&env).unwrap();
    fs::write(env.managed_file("current"), "nord").unwrap();
    let registry = ThemeRegistry::new().unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .apply_to_tools(registry.get("catppuccin-mocha").unwrap(), &["bat".into()])
        .unwrap();
    assert_eq!(report.failed_count(), 1);
    assert_eq!(report.applied_count(), 0);
    assert!(report.ensure_no_failures().is_err());
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    assert!(!env.managed_file("managed/shell/env.zsh").exists());
    // Asset writes precede the failing external build. Do not pretend they
    // were rolled back together with the uncommitted theme selection.
    assert_eq!(
        fs::read_dir(BatAdapter.themes_dir(&env)).unwrap().count(),
        registry.all().len()
    );
    assert_eq!(fs::read(env.home().join("calls")).unwrap(), b"call\n");
}

#[test]
fn bat_cache_failure_never_commits_a_new_theme() {
    for body in [
        "exit 7",
        "printf \"bat has been built without the 'build-assets' feature.\\n\"",
    ] {
        let td = tempfile::tempdir().unwrap();
        let bin = td.path().join("bin/bat");
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(
            &bin,
            format!("#!/bin/sh\nprintf 'call\\n' >> \"$HOME/calls\"\n{body}\n"),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", bin.parent().unwrap())
            .current_dir(td.path())
            .args([
                "--exact",
                "bat_failure_commit_private_child",
                "--ignored",
                "--nocapture",
            ])
            .timeout(Duration::from_secs(7))
            .assert()
            .success();
    }
}

#[test]
fn bat_apply_uses_injected_home_fallback_and_private_cache() {
    let td = tempfile::tempdir().unwrap();
    let bin = td.path().join("injected 主题/.local/bin");
    fs::create_dir_all(&bin).unwrap();
    // A private bat prevents fallback lookup from reaching a host installation.
    let executable = bin.join("bat");
    fs::write(&executable, concat!(
        "#!/bin/sh\n",
        "printf '%s\\0' \"$0\" \"$HOME\" \"$XDG_CONFIG_HOME\" \"$XDG_CACHE_HOME\" ",
        "\"$BAT_CONFIG_DIR\" \"$BAT_CACHE_PATH\" \"$BAT_CONFIG_PATH\" ",
        "\"${BAT_THEME-unset}\" \"${BAT_OPTS-unset}\" \"$@\" >> \"$SLATE_BAT_FIXTURE/invocations\"\n",
    )).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let ambient = td.path().join("ambient");
    fs::create_dir_all(&ambient).unwrap();
    fs::write(ambient.join("sentinel"), "must remain unchanged").unwrap();
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("HOME", &ambient)
        .env("SLATE_HOME", &ambient)
        .env("PATH", td.path().join("empty-path"))
        .env("BAT_CONFIG_DIR", ambient.join("bat-config"))
        .env("BAT_CONFIG_PATH", ambient.join("batrc"))
        .env("BAT_CACHE_PATH", ambient.join("bat-cache"))
        .env("BAT_THEME", "host-theme")
        .env("BAT_OPTS", "host-options")
        .env("SLATE_BAT_FIXTURE", td.path())
        .current_dir(td.path())
        .args([
            "--exact",
            "bat_apply_private_child",
            "--ignored",
            "--nocapture",
        ])
        .timeout(Duration::from_secs(7))
        .assert()
        .success();
}
