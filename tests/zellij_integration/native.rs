use super::*;

fn native(binary: &Path, env: &SlateEnv) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(binary);
    cmd.env_clear()
        .env("HOME", env.home())
        .env("XDG_CONFIG_HOME", env.xdg_config_home())
        .env("XDG_CACHE_HOME", env.home().join("cache"))
        .env("XDG_DATA_HOME", env.home().join("data"))
        .env("XDG_STATE_HOME", env.home().join("state"))
        .env("TMPDIR", env.home())
        .env("PATH", env.user_local_bin())
        .env("TERM", "xterm-256color")
        .current_dir(env.home())
        .arg("--config-dir")
        .arg(env.home().join(".config/zellij"))
        .arg("--config")
        .arg(ZellijAdapter::config_path(env).unwrap())
        .args(["setup", "--check"])
        .timeout(Duration::from_secs(8));
    cmd
}

#[test]
#[ignore = "requires explicit SLATE_ZELLIJ_BINARY; isolated native configuration parsing, no sessions"]
fn zellij_native_parses_all_palettes_and_rejects_a_corrupted_component() {
    let binary = std::env::var_os("SLATE_ZELLIJ_BINARY").expect("explicit Zellij executable");
    let binary = Path::new(&binary);
    assert!(binary.is_absolute());
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    let registry = ThemeRegistry::new().unwrap();
    for theme in registry.all() {
        ZellijAdapter.apply_theme_with_env(theme, &env).unwrap();
        let paths = ZellijAdapter::paths(&env).unwrap();
        let before = paths.clone().map(|p| fs::read(p).unwrap());
        native(binary, &env).assert().success();
        assert_eq!(paths.map(|p| fs::read(p).unwrap()), before);
    }
    // A negative control proves --check loaded the asset, not just config.kdl.
    let [config, asset] = ZellijAdapter::paths(&env).unwrap();
    let theme = fs::read_to_string(&asset).unwrap();
    seed(&asset, theme.replacen("base \"#", "base \"INVALID#", 1));
    let output = native(binary, &env).assert().failure().get_output().clone();
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostic.contains("slate-sync") || diagnostic.contains("INVALID"),
        "{diagnostic}"
    );
    // The value used by clean must also be accepted by the native program.
    seed(&asset, theme);
    seed(
        &config,
        "theme \"default\"\ntheme_dark \"default\"\ntheme_light \"default\"\n",
    );
    native(binary, &env).assert().success();
}
