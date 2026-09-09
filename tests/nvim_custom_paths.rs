//! One bounded runtime check for the configured cache path and live updates.
#![cfg(feature = "has-nvim")]

use slate_cli::{adapter::NvimAdapter, env::SlateEnv, theme::ThemeRegistry};
use std::process::Command;
use tempfile::TempDir;

#[test]
fn nvim_custom_paths_load_and_reload_the_configured_cache() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::from_vars(|key| match key {
        "HOME" => Some(td.path().as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(td.path().join("config root").into_os_string()),
        "XDG_CACHE_HOME" => Some(td.path().join("cache root").into_os_string()),
        "NVIM_APPNAME" => Some("profiles/work".into()),
        _ => None,
    })
    .unwrap();
    let registry = ThemeRegistry::new().unwrap();
    NvimAdapter::setup(&env, registry.get("catppuccin-mocha").unwrap()).unwrap();
    let loader = env.nvim_config_dir().join("lua/slate/init.lua");
    let state = env.slate_cache_dir().join("current_theme.lua");
    let script = format!(
        "local ok, err = pcall(function() dofile({loader:?}); \
         assert(vim.g.colors_name == 'slate-catppuccin-mocha', 'initial theme'); \
         vim.wait(150, function() return false end); \
         vim.fn.writefile({{'return \"nord\"'}}, {state:?}); \
         assert(vim.wait(2000, function() return vim.g.colors_name == 'slate-nord' end), 'live reload'); \
         print('SLATE_CUSTOM_PATHS_OK') end); \
         if not ok then vim.api.nvim_err_writeln(tostring(err)); vim.cmd('cquit 1') end",
        loader = loader.to_string_lossy(), state = state.to_string_lossy(),
    );
    let output = Command::new("nvim")
        .args([
            "--headless",
            "-u",
            "NONE",
            "-i",
            "NONE",
            "-c",
            &format!("lua {script}"),
            "-c",
            "qa!",
        ])
        .env("HOME", td.path())
        .env("XDG_CONFIG_HOME", td.path().join("unused-config"))
        .env("XDG_CACHE_HOME", td.path().join("unused-cache"))
        .env_remove("NVIM_APPNAME")
        .output()
        .expect("has-nvim requires the nvim executable");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("SLATE_CUSTOM_PATHS_OK"), "{stderr}");
}
