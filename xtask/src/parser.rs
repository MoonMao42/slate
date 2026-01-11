use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unified theme JSON schema for all 18 themes.
/// This schema bridges Catppuccin, Rosé Pine, Base16, Tokyo Night, Kanagawa, and Everforest.
/// Reference: https://docs.rs/serde/latest/ for Deserialize/Serialize macros
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeJson {
    /// Unique theme identifier (snake_case): e.g., "catppuccin_mocha"
    pub id: String,

    /// Display name: e.g., "Catppuccin Mocha"
    pub name: String,

    /// Theme family for grouping: "catppuccin", "rosepine", "base16", "tokyo-night", "kanagawa", "everforest"
    pub family: String,

    /// Optional description of the theme
    #[serde(default)]
    pub description: Option<String>,

    /// Theme appearance: "Dark" or "Light"
    pub appearance: String,

    /// Whether this theme has an auto-paired variant (for day/night switching)
    pub auto_pair: Option<String>,

    /// ANSI 256 color palette (16 colors: black-white + bright variants)
    pub colors: ColorsJson,

    /// Semantic color overrides (backgrounds for dimming)
    pub semantic: SemanticJson,

    /// Extra colors specific to theme family (e.g., Catppuccin rosewater, flamingo)
    #[serde(default)]
    pub extras: Option<HashMap<String, String>>,
}

/// ANSI 256 colors in order: 0-7 (standard), 8-15 (bright)
/// Reference: https://docs.rs/serde/latest/serde/attr.fn.default.html for field defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsJson {
    pub foreground: String,
    pub background: String,

    // Standard colors (0-7)
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,

    // Bright colors (8-15)
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

/// Semantic color backgrounds used for UI dimming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticJson {
    /// Slightly dimmed background
    pub bg_dim: String,

    /// More dimmed background
    pub bg_darker: String,

    /// Darkest background variant
    pub bg_darkest: String,
}

impl ThemeJson {
    /// Validate theme JSON structure and content.
    /// Checks:
    /// - id matches filename (without .json extension)
    /// - appearance is "Dark" or "Light"
    /// - all hex colors are valid #RRGGBB format
    /// - extras HashMap (if present) contains valid hex colors
    pub fn validate(&self, filename: &str) -> anyhow::Result<()> {
        // Check filename matches id
        let expected_filename = format!("{}.json", self.id);
        if filename != expected_filename {
            return Err(anyhow::anyhow!(
                "Theme id '{}' does not match filename '{}' (expected '{}')",
                self.id,
                filename,
                expected_filename
            ));
        }

        // Check appearance is valid enum
        if self.appearance != "Dark" && self.appearance != "Light" {
            return Err(anyhow::anyhow!(
                "Theme appearance must be 'Dark' or 'Light', got '{}'",
                self.appearance
            ));
        }

        // Validate all hex colors
        self.colors.validate()?;
        self.semantic.validate()?;

        // Validate extras if present
        if let Some(ref extras) = self.extras {
            for (key, color) in extras.iter() {
                validate_hex_color(color).map_err(|_| {
                    anyhow::anyhow!(
                        "Invalid hex color in extras['{}'] = '{}', expected #RRGGBB format",
                        key,
                        color
                    )
                })?;
            }
        }

        Ok(())
    }
}

impl ColorsJson {
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_hex_color(&self.foreground)
            .map_err(|_| anyhow::anyhow!("Invalid foreground color: {}", self.foreground))?;
        validate_hex_color(&self.background)
            .map_err(|_| anyhow::anyhow!("Invalid background color: {}", self.background))?;

