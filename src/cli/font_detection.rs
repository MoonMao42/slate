use crate::error::Result;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Detect current terminal font from Ghostty or Alacritty config
pub fn detect_current_font() -> Result<Option<String>> {
    // Try Ghostty first
    if let Ok(Some(font)) = read_ghostty_font() {
        return Ok(Some(font));
    }

    // Fall back to Alacritty
    if let Ok(Some(font)) = read_alacritty_font() {
        return Ok(Some(font));
    }

    // No custom font found
    Ok(None)
}

/// Parse Ghostty config (key=value format) for font-family setting
fn read_ghostty_font() -> Result<Option<String>> {
    for config_path in ghostty_config_paths() {
        if !config_path.exists() {
            continue;
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => {
                if let Some(font) = parse_ghostty_font_config(&content) {
                    return Ok(Some(font));
                }
            }
            Err(_) => continue,
        }
    }

    Ok(None)
}

/// Parse Alacritty TOML config for font setting
fn read_alacritty_font() -> Result<Option<String>> {
    let home = env::var("HOME").ok();
    if home.is_none() {
        return Ok(None);
    }

    let config_path = PathBuf::from(home.unwrap()).join(".config/alacritty/alacritty.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => {
            if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                // Look for [font] section, then [font.normal] section, then family field
                if let Some(font_table) = doc.get("font").and_then(|v| v.as_table()) {
                    if let Some(normal_table) = font_table.get("normal").and_then(|v| v.as_table())
                    {
                        if let Some(family_val) =
                            normal_table.get("family").and_then(|v| v.as_str())
                        {
                            return Ok(Some(family_val.to_string()));
                        }
                    }
                }
            }
            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

fn ghostty_config_paths() -> Vec<PathBuf> {
    ghostty_config_paths_from_env(
        env::var_os("HOME").as_deref(),
        env::var_os("XDG_CONFIG_HOME").as_deref(),
    )
}

fn ghostty_config_paths_from_env(
    home: Option<&std::ffi::OsStr>,
    xdg_config_home: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(xdg) = xdg_config_home {
        let xdg_path = Path::new(xdg);
        paths.push(xdg_path.join("ghostty/config.ghostty"));
        paths.push(xdg_path.join("ghostty/config"));
    }

    if let Some(home_dir) = home {
        let home_path = Path::new(home_dir).join(".config/ghostty");
        paths.push(home_path.join("config.ghostty"));
        paths.push(home_path.join("config"));
    }

    paths
}

fn parse_ghostty_font_config(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("font-family") {
            let Some((_, value_part)) = trimmed.split_once('=') else {
                continue;
            };
            let font = value_part
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if !font.is_empty() {
                return Some(font);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_current_font_no_config() {
        // When no configs exist, should return Ok(None)
        let result = detect_current_font();
        assert!(result.is_ok());
        // Result may be None or Some depending on test environment
    }

    #[test]
    fn test_parse_ghostty_font_config_reads_font_family() {
        let content = r#"
            # comment
            font-family = "JetBrains Mono Nerd Font"
        "#;

        let font = parse_ghostty_font_config(content);
        assert_eq!(font.as_deref(), Some("JetBrains Mono Nerd Font"));
    }

    #[test]
    fn test_ghostty_config_paths_include_current_and_legacy_locations() {
        let paths =
            ghostty_config_paths_from_env(Some("/tmp/home".as_ref()), Some("/tmp/xdg".as_ref()));

        assert_eq!(paths[0], PathBuf::from("/tmp/xdg/ghostty/config.ghostty"));
        assert_eq!(paths[1], PathBuf::from("/tmp/xdg/ghostty/config"));
        assert_eq!(
            paths[2],
            PathBuf::from("/tmp/home/.config/ghostty/config.ghostty")
        );
        assert_eq!(paths[3], PathBuf::from("/tmp/home/.config/ghostty/config"));
    }
}
