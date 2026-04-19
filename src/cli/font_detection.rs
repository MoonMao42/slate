use crate::env::SlateEnv;
use crate::error::Result;
use std::fs;
use std::path::PathBuf;

/// Detect current terminal font from Ghostty or Alacritty config
pub fn detect_current_font() -> Result<Option<String>> {
    let env = SlateEnv::from_process()?;
    detect_current_font_with_env(&env)
}

/// Detect current terminal font with injected SlateEnv (for testing)
pub fn detect_current_font_with_env(env: &SlateEnv) -> Result<Option<String>> {
    // Try Ghostty first
    if let Ok(Some(font)) = read_ghostty_font_with_env(env) {
        return Ok(Some(font));
    }

    // Fall back to Alacritty
    if let Ok(Some(font)) = read_alacritty_font_with_env(env) {
        return Ok(Some(font));
    }

    // No custom font found
    Ok(None)
}

/// Parse Ghostty config (key=value format) for font-family setting
fn read_ghostty_font_with_env(env: &SlateEnv) -> Result<Option<String>> {
    for config_path in ghostty_config_paths_with_env(env) {
        if !config_path.exists() {
            continue;
        }

        match fs::read(&config_path) {
            Ok(content) => {
                if let Some(font) = parse_ghostty_font_config_bytes(&content) {
                    return Ok(Some(font));
                }
            }
            Err(_) => continue,
        }
    }

    Ok(None)
}

/// Parse Alacritty TOML config for font setting
fn read_alacritty_font_with_env(env: &SlateEnv) -> Result<Option<String>> {
    let config_path = env.xdg_config_home().join("alacritty/alacritty.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    match fs::read(&config_path) {
        Ok(content) => {
            if let Ok(content) = String::from_utf8(content) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    // Look for [font] section, then [font.normal] section, then family field
                    if let Some(font_table) = doc.get("font").and_then(|v| v.as_table()) {
                        if let Some(normal_table) =
                            font_table.get("normal").and_then(|v| v.as_table())
                        {
                            if let Some(family_val) =
                                normal_table.get("family").and_then(|v| v.as_str())
                            {
                                return Ok(Some(family_val.to_string()));
                            }
                        }
                    }
                }
            }
            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

fn parse_ghostty_font_config_bytes(content: &[u8]) -> Option<String> {
    for line in content.split(|b| *b == b'\n') {
        let trimmed = line
            .iter()
            .copied()
            .skip_while(|b| b.is_ascii_whitespace())
            .collect::<Vec<u8>>();

        if trimmed.starts_with(b"#") || trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with(b"font-family") {
            let value_part = trimmed.splitn(2, |b| *b == b'=').nth(1)?;
            let font = value_part
                .iter()
                .copied()
                .skip_while(|b| b.is_ascii_whitespace())
                .collect::<Vec<u8>>();
            let font = trim_ascii_quotes_and_space(&font);
            if !font.is_empty() {
                if let Ok(font) = String::from_utf8(font.to_vec()) {
                    return Some(font);
                }
            }
        }
    }

    None
}

fn trim_ascii_quotes_and_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace() && *b != b'"' && *b != b'\'')
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace() && *b != b'"' && *b != b'\'')
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

#[cfg(test)]
fn parse_ghostty_font_config(content: &str) -> Option<String> {
    parse_ghostty_font_config_bytes(content.as_bytes())
}

fn ghostty_config_paths_with_env(env: &SlateEnv) -> Vec<PathBuf> {
    let config_base = env.xdg_config_home();
    vec![
        config_base.join("ghostty/config.ghostty"),
        config_base.join("ghostty/config"),
    ]
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
    fn test_parse_ghostty_font_config_with_single_quotes() {
        let content = "font-family = 'FiraCode Nerd Font'";
        let font = parse_ghostty_font_config(content);
        assert_eq!(font.as_deref(), Some("FiraCode Nerd Font"));
    }

    #[test]
    fn test_parse_ghostty_font_config_ignores_comments() {
        let content = r#"
            # font-family = "Bad Font"
            font-family = "Good Font"
        "#;
        let font = parse_ghostty_font_config(content);
        assert_eq!(font.as_deref(), Some("Good Font"));
    }

    #[test]
    fn test_parse_ghostty_font_config_handles_equals_in_value() {
        let content = r#"font-family = "SomeName=Something Nerd Font""#;
        let font = parse_ghostty_font_config(content);
        assert_eq!(font.as_deref(), Some("SomeName=Something Nerd Font"));
    }

    #[test]
    fn test_parse_ghostty_font_config_bytes_handles_non_utf8_prefix() {
        let content = b"\xFF\nfont-family = \"JetBrains Mono Nerd Font\"\n";
        let font = parse_ghostty_font_config_bytes(content);
        assert_eq!(font.as_deref(), Some("JetBrains Mono Nerd Font"));
    }

    #[test]
    fn test_parse_ghostty_font_config_ignores_incomplete_lines() {
        let content = r#"
            font-family
            font-family =
        "#;
        let font = parse_ghostty_font_config(content);
        assert!(font.is_none());
    }

    #[test]
    fn test_detect_current_font_with_env_respects_injected_home() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // With empty tempdir, should return None for both Ghostty and Alacritty
        let result = detect_current_font_with_env(&env);
        assert!(result.is_ok());
        // Result should be None since no configs exist in tempdir
    }
}
