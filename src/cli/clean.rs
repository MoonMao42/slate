use crate::env::SlateEnv;
use crate::error::Result;
use crate::{config::ConfigManager, platform};
use std::fs;
use std::path::Path;

/// Handle `slate clean` command
/// Removes managed files, stops the auto-theme watcher, and removes .zshrc marker block
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

    // Exit message (brand text)
    log::remark("")?;
    log::info(
        "slate clean removed slate's managed files and watcher. \
Your original dotfiles were NOT restored. Backups remain under \
~/.cache/slate/backups, but the restore CLI is not exposed yet.",
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

    if indices_to_remove.is_empty() {
        // No marker blocks found — nothing to clean
        return Ok(());
    }

    // Remove blocks in reverse order (to maintain indices)
    let mut cleaned_lines = lines.clone();
    for range in indices_to_remove.iter().rev() {
        let start = *range.start();
        let count = *range.end() - start + 1;
        for _ in 0..count {
            if start < cleaned_lines.len() {
                cleaned_lines.remove(start);
            }
        }
    }

    // Verify no orphaned markers remain after removal
    for line in &cleaned_lines {
        if line.trim().starts_with("# slate:start") || line.trim().starts_with("# slate:end") {
            return Err(crate::error::SlateError::Internal(
                "Orphaned marker block detected in .zshrc after cleanup — manual review needed"
                    .to_string(),
            ));
        }
    }

    // Reconstruct content and preserve trailing newline if it existed
    let cleaned = cleaned_lines.join("\n");
    let output = if content.ends_with('\n') {
        format!("{}\n", cleaned)
    } else {
        cleaned
    };

    fs::write(&zshrc_path, output)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_remove_marker_block_no_zshrc() {
        // If .zshrc doesn't exist, should not error
        let tempdir = TempDir::new().unwrap();
        let result = remove_marker_block_from_zshrc(tempdir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_marker_block_no_markers() {
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        fs::write(&zshrc_path, "export PATH=/usr/local/bin:$PATH\n").unwrap();

        let result = remove_marker_block_from_zshrc(tempdir.path());
        assert!(result.is_ok());

        let content = fs::read_to_string(&zshrc_path).unwrap();
        assert_eq!(content, "export PATH=/usr/local/bin:$PATH\n");
    }

    #[test]
    fn test_remove_marker_block_single_block() {
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        let content = "export PATH=/usr/local/bin:$PATH\n# slate:start\nexport SLATE=1\n# slate:end\necho 'done'\n";
        fs::write(&zshrc_path, content).unwrap();

        remove_marker_block_from_zshrc(tempdir.path()).unwrap();

        let result = fs::read_to_string(&zshrc_path).unwrap();
        assert_eq!(result, "export PATH=/usr/local/bin:$PATH\necho 'done'\n");
        assert!(!result.contains("slate:start"));
        assert!(!result.contains("slate:end"));
        assert!(!result.contains("SLATE=1"));
    }

    #[test]
    fn test_remove_marker_block_multiple_blocks() {
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        let content = "# slate:start\nblock1\n# slate:end\necho middle\n# slate:start\nblock2\n# slate:end\necho end\n";
        fs::write(&zshrc_path, content).unwrap();

        remove_marker_block_from_zshrc(tempdir.path()).unwrap();

        let result = fs::read_to_string(&zshrc_path).unwrap();
        assert_eq!(result, "echo middle\necho end\n");
        assert!(!result.contains("block1"));
        assert!(!result.contains("block2"));
    }

    #[test]
    fn test_remove_marker_block_with_spaces() {
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        let content = "echo start\n  # slate:start\nslate config\n  # slate:end\necho end\n";
        fs::write(&zshrc_path, content).unwrap();

        remove_marker_block_from_zshrc(tempdir.path()).unwrap();

        let result = fs::read_to_string(&zshrc_path).unwrap();
        assert_eq!(result, "echo start\necho end\n");
        assert!(!result.contains("slate config"));
    }

    #[test]
    fn test_remove_marker_block_preserves_trailing_newline() {
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        let content = "echo line1\n# slate:start\nhidden\n# slate:end\necho line2\n";
        fs::write(&zshrc_path, content).unwrap();

        remove_marker_block_from_zshrc(tempdir.path()).unwrap();

        let result = fs::read_to_string(&zshrc_path).unwrap();
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn test_remove_marker_block_nested_markers_returns_error() {
        // Nested markers leave orphans after first-level removal.
        // The orphan detection now catches this and returns Err
        // instead of silently leaving a broken .zshrc.
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        let content = "# slate:start\nouter\n# slate:start\ninner\n# slate:end\n# slate:end\n";
        fs::write(&zshrc_path, content).unwrap();

        let result = remove_marker_block_from_zshrc(tempdir.path());
        assert!(
            result.is_err(),
            "Nested markers should trigger orphan detection error"
        );
    }

    #[test]
    fn test_remove_marker_block_empty_block() {
        // Marker block with no content between start and end
        let tempdir = TempDir::new().unwrap();
        let zshrc_path = tempdir.path().join(".zshrc");
        let content = "echo before
# slate:start
# slate:end
echo after
";
        fs::write(&zshrc_path, content).unwrap();

        remove_marker_block_from_zshrc(tempdir.path()).unwrap();

        let result = fs::read_to_string(&zshrc_path).unwrap();
        assert_eq!(
            result,
            "echo before
echo after
"
        );
        assert!(!result.contains("slate:start"));
    }
}
