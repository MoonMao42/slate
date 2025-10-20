use std::collections::HashMap;

pub mod catppuccin;
pub mod dracula;
pub mod nord;
pub mod tokyo_night;

/// Color palette for a theme
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub foreground: String,
    pub background: String,
    pub cursor: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub tool_overrides: HashMap<String, String>,
}

/// Theme family classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeFamily {
    Catppuccin,
    TokyoNight,
    Dracula,
    Nord,
}

impl std::fmt::Display for ThemeFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeFamily::Catppuccin => write!(f, "catppuccin"),
            ThemeFamily::TokyoNight => write!(f, "tokyo-night"),
            ThemeFamily::Dracula => write!(f, "dracula"),
            ThemeFamily::Nord => write!(f, "nord"),
        }
    }
}

/// A complete theme with name, family, and colors
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub family: ThemeFamily,
    pub colors: ThemeColors,
}

/// Normalize a theme name to kebab-case, lowercase
pub fn normalize_theme_name(input: &str) -> String {
    input
        .to_lowercase()
        .replace(' ', "-")
        .replace('_', "-")
}

/// Parse theme input and return (family, optional variant)
pub fn parse_theme_input(input: &str) -> Option<(ThemeFamily, Option<String>)> {
    let normalized = normalize_theme_name(input);
    let parts: Vec<&str> = normalized.split('-').collect();

    match parts.as_slice() {
        ["catppuccin"] => Some((ThemeFamily::Catppuccin, None)),
        ["catppuccin", variant] => Some((ThemeFamily::Catppuccin, Some(variant.to_string()))),
        ["tokyo", "night"] => Some((ThemeFamily::TokyoNight, None)),
        ["tokyo", "night", variant] => Some((ThemeFamily::TokyoNight, Some(variant.to_string()))),
        ["dracula"] => Some((ThemeFamily::Dracula, None)),
        ["nord"] => Some((ThemeFamily::Nord, None)),
        _ => None,
    }
}

/// Get a theme by name (case-insensitive)
pub fn get_theme(name: &str) -> Option<Theme> {
    let normalized = normalize_theme_name(name);
    match normalized.as_str() {
        "catppuccin-latte" => Some(catppuccin::catppuccin_latte()),
        "catppuccin-frappe" => Some(catppuccin::catppuccin_frappe()),
        "catppuccin-macchiato" => Some(catppuccin::catppuccin_macchiato()),
        "catppuccin-mocha" => Some(catppuccin::catppuccin_mocha()),
        "tokyo-night-light" => Some(tokyo_night::tokyo_night_light()),
        "tokyo-night-dark" => Some(tokyo_night::tokyo_night_dark()),
        "dracula" => Some(dracula::dracula()),
        "nord" => Some(nord::nord()),
        _ => None,
    }
}

/// Get list of all available theme names
pub fn available_themes() -> Vec<String> {
    vec![
        "catppuccin-latte".to_string(),
        "catppuccin-frappe".to_string(),
        "catppuccin-macchiato".to_string(),
        "catppuccin-mocha".to_string(),
        "tokyo-night-light".to_string(),
        "tokyo-night-dark".to_string(),
        "dracula".to_string(),
        "nord".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_theme_name() {
        assert_eq!(normalize_theme_name("Catppuccin Mocha"), "catppuccin-mocha");
        assert_eq!(normalize_theme_name("Tokyo Night Light"), "tokyo-night-light");
        assert_eq!(normalize_theme_name("DRACULA"), "dracula");
    }

    #[test]
    fn test_parse_theme_input_full() {
        assert_eq!(
            parse_theme_input("catppuccin-mocha"),
            Some((ThemeFamily::Catppuccin, Some("mocha".to_string())))
        );
    }

    #[test]
    fn test_parse_theme_input_invalid() {
        assert_eq!(parse_theme_input("nonexistent"), None);
    }

    #[test]
    fn test_get_theme_case_insensitive() {
        assert!(get_theme("catppuccin-mocha").is_some());
        assert!(get_theme("CATPPUCCIN-MOCHA").is_some());
    }

    #[test]
    fn test_available_themes_count() {
        assert_eq!(available_themes().len(), 8);
    }
}
