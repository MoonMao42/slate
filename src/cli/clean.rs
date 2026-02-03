use crate::env::SlateEnv;
use crate::error::Result;
use crate::{config::ConfigManager, platform};
use std::fs;
use std::path::Path;

/// Handle `slate clean` command
/// Removes managed files, stops the auto-theme watcher, and removes .zshrc marker block
/// Clean removes slate-managed assets; see 'slate restore' to recover from snapshot
pub fn handle_clean() -> Result<()> {
    use cliclack::{intro, log};

    intro("✦ Clean Up Slate")?;

    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    // Step 1: Stop watcher + clear config flag
    log::step("Stopping auto-theme watcher...")?;
    if let Err(err) = config.set_auto_theme_enabled(false) {
        log::remark(format!("  (couldn't update auto-theme flag: {})", err))?;
    }
    platform::dark_mode_notify::stop()?;
    platform::dark_mode_notify::remove_binary(&config)?;
    log::success("✓ Watcher stopped")?;

    // Step 2: Delete managed directory
    log::step("Removing managed files...")?;
    let managed_dir = env.config_dir().join("managed");
    if managed_dir.exists() {
        fs::remove_dir_all(&managed_dir)?;
        log::success("✓ Removed managed/")?;
    } else {
        log::remark("  (managed/ already removed)")?;
    }

    // Step 3: Remove marker block from .zshrc
    log::step("Removing shell integration...")?;
    remove_marker_block_from_zshrc(env.home())?;
    log::success("✓ Removed marker block")?;

    // Exit message (Clarify clean vs restore boundary)
    log::remark("")?;
    log::info(
        "✦ clean removed slate's managed files and watcher. \
Your original dotfiles were NOT restored. \
Use 'slate restore' to recover from a snapshot.",
    )?;
    log::remark("")?;

    Ok(())
}

/// Remove marker block from .zshrc
/// Handles multiple blocks and preserves rest of file content
fn remove_marker_block_from_zshrc(home: &Path) -> Result<()> {
    let zshrc_path = home.join(".zshrc");
    crate::adapter::marker_block::remove_managed_blocks_from_file(&zshrc_path)
}
