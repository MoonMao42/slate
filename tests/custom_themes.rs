//! Custom themes must remain usable after the initial catalog lookup.
//! Every command runs against a disposable profile with private tool stubs.
use slate_cli::{
    adapter::{BatAdapter, BtopAdapter},
    cli::{picker::PickerState, set, theme_apply::ThemeApplyCoordinator},
    config::{execute_restore_with_env, get_restore_point_with_env, OriginalFileState},
    env::SlateEnv,
    opacity::OpacityPreset,
    theme::{custom::parse_custom_themes_str, ThemeRegistry},
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[path = "support/tree.rs"]
mod snapshot;

const THEME_ID: &str = "review-custom";

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let mut theme = ThemeRegistry::new()
        .unwrap()
        .get("catppuccin-mocha")
        .unwrap()
        .clone();
    theme.id = THEME_ID.into();
    theme.name = "Review Custom".into();
    theme.family = "Review".into();
    theme.palette.background = "#102030".into();
    theme.tool_refs.clear(); // Exercise custom defaults, including slate-review-custom for bat.
    theme.auto_pair = None;
    let source = toml::to_string(&theme).unwrap();
    let parsed = parse_custom_themes_str(&source).unwrap();
    assert_eq!(parsed[0].tool_refs["bat"], "slate-review-custom");
    write(&env.config_dir().join("themes/review-custom.toml"), &source);

    let bat = env.user_local_bin().join("bat");
    write(
        &bat,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/bat-cache-calls\"\n",
    );
    fs::set_permissions(&bat, fs::Permissions::from_mode(0o700)).unwrap();
    let btop = env.user_local_bin().join("btop");
    write(
        &btop,
        "#!/bin/sh\nprintf called > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    );
    fs::set_permissions(&btop, fs::Permissions::from_mode(0o700)).unwrap();
    write(
        &BtopAdapter::config_path(&env),
        "color_theme=personal\ntheme_background=false\n",
    );
    (home, env)
}

fn command(env: &SlateEnv) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", env.home())
        .env("SLATE_HOME", env.home())
        .env("PATH", env.user_local_bin())
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(10));
    command
}

fn json(env: &SlateEnv, args: &[&str]) -> serde_json::Value {
    let output = command(env)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}

