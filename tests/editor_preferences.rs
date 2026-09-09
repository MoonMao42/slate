//! Consent, diagnostic and file-recovery checks without launching Neovim.
use slate_cli::{adapter::NvimAdapter, config::ConfigManager, env::SlateEnv, theme::ThemeRegistry};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree;

fn cli(home: &std::path::Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(6));
    command
}

#[test]
fn editor_preference_cli_diagnostics_and_restore_preserve_consent() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_sound_enabled(false).unwrap();
    NvimAdapter::setup(&env, ThemeRegistry::new().unwrap().get("nord").unwrap()).unwrap();
    let init = env.nvim_init_path();
    let user = "-- private user settings\nvim.g.private_fixture = true\n";
    let initial = format!(
        "{user}-- {}\npcall(require, 'slate')\n-- {}\n",
        slate_cli::adapter::marker_block::START,
        slate_cli::adapter::marker_block::END
    );
    std::fs::write(&init, &initial).unwrap();
    let baseline = slate_cli::config::begin_restore_point_baseline_with_env(&env).unwrap();
    let entry = baseline
        .entries
        .iter()
        .find(|entry| entry.tool_key == "nvim-auto-activation")
        .unwrap();
    assert_eq!(entry.original_path, env.nvim_auto_activation_path());
    assert_eq!(
        entry.original_state,
        slate_cli::config::OriginalFileState::Absent
    );

    cli(td.path())
        .args(["config", "set", "editor", "disable"])
        .assert()
        .success();
    assert!(!config.is_editor_auto_activation_enabled().unwrap());
    assert_eq!(std::fs::read_to_string(&init).unwrap(), user);
    assert!(env
        .nvim_config_dir()
        .join("colors/slate-nord.lua")
        .is_file());
    let before = tree::tree(td.path());
    let output = cli(td.path())
        .args(["doctor", "nvim", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = report["checks"].as_array().unwrap();
    assert!(checks
        .iter()
        .any(|check| check["code"] == "auto_activation" && check["status"] == "info"));
    assert!(!checks
        .iter()
        .any(|check| check["status"] == "warning" || check["status"] == "error"));
    assert_eq!(tree::tree(td.path()), before);
    let output = cli(td.path())
        .args(["clean", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["changes"].as_array().unwrap().iter().any(|change| {
        change["path"] == env.nvim_auto_activation_path().to_string_lossy().as_ref()
            && change["action"] == "remove"
    }));
    assert_eq!(tree::tree(td.path()), before);

    let restored = slate_cli::config::execute_restore_with_env(&env, &baseline.id).unwrap();
    assert!(config.is_editor_auto_activation_enabled().unwrap());
    assert_eq!(std::fs::read_to_string(&init).unwrap(), initial);
    slate_cli::config::execute_restore_with_env(&env, &restored.pre_restore_point_id).unwrap();
    assert!(!config.is_editor_auto_activation_enabled().unwrap());
    assert_eq!(std::fs::read_to_string(&init).unwrap(), user);
    cli(td.path())
        .args(["config", "set", "editor", "enable"])
        .assert()
        .success();
    assert!(config.is_editor_auto_activation_enabled().unwrap());
    assert_eq!(
        std::fs::read_to_string(&init).unwrap(),
        user,
        "enable must not silently insert a hook"
    );
}