        validate_hex_color(&self.black)
            .map_err(|_| anyhow::anyhow!("Invalid black color: {}", self.black))?;
        validate_hex_color(&self.red)
            .map_err(|_| anyhow::anyhow!("Invalid red color: {}", self.red))?;
        validate_hex_color(&self.green)
            .map_err(|_| anyhow::anyhow!("Invalid green color: {}", self.green))?;
        validate_hex_color(&self.yellow)
            .map_err(|_| anyhow::anyhow!("Invalid yellow color: {}", self.yellow))?;
        validate_hex_color(&self.blue)
            .map_err(|_| anyhow::anyhow!("Invalid blue color: {}", self.blue))?;
        validate_hex_color(&self.magenta)
            .map_err(|_| anyhow::anyhow!("Invalid magenta color: {}", self.magenta))?;
        validate_hex_color(&self.cyan)
            .map_err(|_| anyhow::anyhow!("Invalid cyan color: {}", self.cyan))?;
        validate_hex_color(&self.white)
            .map_err(|_| anyhow::anyhow!("Invalid white color: {}", self.white))?;

        validate_hex_color(&self.bright_black)
            .map_err(|_| anyhow::anyhow!("Invalid bright_black color: {}", self.bright_black))?;
        validate_hex_color(&self.bright_red)
            .map_err(|_| anyhow::anyhow!("Invalid bright_red color: {}", self.bright_red))?;
        validate_hex_color(&self.bright_green)
            .map_err(|_| anyhow::anyhow!("Invalid bright_green color: {}", self.bright_green))?;
        validate_hex_color(&self.bright_yellow)
            .map_err(|_| anyhow::anyhow!("Invalid bright_yellow color: {}", self.bright_yellow))?;
        validate_hex_color(&self.bright_blue)
            .map_err(|_| anyhow::anyhow!("Invalid bright_blue color: {}", self.bright_blue))?;
        validate_hex_color(&self.bright_magenta).map_err(|_| {
            anyhow::anyhow!("Invalid bright_magenta color: {}", self.bright_magenta)
        })?;
        validate_hex_color(&self.bright_cyan)
            .map_err(|_| anyhow::anyhow!("Invalid bright_cyan color: {}", self.bright_cyan))?;
        validate_hex_color(&self.bright_white)
            .map_err(|_| anyhow::anyhow!("Invalid bright_white color: {}", self.bright_white))?;

        Ok(())
    }
}

impl SemanticJson {
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_hex_color(&self.bg_dim)
            .map_err(|_| anyhow::anyhow!("Invalid bg_dim color: {}", self.bg_dim))?;
        validate_hex_color(&self.bg_darker)
            .map_err(|_| anyhow::anyhow!("Invalid bg_darker color: {}", self.bg_darker))?;
        validate_hex_color(&self.bg_darkest)
            .map_err(|_| anyhow::anyhow!("Invalid bg_darkest color: {}", self.bg_darkest))?;
        Ok(())
    }
}

/// Parse theme JSON string into ThemeJson struct.
/// Uses serde_json::from_str: https://docs.rs/serde_json/latest/serde_json/fn.from_str.html
pub fn parse_theme_json(json_str: &str, filename: &str) -> anyhow::Result<ThemeJson> {
    let theme: ThemeJson = serde_json::from_str(json_str)?;
    theme.validate(filename)?;
    Ok(theme)
}

/// Validate hex color format (#RRGGBB).
fn validate_hex_color(color: &str) -> Result<(), String> {
    if !color.starts_with('#') || color.len() != 7 {
        return Err(format!("Invalid hex color format: {}", color));
    }

    decode_hex(&color[1..]).map_err(|_| format!("Invalid hex color: {}", color))?;

    Ok(())
}

/// Minimal hex decoding without external dependency.
/// Reference: https://doc.rust-lang.org/std/primitive.u8.html#method.from_str_radix
fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd number of hex digits".to_string());
    }

    let mut bytes = Vec::new();
    for chunk in s.chars().collect::<Vec<_>>().chunks(2) {
        let hex_str: String = chunk.iter().collect();
        let byte = u8::from_str_radix(&hex_str, 16)
            .map_err(|_| format!("invalid hex digit in '{}'", hex_str))?;
        bytes.push(byte);
    }
    Ok(bytes)
}
