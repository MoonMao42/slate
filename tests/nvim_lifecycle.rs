//! Bounded, isolated checks for the generated loader's libuv ownership and reloads.
#![cfg(feature = "has-nvim")]

use slate_cli::{adapter::NvimAdapter, env::SlateEnv, theme::ThemeRegistry};
use std::time::Duration;
use tempfile::TempDir;

fn run_loader_check(script: &str) {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let registry = ThemeRegistry::new().unwrap();
    NvimAdapter::setup(&env, registry.get("catppuccin-mocha").unwrap()).unwrap();
    let script_path = td.path().join("check.lua");
    std::fs::write(&script_path, script).unwrap();

    let output = assert_cmd::Command::new("nvim")
        .args([
            "--headless", "--noplugin", "-u", "NONE", "-i", "NONE", "-c",
            "lua local ok, err = xpcall(function() dofile(vim.env.SLATE_TEST_SCRIPT) end, debug.traceback); if not ok then vim.api.nvim_err_writeln(err); vim.cmd('cquit 1') end",
            "-c", "qa!",
        ])
        .env("HOME", td.path())
        .env("XDG_CONFIG_HOME", td.path().join(".config"))
        .env("XDG_CACHE_HOME", td.path().join(".cache"))
        .env("XDG_DATA_HOME", td.path().join(".local/share"))
        .env("XDG_STATE_HOME", td.path().join(".local/state"))
        .env_remove("NVIM_APPNAME")
        .env("SLATE_TEST_SCRIPT", &script_path)
        .env("SLATE_TEST_LOADER", env.nvim_config_dir().join("lua/slate/init.lua"))
        .env("SLATE_TEST_STATE", env.slate_cache_dir().join("current_theme.lua"))
        .timeout(Duration::from_secs(10))
        .assert()
        .success()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SLATE_NVIM_LIFECYCLE_OK"), "{stderr}");
    assert!(!stderr.contains("Error"), "{stderr}");
}

#[test]
fn nvim_loader_closes_handles_and_rejects_stale_callbacks() {
    run_loader_check(include_str!("fixtures/nvim_handle_lifecycle.lua"));
}

#[test]
fn nvim_loader_tracks_atomic_replacements_and_recreated_state() {
    run_loader_check(include_str!("fixtures/nvim_state_lifecycle.lua"));
}