// Direct library calls must resolve the fixture's tools even when the host has
// bat installed. Leave SLATE_HOME unset to exercise the injected HOME binding.
fn run_in_private_process(test_name: &str) -> bool {
    if std::env::var("SLATE_CUSTOM_THEME_TEST").as_deref() == Ok(test_name) {
        return false;
    }
    let home = tempfile::tempdir().unwrap();
    assert_cmd::Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .env("SLATE_CUSTOM_THEME_TEST", test_name)
        .args(["--exact", test_name, "--nocapture"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
    true
}

#[test]
fn custom_theme_survives_cli_apply_sync_pairing_and_prompt_import() {
    let (home, env) = fixture();
    let before_reads = snapshot::tree(home.path());
    let ids = command(&env)
        .args(["list", "--ids"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(ids)
        .unwrap()
        .lines()
        .any(|id| id == THEME_ID));
    assert_eq!(snapshot::tree(home.path()), before_reads);

    command(&env).args(["theme", THEME_ID]).assert().success();
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        THEME_ID
    );
    let shell = env.managed_file("managed/shell/env.bash");
    assert!(fs::read_to_string(&shell)
        .unwrap()
        .contains("export BAT_THEME='slate-review-custom'"));
    assert_eq!(
        json(&env, &["status", "--json"])["theme"]["name"],
        "Review Custom"
    );
    let bat_theme = BatAdapter
        .themes_dir(&env)
        .join("slate-review-custom.tmTheme");
    assert!(fs::read_to_string(&bat_theme)
        .unwrap()
        .contains("<string>slate-review-custom</string>"));
    assert!(fs::read_to_string(home.path().join("bat-cache-calls"))
        .unwrap()
        .contains("cache --build"));

    let btop_theme = BtopAdapter::theme_path(&env);
    fs::rename(&btop_theme, home.path().join("previous-btop-theme")).unwrap();
    let before_sync_preview = snapshot::tree(home.path());
    let sync = json(&env, &["tools", "sync", "btop", "--dry-run", "--json"]);
    assert_eq!(sync["theme"], THEME_ID);
    assert_eq!(snapshot::tree(home.path()), before_sync_preview);
    command(&env)
        .args(["tools", "sync", "btop", "--yes"])
        .assert()
        .success();
    assert!(fs::read_to_string(&btop_theme)
        .unwrap()
        .contains("theme[main_bg]=\"#102030\""));
    assert!(fs::read_to_string(&shell)
        .unwrap()
        .contains("slate-review-custom"));

    write(&env.managed_file("auto.toml"), "dark_theme = 'nord'\n");
    let before_pair_preview = snapshot::tree(home.path());
    let preview = json(
        &env,
        &[
            "config",
            "pairing",
            "--dark",
            THEME_ID,
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(preview["pairing"]["dark"]["theme_id"], THEME_ID);
    assert_eq!(snapshot::tree(home.path()), before_pair_preview);
    let saved = json(&env, &["config", "pairing", "--dark", THEME_ID, "--json"]);
    assert_eq!(saved["action"], "saved");
    assert_eq!(saved["pairing"]["dark"]["theme_id"], THEME_ID);
    assert_eq!(
        json(&env, &["config", "pairing", "--json"])["pairing"]["dark"]["theme_id"],
        THEME_ID
    );

    let prompt_only = "slate://v1/none/none/none/none?prompt=compact";
    let before_import_preview = snapshot::tree(home.path());
    assert_eq!(
        json(&env, &["import", prompt_only, "--dry-run", "--json"])["prompt"],
        "compact"
    );
    assert_eq!(snapshot::tree(home.path()), before_import_preview);
    command(&env)
        .args(["import", prompt_only])
        .assert()
        .success();
    assert!(fs::read_to_string(&shell)
        .unwrap()
        .contains("export BAT_THEME='slate-review-custom'"));
    assert_eq!(json(&env, &["status", "--json"])["theme"]["id"], THEME_ID);
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
}

#[test]
fn picker_preview_and_save_accept_a_custom_theme_from_the_injected_profile() {
    if run_in_private_process(
        "picker_preview_and_save_accept_a_custom_theme_from_the_injected_profile",
    ) {
        return;
    }
    let (_home, env) = fixture();
    let state = PickerState::with_env(&env, THEME_ID, OpacityPreset::Solid).unwrap();
    assert_eq!(state.get_current_theme_id(), THEME_ID);
    assert!(state.theme_ids().iter().any(|id| id == THEME_ID));
    set::silent_preview_apply(&env, THEME_ID, OpacityPreset::Solid).unwrap();
    assert!(!env.managed_file("current").exists());
    set::silent_commit_apply(
        &env,
        THEME_ID,
        OpacityPreset::Solid,
        "catppuccin-mocha",
        OpacityPreset::Solid,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        THEME_ID
    );
    assert!(fs::read_to_string(env.home().join("bat-cache-calls"))
        .unwrap()
        .contains("cache --build"));
    assert!(!env.home().join("UNEXPECTED_PROCESS").exists());
}

#[test]
fn custom_bat_asset_has_exact_checkpoint_recovery_for_existing_and_absent_files() {
    if run_in_private_process(
        "custom_bat_asset_has_exact_checkpoint_recovery_for_existing_and_absent_files",
    ) {
        return;
    }
    for existing in [false, true] {
        let (_home, env) = fixture();
        let asset = BatAdapter
            .themes_dir(&env)
            .join("slate-review-custom.tmTheme");
        let original = "private custom bat asset\n";
        if existing {
            write(&asset, original);
            fs::set_permissions(&asset, fs::Permissions::from_mode(0o640)).unwrap();
        }
        let theme = ThemeRegistry::with_env(&env)
            .unwrap()
            .get(THEME_ID)
            .unwrap()
            .clone();
        let report = ThemeApplyCoordinator::new(&env)
            .apply_to_tools(&theme, &["bat".into()])
            .unwrap();
        report.ensure_no_failures().unwrap();
        assert_eq!(
            fs::read_to_string(env.home().join("bat-cache-calls")).unwrap(),
            "cache --build\n"
        );
        let point =
            get_restore_point_with_env(&env, report.restore_point_id.as_ref().unwrap()).unwrap();
        let entry = point
            .entries
            .iter()
            .find(|entry| entry.original_path == asset)
            .unwrap();
        assert_eq!(
            entry.original_state,
            if existing {
                OriginalFileState::Present
            } else {
                OriginalFileState::Absent
            }
        );
        if existing {
            assert_eq!(
                fs::read_to_string(entry.backup_path.as_ref().unwrap()).unwrap(),
                original
            );
        }
        assert!(fs::read_to_string(&asset)
            .unwrap()
            .contains("<string>slate-review-custom</string>"));
        assert!(execute_restore_with_env(&env, &point.id)
            .unwrap()
            .is_fully_successful());
        if existing {
            assert_eq!(fs::read_to_string(&asset).unwrap(), original);
            assert_eq!(
                fs::metadata(&asset).unwrap().permissions().mode() & 0o777,
                0o640
            );
        } else {
            assert!(!asset.exists());
        }
    }
}
