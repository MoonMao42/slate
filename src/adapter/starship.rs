use crate::adapter::ToolAdapter;
use crate::config::backup::{create_backup, create_backup_with_session, BackupSession};
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use toml_edit::DocumentMut;

pub struct StarshipAdapter;

/// Returns Starship palette color definitions for a given palette name.
/// Colors use Catppuccin-style semantic names so any starship format string
/// referencing these names (red, peach, yellow, green, sapphire, lavender, crust, etc.)
/// works consistently across all themes.
fn starship_palette_colors(palette_name: &str) -> Option<Vec<(&'static str, &'static str)>> {
    let colors = match palette_name {
        "catppuccin_latte" => vec![
            ("rosewater", "#dc8a78"), ("flamingo", "#dd7878"), ("pink", "#ea76cb"),
            ("mauve", "#8839ef"), ("red", "#d20f39"), ("maroon", "#e64553"),
            ("peach", "#fe640b"), ("yellow", "#df8e1d"), ("green", "#40a02b"),
            ("teal", "#179299"), ("sky", "#04a5e5"), ("sapphire", "#209fb5"),
            ("blue", "#1e66f5"), ("lavender", "#7287fd"), ("text", "#4c4f69"),
            ("subtext1", "#5c5f77"), ("subtext0", "#6c6f85"), ("overlay2", "#7c7f93"),
            ("overlay1", "#8c8fa1"), ("overlay0", "#9ca0b0"), ("surface2", "#acb0be"),
            ("surface1", "#bcc0cc"), ("surface0", "#ccd0da"), ("base", "#eff1f5"),
            ("mantle", "#e6e9ef"), ("crust", "#dce0e8"),
        ],
        "catppuccin_frappe" => vec![
            ("rosewater", "#f2d5cf"), ("flamingo", "#eebebe"), ("pink", "#f4b8e4"),
            ("mauve", "#ca9ee6"), ("red", "#e78284"), ("maroon", "#ea999c"),
            ("peach", "#ef9f76"), ("yellow", "#e5c890"), ("green", "#a6d189"),
            ("teal", "#81c8be"), ("sky", "#99d1db"), ("sapphire", "#85c1dc"),
            ("blue", "#8caaee"), ("lavender", "#babbf1"), ("text", "#c6d0f5"),
            ("subtext1", "#b5bfe2"), ("subtext0", "#a5adce"), ("overlay2", "#949cbb"),
            ("overlay1", "#838ba7"), ("overlay0", "#737994"), ("surface2", "#626880"),
            ("surface1", "#51576d"), ("surface0", "#414559"), ("base", "#303446"),
            ("mantle", "#292c3c"), ("crust", "#232634"),
        ],
        "catppuccin_macchiato" => vec![
            ("rosewater", "#f4dbd6"), ("flamingo", "#f0c6c6"), ("pink", "#f5bde6"),
            ("mauve", "#c6a0f6"), ("red", "#ed8796"), ("maroon", "#ee99a0"),
            ("peach", "#f5a97f"), ("yellow", "#eed49f"), ("green", "#a6da95"),
            ("teal", "#8bd5ca"), ("sky", "#91d7e3"), ("sapphire", "#7dc4e4"),
            ("blue", "#8aadf4"), ("lavender", "#b7bdf8"), ("text", "#cad3f5"),
            ("subtext1", "#b8c0e0"), ("subtext0", "#a5adcb"), ("overlay2", "#939ab7"),
            ("overlay1", "#8087a2"), ("overlay0", "#6e738d"), ("surface2", "#5b6078"),
            ("surface1", "#494d64"), ("surface0", "#363a4f"), ("base", "#24273a"),
            ("mantle", "#1e2030"), ("crust", "#181926"),
        ],
        "catppuccin_mocha" => vec![
            ("rosewater", "#f5e0dc"), ("flamingo", "#f2cdcd"), ("pink", "#f5c2e7"),
            ("mauve", "#cba6f7"), ("red", "#f38ba8"), ("maroon", "#eba0ac"),
            ("peach", "#fab387"), ("yellow", "#f9e2af"), ("green", "#a6e3a1"),
            ("teal", "#94e2d5"), ("sky", "#89dceb"), ("sapphire", "#74c7ec"),
            ("blue", "#89b4fa"), ("lavender", "#b4befe"), ("text", "#cdd6f4"),
            ("subtext1", "#bac2de"), ("subtext0", "#a6adc8"), ("overlay2", "#9399b2"),
            ("overlay1", "#7f849c"), ("overlay0", "#6c7086"), ("surface2", "#585b70"),
            ("surface1", "#45475a"), ("surface0", "#313244"), ("base", "#1e1e2e"),
            ("mantle", "#181825"), ("crust", "#11111b"),
        ],
        "dracula" => vec![
            ("rosewater", "#f8f8f2"), ("flamingo", "#ff79c6"), ("pink", "#ff79c6"),
            ("mauve", "#bd93f9"), ("red", "#ff5555"), ("maroon", "#ff6e6e"),
            ("peach", "#ffb86c"), ("yellow", "#f1fa8c"), ("green", "#50fa7b"),
            ("teal", "#8be9fd"), ("sky", "#8be9fd"), ("sapphire", "#8be9fd"),
            ("blue", "#6272a4"), ("lavender", "#bd93f9"), ("text", "#f8f8f2"),
            ("subtext1", "#f8f8f2"), ("subtext0", "#bfbfbf"), ("overlay2", "#6272a4"),
            ("overlay1", "#565869"), ("overlay0", "#44475a"), ("surface2", "#44475a"),
            ("surface1", "#383a4a"), ("surface0", "#313245"), ("base", "#282a36"),
            ("mantle", "#21222c"), ("crust", "#191a21"),
        ],
        "nord" => vec![
            ("rosewater", "#d8dee9"), ("flamingo", "#bf616a"), ("pink", "#b48ead"),
            ("mauve", "#b48ead"), ("red", "#bf616a"), ("maroon", "#bf616a"),
            ("peach", "#d08770"), ("yellow", "#ebcb8b"), ("green", "#a3be8c"),
            ("teal", "#8fbcbb"), ("sky", "#88c0d0"), ("sapphire", "#88c0d0"),
            ("blue", "#81a1c1"), ("lavender", "#b48ead"), ("text", "#eceff4"),
            ("subtext1", "#e5e9f0"), ("subtext0", "#d8dee9"), ("overlay2", "#4c566a"),
            ("overlay1", "#434c5e"), ("overlay0", "#3b4252"), ("surface2", "#4c566a"),
            ("surface1", "#434c5e"), ("surface0", "#3b4252"), ("base", "#2e3440"),
            ("mantle", "#272c36"), ("crust", "#242933"),
        ],
        "tokyo-night" => vec![
            ("rosewater", "#c0caf5"), ("flamingo", "#f7768e"), ("pink", "#ff007c"),
            ("mauve", "#bb9af7"), ("red", "#f7768e"), ("maroon", "#ff007c"),
            ("peach", "#ff9e64"), ("yellow", "#e0af68"), ("green", "#9ece6a"),
            ("teal", "#73daca"), ("sky", "#7dcfff"), ("sapphire", "#7dcfff"),
            ("blue", "#7aa2f7"), ("lavender", "#bb9af7"), ("text", "#c0caf5"),
            ("subtext1", "#a9b1d6"), ("subtext0", "#9aa5ce"), ("overlay2", "#787c99"),
            ("overlay1", "#565f89"), ("overlay0", "#414868"), ("surface2", "#414868"),
            ("surface1", "#343a52"), ("surface0", "#292e42"), ("base", "#1a1b26"),
            ("mantle", "#16161e"), ("crust", "#13131a"),
        ],
        "tokyo-night-light" => vec![
            ("rosewater", "#3760bf"), ("flamingo", "#f52a65"), ("pink", "#9854f1"),
            ("mauve", "#9854f1"), ("red", "#f52a65"), ("maroon", "#c64343"),
            ("peach", "#d26900"), ("yellow", "#b58c30"), ("green", "#6b9440"),
            ("teal", "#118c74"), ("sky", "#0184bc"), ("sapphire", "#0b8aab"),
            ("blue", "#2e7de9"), ("lavender", "#9854f1"), ("text", "#3760bf"),
            ("subtext1", "#4c5374"), ("subtext0", "#6172b0"), ("overlay2", "#848aab"),
            ("overlay1", "#9ca0bc"), ("overlay0", "#b4b8cf"), ("surface2", "#c4c8d6"),
            ("surface1", "#d0d4e0"), ("surface0", "#dcdfe8"), ("base", "#e1e2e7"),
            ("mantle", "#e9eaf0"), ("crust", "#d0d5e3"),
        ],
        _ => return None,
    };
    Some(colors)
}

