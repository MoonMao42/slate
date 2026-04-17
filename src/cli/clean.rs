use crate::adapter::{GhosttyAdapter, ToolAdapter};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::{config::ConfigManager, platform};
use std::fs;
use std::path::Path;

/// Handle `slate clean` command
/// Removes managed files, stops the auto-theme watcher, and removes.zshrc marker block
/// Clean removes slate-managed assets; see 'slate restore' to recover from snapshot
pub fn handle_clean() -> Result<()> {
    use cliclack::{intro, log};

    intro("✦ Clean Up Slate")?;

    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    // Step 0: Snapshot the current state so the user can undo this clean. Without this
    // the only restore point after clean is the pre-slate baseline, which is the wrong
    // target if the user just wants to roll back the clean itself. Best-effort — we don't
    // want a backup hiccup to block the clean.
    {
        let label = config
            .get_current_theme()
            .ok()
            .flatten()
            .map(|theme| format!("pre-clean-{}", theme))
            .unwrap_or_else(|| "pre-clean".to_string());
        if let Err(err) = crate::config::snapshot_current_state_with_env(&env, &label) {
            log::remark(format!("  (couldn't create pre-clean snapshot: {})", err))?;
        } else {
            log::success(format!("✓ Saved pre-clean snapshot ({})", label))?;
        }
    }

    // Step 1: Stop watcher + clear config flag
    log::step("Stopping auto-theme watcher...")?;
    if let Err(err) = config.set_auto_theme_enabled(false) {
        log::remark(format!("  (couldn't update auto-theme flag: {})", err))?;
    }
    platform::dark_mode_notify::stop()?;
    platform::dark_mode_notify::remove_binary(&config)?;
    log::success("✓ Watcher stopped")?;

    // Step 2: Remove integration references before deleting managed files
    log::step("Removing integration references...")?;
    remove_marker_block_from_zshrc(env.home())?;
    remove_marker_blocks_from_bash(&env)?;
    remove_fish_loader(&env)?;
    remove_ghostty_managed_references(&env)?;
    remove_alacritty_managed_references(&env)?;
    remove_tmux_managed_references(env.home())?;
    remove_delta_managed_references(env.home())?;
    log::success("✓ Removed config-file/import/source hooks")?;

    // Step 3: Delete Slate-owned config directory
    log::step("Removing Slate-managed config state...")?;
    let config_dir = env.config_dir();
    if config_dir.exists() {
        fs::remove_dir_all(config_dir)?;
        log::success("✓ Removed ~/.config/slate")?;
    } else {
        log::remark("  (~/.config/slate already removed)")?;
    }

    // Step 4: Reload running terminals so the theme actually drops.
    // Removing the config-file line from ~/.config/ghostty/config only takes effect on the
    // next reload; without this, users see "clean succeeded" but the background + palette
    // stay applied until they restart Ghostty themselves. Best-effort — if the terminal
    // isn't running we silently move on.
    let _ = GhosttyAdapter.reload();

    // Exit message: Clarify clean vs restore boundary)
    log::remark("")?;
    log::info(
        "✦ clean removed Slate-owned shell hooks, watcher artifacts, and config state. \
Third-party tools installed through Homebrew remain installed. \
Use 'slate restore' before cleanup if you want to roll back to a snapshot instead.",
    )?;
    log::remark("")?;

    Ok(())
}

/// Remove marker block from.zshrc
/// Handles multiple blocks and preserves rest of file content
fn remove_marker_block_from_zshrc(home: &Path) -> Result<()> {
    let zshrc_path = home.join(".zshrc");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&zshrc_path)
}

/// Remove marker blocks from any bash rc file Slate might have written to.
///
/// On macOS we may have written to `.bash_profile` (login-shell convention); on Linux we
/// write to `.bashrc`. Sweep both so a reinstall/clean across machines or a migration from
/// an older slate version still leaves no orphaned loaders. `remove_managed_blocks_from_file`
/// is a no-op on missing files, so unconditional calls are safe.
fn remove_marker_blocks_from_bash(env: &SlateEnv) -> Result<()> {
    crate::adapter::marker_block::remove_managed_blocks_from_file(&env.bashrc_path())?;
    crate::adapter::marker_block::remove_managed_blocks_from_file(&env.bash_profile_path())?;
    Ok(())
}

fn remove_fish_loader(env: &SlateEnv) -> Result<()> {
    match fs::remove_file(env.fish_loader_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn remove_ghostty_managed_references(env: &SlateEnv) -> Result<()> {
    let adapter = crate::adapter::GhosttyAdapter;
    let integration_path = adapter.integration_config_path_with_env(env)?;
    if !integration_path.exists() {
        return Ok(());
    }

    let managed_prefix = env
        .config_dir()
        .join("managed")
        .join("ghostty")
        .to_string_lossy()
        .to_string();
    let content = fs::read_to_string(&integration_path)?;
    let mut cleaned = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("config-file") && trimmed.contains(&managed_prefix) {
            continue;
        }
        cleaned.push(line);
    }

    let new_content = if cleaned.is_empty() {
        String::new()
    } else {
        format!("{}\n", cleaned.join("\n"))
    };
    fs::write(integration_path, new_content)?;
    Ok(())
}

fn remove_alacritty_managed_references(env: &SlateEnv) -> Result<()> {
    let integration_path =
        crate::adapter::alacritty::AlacrittyAdapter::integration_config_path_with_env(env);
    if !integration_path.exists() {
        return Ok(());
    }

    let managed_prefix = env
        .config_dir()
        .join("managed")
        .join("alacritty")
        .to_string_lossy()
        .to_string();
    let content = fs::read_to_string(&integration_path)?;
    let mut doc: toml_edit::DocumentMut = if content.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        content.parse().map_err(|e| {
            crate::error::SlateError::InvalidConfig(format!(
                "Failed to parse Alacritty TOML during clean: {}",
                e
            ))
        })?
    };

    if let Some(imports) = doc
        .get_mut("general")
        .and_then(|general| general.get_mut("import"))
        .and_then(|imports| imports.as_array_mut())
    {
        let retained: Vec<String> = imports
            .iter()
            .filter_map(|item| item.as_str())
            .filter(|path| !path.contains(&managed_prefix))
            .map(ToString::to_string)
            .collect();

        imports.clear();
        for path in retained {
            imports.push(path);
        }
    }

    if doc
        .get("general")
        .and_then(|general| general.as_table())
        .and_then(|table| table.get("import"))
        .and_then(|imports| imports.as_array())
        .is_some_and(|imports| imports.is_empty())
    {
        if let Some(general) = doc.get_mut("general").and_then(|item| item.as_table_mut()) {
            general.remove("import");
        }
    }

    if doc
        .get("general")
        .and_then(|general| general.as_table())
        .is_some_and(|table| table.is_empty())
    {
        doc.remove("general");
    }

    fs::write(integration_path, doc.to_string())?;
    Ok(())
}

fn remove_tmux_managed_references(home: &Path) -> Result<()> {
    let tmux_path = home.join(".tmux.conf");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&tmux_path)
}

fn remove_delta_managed_references(home: &Path) -> Result<()> {
    let gitconfig_path = home.join(".gitconfig");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&gitconfig_path)
}
