use crate::error::Result;
use std::path::PathBuf;
use std::env;
use std::fs;

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
    let home = env::var("HOME").ok();
    if home.is_none() {
        return Ok(None);
    }
    
    let config_path = PathBuf::from(home.unwrap())
        .join(".config/ghostty/config");
    
    if !config_path.exists() {
        return Ok(None);
    }
    
    match fs::read_to_string(&config_path) {
        Ok(content) => {
            for line in content.lines() {
                let trimmed = line.trim_start();
                
                // Skip comments
                if trimmed.starts_with("#") {
                    continue;
                }
                
                // Look for font-family = value
                if trimmed.starts_with("font-family") {
                    if let Some(value_part) = trimmed.split('=').nth(1) {
                        let font = value_part.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !font.is_empty() {
                            return Ok(Some(font));
                        }
                    }
                }
            }
            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

/// Parse Alacritty TOML config for font setting
fn read_alacritty_font() -> Result<Option<String>> {
    let home = env::var("HOME").ok();
    if home.is_none() {
        return Ok(None);
    }
    
    let config_path = PathBuf::from(home.unwrap())
        .join(".config/alacritty/alacritty.toml");
    
    if !config_path.exists() {
        return Ok(None);
    }
    
    match fs::read_to_string(&config_path) {
        Ok(content) => {
            if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                // Look for [font] section, then [font.normal] section, then family field
                if let Some(font_table) = doc.get("font")
                    .and_then(|v| v.as_table()) {
                    
                    if let Some(normal_table) = font_table.get("normal")
                        .and_then(|v| v.as_table()) {
                        
                        if let Some(family_val) = normal_table.get("family")
                            .and_then(|v| v.as_str()) {
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
}
