use crate::ThemeResult;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use tempfile::Builder;

/// Auto-create minimal config file if tool is installed but config doesn't exist 
/// 1. Check if config_path exists; if yes, return Ok()
/// 2. If not, create parent directory via fs::create_dir_all()
/// 3. Generate minimal config template per tool
/// 4. Use atomic_write_file to write atomically
/// 5. Return Ok() on success, Err on failure
pub fn auto_create_config(tool: &str, config_path: &Path) -> ThemeResult<()> {
    // Generate minimal config template per tool
    let template = match tool {
        "ghostty" => "# slate: Auto-created minimal config\ntheme = \"Catppuccin Mocha\"\n",
        "starship" => "# slate: Auto-created minimal config\npalette = \"catppuccin-mocha\"\n",
        "bat" => "# slate: Auto-created minimal config\n--theme=\"Catppuccin Mocha\"\n",
        _ => return Ok(()), // Unknown tool, skip
    };

    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp_file = Builder::new().prefix(".slate.").tempfile_in(temp_dir)?;
    temp_file.write_all(template.as_bytes())?;
    temp_file.flush()?;

    match temp_file.persist_noclobber(config_path) {
        Ok(_) => Ok(()),
        Err(err) if err.error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err.error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_auto_create_config_ghostty() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("ghostty_config");

        let result = auto_create_config("ghostty", &config_path);
        assert!(result.is_ok());
        assert!(config_path.exists());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("theme"));
        assert!(content.contains("Catppuccin Mocha"));
    }

    #[test]
    fn test_auto_create_config_starship() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("starship_config");

        let result = auto_create_config("starship", &config_path);
        assert!(result.is_ok());
        assert!(config_path.exists());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("palette"));
    }

    #[test]
    fn test_auto_create_config_bat() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("bat_config");

        let result = auto_create_config("bat", &config_path);
        assert!(result.is_ok());
        assert!(config_path.exists());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("--theme"));
    }

    #[test]
    fn test_auto_create_config_skips_if_exists() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("existing_config");

        // Create the file first
        std::fs::write(&config_path, "existing content").unwrap();

        let result = auto_create_config("ghostty", &config_path);
        assert!(result.is_ok());

        // Verify content wasn't overwritten
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "existing content");
    }

    #[test]
    fn test_auto_create_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nested").join("dir").join("config");

        let result = auto_create_config("ghostty", &config_path);
        assert!(result.is_ok());
        assert!(config_path.exists());
    }

    #[test]
    fn test_auto_create_unknown_tool() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("unknown_config");

        let result = auto_create_config("unknown-tool", &config_path);
        assert!(result.is_ok());
        // Should not create file for unknown tool
        assert!(!config_path.exists());
    }
}
