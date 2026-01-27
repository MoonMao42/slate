use crate::env::SlateEnv;
use crate::error::Result;
use crate::{config::ConfigManager, platform};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
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

    if !zshrc_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&zshrc_path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Find all marker blocks and collect their ranges (handles multiple blocks)
    let mut indices_to_remove = Vec::new();
    let mut in_block = false;
    let mut block_start = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("# slate:start") {
            if !in_block {
                in_block = true;
                block_start = i;
            }
        } else if line.trim().starts_with("# slate:end") && in_block {
            indices_to_remove.push(block_start..=i);
            in_block = false;
        }
    }

    // Reconstruct file without marker blocks
    let filtered_lines: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !indices_to_remove.iter().any(|r| r.contains(i)))
        .map(|(_, line)| *line)
        .collect();

    let new_content = filtered_lines.join("\n");

    // Write atomically using AtomicWriteFile::open
    let mut writer = AtomicWriteFile::open(&zshrc_path)?;
    writer.write_all(new_content.as_bytes())?;
    writer.commit()?;

    Ok(())
}
