use crate::adapter::ToolAdapter;
use crate::config::backup::create_backup;
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use directories::ProjectDirs;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use toml_edit::DocumentMut;

pub struct StarshipAdapter;

impl StarshipAdapter {
    /// Get the Starship config directory path.
    fn get_config_dir() -> ThemeResult<PathBuf> {
        let proj_dirs = ProjectDirs::from("", "", "starship")
            .ok_or_else(|| ThemeError::Other("Cannot determine config directory".to_string()))?;
        
        let config_dir = proj_dirs.config_dir().to_path_buf();
        Ok(config_dir)
    }
}

impl ToolAdapter for StarshipAdapter {
    fn is_installed(&self) -> ThemeResult<bool> {
        // Check if binary exists in PATH
        let binary_exists = which::which("starship").is_ok();
        
        // Check if config file exists
        let config_exists = match self.config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };
        
        // Tool is installed if both binary AND config exist (Starship requires config)
        Ok(binary_exists && config_exists)
    }

    fn config_path(&self) -> ThemeResult<PathBuf> {
        let config_dir = Self::get_config_dir()?;
        Ok(config_dir.join("starship.toml"))
    }

    fn config_exists(&self) -> ThemeResult<bool> {
        let path = self.config_path()?;
        Ok(path.exists() && path.is_file())
    }

    fn apply_theme(&self, theme: &Theme) -> ThemeResult<()> {
        // Get canonical path (resolve symlinks)
        let config_path = self.config_path()?;
        let canonical_path = fs::canonicalize(&config_path)
            .map_err(|_e| ThemeError::SymlinkError {
                path: config_path.display().to_string(),
            })?;

        // Create backup before modification (SAFE-04)
        let _backup_info = create_backup("starship", &theme.name, &canonical_path)?;

        // Read config file as string
        let content = fs::read_to_string(&canonical_path)
            .map_err(|e| ThemeError::Io(e))?;

        // Parse using toml-edit (SAFE-02: preserves comments and formatting)
        let mut doc: DocumentMut = content.parse()
            .map_err(|e: toml_edit::TomlError| ThemeError::InvalidToml {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;

        // Get the Starship palette name from tool_overrides
        let palette_name = theme
            .colors
            .tool_overrides
            .get("starship")
            .ok_or_else(|| ThemeError::Other(
                format!("No Starship theme override for {}", theme.name)
            ))?
            .to_string();

        // Modify the palette key in the document root using toml_edit::value
        doc["palette"] = toml_edit::value(palette_name);

        // Get the modified content as string
        let new_content = doc.to_string();

        // Atomic write
        let mut file = AtomicWriteFile::open(&canonical_path)
            .map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;
        
        file.write_all(new_content.as_bytes())
            .map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;
        
        file.commit()
            .map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    fn get_current_theme(&self) -> ThemeResult<Option<String>> {
        if !self.config_exists()? {
            return Ok(None);
        }

        let path = self.config_path()?;
        let content = fs::read_to_string(&path)
            .map_err(|e| ThemeError::Io(e))?;

        let doc: DocumentMut = content.parse()
            .map_err(|e: toml_edit::TomlError| ThemeError::InvalidToml {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;

        if let Some(palette_item) = doc.get("palette") {
            if let Some(palette_str) = palette_item.as_str() {
                return Ok(Some(palette_str.to_string()));
            }
        }

        Ok(None)
    }

    fn tool_name(&self) -> &'static str {
        "starship"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starship_tool_name() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.tool_name(), "starship");
    }

    #[test]
    fn test_starship_config_path() {
        let adapter = StarshipAdapter;
        let path = adapter.config_path().unwrap();
        assert!(path.to_string_lossy().contains("starship"));
        assert!(path.to_string_lossy().contains("starship.toml"));
    }

    #[test]
    fn test_starship_parse_toml() {
        let content = r#"
# This is a comment
format = "..."

[palette]
palette_name = "catppuccin-mocha"
"#;
        
        let doc: DocumentMut = content.parse().unwrap();
        assert!(doc.get("format").is_some());
        assert!(doc.get("palette").is_some());
    }

    #[test]
    fn test_starship_palette_modification() {
        let content = r#"
format = "..."
palette = "old-palette"
"#;
        
        let mut doc: DocumentMut = content.parse().unwrap();
        doc["palette"] = toml_edit::value("new-palette");
        
        let result = doc.to_string();
        assert!(result.contains("new-palette"));
        assert!(!result.contains("old-palette"));
    }

    #[test]
    fn test_starship_invalid_toml() {
        let content = r#"
format = "..."
[invalid toml without closing bracket
"#;
        
        let result: Result<DocumentMut, _> = content.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_starship_preserve_comments() {
        let content = r#"
# Top-level comment
format = "..."  # inline comment
palette = "old"  # palette comment
"#;
        
        let mut doc: DocumentMut = content.parse().unwrap();
        doc["palette"] = toml_edit::value("new");
        
        let result = doc.to_string();
        // Comments should be preserved
        assert!(result.contains("# Top-level comment"));
        assert!(result.contains("# inline comment"));
    }

    #[test]
    fn test_starship_multiline_values() {
        let content = r#"
format = """
$username\
$hostname\
"""
palette = "old"
"#;
        
        let mut doc: DocumentMut = content.parse().unwrap();
        doc["palette"] = toml_edit::value("new");
        
        let result = doc.to_string();
        assert!(result.contains("new"));
        // Multiline value should be preserved
        assert!(result.contains("username"));
    }
}