impl StarshipAdapter {
    /// Pure path resolution: env override → XDG default (no global state)
    fn resolve_path(starship_config: Option<&str>, config_home: &std::path::Path) -> PathBuf {
        if let Some(val) = starship_config {
            if !val.is_empty() {
                return PathBuf::from(val);
            }
        }
        config_home.join("starship.toml")
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

        // Tool is installed if binary OR config exists (zero-config: binary alone = installed)
        Ok(binary_exists || config_exists)
    }

    fn config_path(&self) -> ThemeResult<PathBuf> {
        let config_home = crate::adapter::xdg_config_home()?;
        Ok(Self::resolve_path(
            std::env::var("STARSHIP_CONFIG").ok().as_deref(),
            &config_home,
        ))
    }

    fn config_exists(&self) -> ThemeResult<bool> {
        let path = self.config_path()?;
        Ok(path.exists() && path.is_file())
    }

    fn apply_theme(&self, theme: &Theme, session: Option<&BackupSession>) -> ThemeResult<()> {
        // Get canonical path (resolve symlinks)
        let config_path = self.config_path()?;
        let canonical_path =
            fs::canonicalize(&config_path).map_err(|_e| ThemeError::SymlinkError {
                path: config_path.display().to_string(),
            })?;

        // Create backup before modification (SAFE-04)
        if let Some(sess) = session {
            // Manifest-backed backup with persisted metadata
            let _restore_entry =
                create_backup_with_session("starship", "Starship", sess, &canonical_path)?;
        } else {
            // Legacy backup without session
            let _backup_info = create_backup("starship", &theme.name, &canonical_path)?;
        }

        // Read config file as string
        let content = fs::read_to_string(&canonical_path).map_err(|e| ThemeError::Io(e))?;

        // Parse using toml-edit (SAFE-02: preserves comments and formatting)
        let mut doc: DocumentMut =
            content
                .parse()
                .map_err(|e: toml_edit::TomlError| ThemeError::InvalidToml {
                    path: canonical_path.display().to_string(),
                    reason: e.to_string(),
                })?;

        // Get the Starship palette name from tool_overrides
        let palette_name = theme
            .colors
            .tool_overrides
            .get("starship")
            .ok_or_else(|| {
                ThemeError::Other(format!("No Starship theme override for {}", theme.name))
            })?
            .to_string();

        // Modify the palette key in the document root using toml_edit::value
        doc["palette"] = toml_edit::value(palette_name.clone());

        // Write palette color definitions to [palettes.{palette_name}]
        if let Some(colors) = starship_palette_colors(&palette_name) {
            if doc.get("palettes").is_none() {
                doc["palettes"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            if let Some(palettes) = doc["palettes"].as_table_mut() {
                let mut palette_table = toml_edit::Table::new();
                for (key, value) in colors {
                    palette_table[key] = toml_edit::value(value);
                }
                palettes[&palette_name] = toml_edit::Item::Table(palette_table);
            }
        }

        // Get the modified content as string
        let new_content = doc.to_string();

        // Atomic write
        let mut file =
            AtomicWriteFile::open(&canonical_path).map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.write_all(new_content.as_bytes())
            .map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.commit().map_err(|e| ThemeError::WriteError {
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
        let content = fs::read_to_string(&path).map_err(|e| ThemeError::Io(e))?;

        let doc: DocumentMut =
            content
                .parse()
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
    fn test_starship_resolve_path_env_override() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            StarshipAdapter::resolve_path(Some("/custom/starship.toml"), &config_home),
            PathBuf::from("/custom/starship.toml")
        );
    }

    #[test]
    fn test_starship_resolve_path_empty_env_uses_default() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            StarshipAdapter::resolve_path(Some(""), &config_home),
            PathBuf::from("/home/user/.config/starship.toml")
        );
    }

    #[test]
    fn test_starship_resolve_path_default_xdg() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            StarshipAdapter::resolve_path(None, &config_home),
            PathBuf::from("/home/user/.config/starship.toml")
        );
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
