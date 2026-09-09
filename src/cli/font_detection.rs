use super::startup_detection::read_hint;
use crate::config::file_read::MAX_TOOL_CONFIG_BYTES;
use crate::env::SlateEnv;
use crate::error::Result;
use std::path::PathBuf;

/// Detect current terminal font from Ghostty or Alacritty config
pub fn detect_current_font() -> Result<Option<String>> {
    let env = SlateEnv::from_process()?;
    detect_current_font_with_env(&env)
}

/// Best-effort direct-file font hint using the injected profile. Does not follow
/// imports, inspect running windows or validate the full terminal configuration.
pub fn detect_current_font_with_env(env: &SlateEnv) -> Result<Option<String>> {
    Ok(read_ghostty_font_with_env(env).or_else(|| read_alacritty_font_with_env(env)))
}

/// Parse Ghostty config (key=value format) for font-family setting
fn read_ghostty_font_with_env(env: &SlateEnv) -> Option<String> {
    for config_path in ghostty_config_paths_with_env(env) {
        if let Some(content) = read_hint(env, &config_path, MAX_TOOL_CONFIG_BYTES) {
            if let Some(font) = parse_ghostty_font_config_bytes(&content) {
                return Some(font);
            }
        }
    }
    None
}

/// Parse Alacritty TOML config for font setting
fn read_alacritty_font_with_env(env: &SlateEnv) -> Option<String> {
    let config_path = crate::adapter::AlacrittyAdapter::integration_config_path_with_env(env);
    let bytes = read_hint(env, &config_path, MAX_TOOL_CONFIG_BYTES)?;
    let content = std::str::from_utf8(&bytes).ok()?;
    let doc = content.parse::<toml_edit::DocumentMut>().ok()?;
    let family = doc.get("font")?.get("normal")?.get("family")?.as_str()?;
    crate::adapter::font_config::validate_family(family).ok()?;
    Some(family.to_owned())
}

fn parse_ghostty_font_config_bytes(content: &[u8]) -> Option<String> {
    for line in content.split(|b| *b == b'\n') {
        let trimmed = trim_ascii_space(line);

        if trimmed.starts_with(b"#") || trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.splitn(2, |b| *b == b'=');
        let key = trim_ascii_space(parts.next().unwrap_or_default());
        let value_part = parts.next();

        if key == b"font-family" {
            let Some(value_part) = value_part else {
                continue;
            };
            let value = trim_ascii_space(value_part);
            // Match Ghostty's LineIterator: remove one pair of double quotes,
            // not all quotes/inner whitespace. Single quotes and backslashes
            // are ordinary font-name bytes, not shell or TOML escapes.
            let font = if value.len() >= 2
                && value.first() == Some(&b'"')
                && value.last() == Some(&b'"')
            {
                &value[1..value.len() - 1]
            } else {
                value
            };
            if !font.is_empty() {
                if let Ok(font) = std::str::from_utf8(font) {
                    if crate::adapter::font_config::validate_family(font).is_ok() {
                        return Some(font.to_owned());
                    }
                }
            }
        }
    }

    None
}

fn trim_ascii_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

#[cfg(test)]
fn parse_ghostty_font_config(content: &str) -> Option<String> {
    parse_ghostty_font_config_bytes(content.as_bytes())
}

fn ghostty_config_paths_with_env(env: &SlateEnv) -> Vec<PathBuf> {
    crate::adapter::GhosttyAdapter::config_candidates_with_env(env)
        .unwrap_or_else(|_| crate::adapter::GhosttyAdapter::xdg_config_candidates(env))
        .into_iter()
        .rev()
        .map(|candidate| candidate.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_current_font_no_config() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        assert_eq!(detect_current_font_with_env(&env).unwrap(), None);
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
    fn test_parse_ghostty_font_config_ignores_style_specific_font_family_keys() {
        let content = r#"
            font-family-bold = "Bold Font"
            font-family-italic = "Italic Font"
            font-family = "Main Font"
        "#;
        let font = parse_ghostty_font_config(content);
        assert_eq!(font.as_deref(), Some("Main Font"));
    }

    #[test]
    fn test_parse_ghostty_font_config_with_single_quotes() {
        let content = "font-family = 'FiraCode Nerd Font'";
        let font = parse_ghostty_font_config(content);
        assert_eq!(font.as_deref(), Some("'FiraCode Nerd Font'"));
    }

    #[test]
    // SWATCH-RENDERER: hostile font-name styling bytes are rejection-test data.
    fn test_parse_ghostty_font_config_preserves_literal_quotes_and_spaces() {
        for family in [
            "\"Outer Quotes\"",
            "'Outer apostrophes'",
            r"Back\slash 字体",
            "  Literal spaces  ",
        ] {
            let config = crate::adapter::font_config::ghostty(family).unwrap();
            assert_eq!(parse_ghostty_font_config(&config).as_deref(), Some(family));
        }
        assert_eq!(parse_ghostty_font_config("font-family = Bad\x1b[31m"), None);
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
        assert_eq!(detect_current_font_with_env(&env).unwrap(), None);
    }

    #[test]
    fn test_detect_current_font_prefers_last_loaded_ghostty_config() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        std::fs::create_dir_all(&ghostty_dir).unwrap();
        std::fs::write(
            ghostty_dir.join("config.ghostty"),
            "font-family = \"Active Font\"\n",
        )
        .unwrap();
        std::fs::write(
            ghostty_dir.join("config"),
            "font-family = \"Legacy Font\"\n",
        )
        .unwrap();

        let font = detect_current_font_with_env(&env).unwrap();

        assert_eq!(font.as_deref(), Some("Active Font"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_detect_current_font_prefers_macos_app_support_over_xdg() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let app_support_dir = tempdir
            .path()
            .join("Library/Application Support/com.mitchellh.ghostty");
        std::fs::create_dir_all(&ghostty_dir).unwrap();
        std::fs::create_dir_all(&app_support_dir).unwrap();
        std::fs::write(
            ghostty_dir.join("config.ghostty"),
            "font-family = \"XDG Font\"\n",
        )
        .unwrap();
        std::fs::write(
            app_support_dir.join("config"),
            "font-family = \"App Support Font\"\n",
        )
        .unwrap();

        let font = detect_current_font_with_env(&env).unwrap();

        assert_eq!(font.as_deref(), Some("App Support Font"));

        std::fs::write(
            app_support_dir.join("config.ghostty"),
            "font-family = \"App Support Current Font\"\n",
        )
        .unwrap();
        assert_eq!(
            detect_current_font_with_env(&env).unwrap().as_deref(),
            Some("App Support Current Font")
        );
    }
}
