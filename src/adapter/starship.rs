//! Starship adapter with scoped [palettes.slate] editing.
//! Explicit exception to managed-first — Starship has no documented
//! include/import mechanism, so uses EditInPlace strategy to modify user's
//! starship.toml in-place with careful scoping to [palettes.slate] section.

use crate::adapter::{ToolAdapter, ApplyStrategy};
use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::PathBuf;
use toml_edit::DocumentMut;
use which::which;

/// Starship adapter implementing v2 ToolAdapter trait.
pub struct StarshipAdapter;

impl StarshipAdapter {
    /// Pure path resolution: env override → XDG default (no global state)
    fn resolve_path(starship_config: Option<&str>, config_home: &PathBuf) -> PathBuf {
        if let Some(val) = starship_config {
            if !val.is_empty() {
                return PathBuf::from(val);
            }
        }
        config_home.join("starship.toml")
    }
}

impl ToolAdapter for StarshipAdapter {
    fn tool_name(&self) -> &'static str {
        "starship"
    }

    fn is_installed(&self) -> Result<bool> {
        let binary_exists = which("starship").is_ok();

        let config_exists = match self.integration_config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };

        Ok(binary_exists || config_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SlateError::MissingHomeDir)?;
        let config_home = PathBuf::from(home).join(".config");
        Ok(Self::resolve_path(
            std::env::var("STARSHIP_CONFIG").ok().as_deref(),
            &config_home,
        ))
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/starship")
        } else {
            PathBuf::from(".config/slate/managed/starship")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EditInPlace
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        let config_path = self.integration_config_path()?;

        // Step 0: Backup before any modification
        let config_manager = ConfigManager::new()?;
        let _backup_path = config_manager.backup_file(&config_path)?;

        // Step 1: Read and parse TOML (preserves comments via toml_edit)
        let content = fs::read_to_string(&config_path)
            .map_err(|e| SlateError::ConfigReadError(config_path.display().to_string(), e.to_string()))?;

        let mut doc: DocumentMut = content.parse()
            .map_err(|e| SlateError::TomlParseError(e))?;

        // Step 2: Set palette = "slate" at root level
        doc["palette"] = toml_edit::value("slate");

        // Step 3: Ensure [palettes.slate] table exists
        if doc.get("palettes").is_none() {
            doc["palettes"] = toml_edit::Item::Table(toml_edit::Table::new());
        }

        if let Some(palettes) = doc["palettes"].as_table_mut() {
            // Create or get [palettes.slate]
            let mut slate_palette = toml_edit::Table::new();

            // Use PaletteRenderer to get TOML-formatted colors
            let mut semantic_map = std::collections::HashMap::new();
            semantic_map.insert("rosewater", "rosewater");
            semantic_map.insert("flamingo", "flamingo");
            semantic_map.insert("pink", "pink");
            semantic_map.insert("mauve", "mauve");
            semantic_map.insert("red", "red");
            semantic_map.insert("maroon", "maroon");
            semantic_map.insert("peach", "peach");
            semantic_map.insert("yellow", "yellow");
            semantic_map.insert("green", "green");
            semantic_map.insert("teal", "teal");
            semantic_map.insert("sky", "sky");
            semantic_map.insert("sapphire", "sapphire");
            semantic_map.insert("blue", "blue");
            semantic_map.insert("lavender", "lavender");
            semantic_map.insert("text", "text");
            semantic_map.insert("subtext1", "subtext1");
            semantic_map.insert("subtext0", "subtext0");
            semantic_map.insert("overlay2", "overlay2");
            semantic_map.insert("overlay1", "overlay1");
            semantic_map.insert("overlay0", "overlay0");
            semantic_map.insert("surface2", "surface2");
            semantic_map.insert("surface1", "surface1");
            semantic_map.insert("surface0", "surface0");
            semantic_map.insert("foreground", "foreground");
            semantic_map.insert("background", "background");

            let palette_colors = crate::adapter::palette_renderer::PaletteRenderer::to_toml(&theme.palette, &semantic_map)?;

            // Parse the rendered TOML colors and add them to slate_palette
            for line in palette_colors.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    slate_palette[key] = toml_edit::value(value.trim_matches('"'));
                }
            }

            palettes["slate"] = toml_edit::Item::Table(slate_palette);
        }

        // Step 4: Write back to config file (atomic)
        let new_content = doc.to_string();
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;

        let mut file = AtomicWriteFile::open(&config_path)
            .map_err(|e| SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string()))?;

        file.write_all(new_content.as_bytes())
            .map_err(|e| SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string()))?;

        file.commit()
            .map_err(|e| SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string()))?;

        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.tool_name(), "starship");
    }

    #[test]
    fn test_apply_strategy_returns_edit_in_place() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EditInPlace);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = StarshipAdapter;
        let path = adapter.managed_config_path();
        assert!(path.to_string_lossy().contains(".config/slate/managed/starship"));
    }

    #[test]
    fn test_resolve_path_with_env_override() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(Some("/custom/starship.toml"), &config_home);
        assert_eq!(path, PathBuf::from("/custom/starship.toml"));
    }

    #[test]
    fn test_resolve_path_empty_env_uses_default() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(Some(""), &config_home);
        assert_eq!(path, PathBuf::from("/home/user/.config/starship.toml"));
    }

    #[test]
    fn test_resolve_path_default_xdg() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(None, &config_home);
        assert_eq!(path, PathBuf::from("/home/user/.config/starship.toml"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = StarshipAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
