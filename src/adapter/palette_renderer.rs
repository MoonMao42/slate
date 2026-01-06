//! PaletteRenderer — Converts Palette to multiple output formats
//! Per through Renders Palette to 5 output formats (TOML, YAML, shell, tmux, JSONC)
//! for use by palette-intensive adapters (Starship, lazygit, fastfetch, etc.).
//! Semantic mapping pattern: Adapters define a map of palette field names to output field names,
//! allowing flexible rendering across different tools with different naming conventions.

use crate::error::{Result, SlateError};
use crate::theme::Palette;
use std::collections::HashMap;

/// Renders Palette to multiple output formats using semantic field mapping.
/// Adapters provide a semantic_map specifying how palette fields map to output fields.
pub struct PaletteRenderer;

impl PaletteRenderer {
    /// Convert hex color #RRGGBB to (R, G, B) tuple.
    /// Returns SlateError if hex is invalid.
    pub fn hex_to_rgb(hex: &str) -> Result<(u8, u8, u8)> {
        // Normalize: remove leading # if present
        let hex = hex.strip_prefix('#').unwrap_or(hex);

        // Validate length
        if hex.len() != 6 {
            return Err(SlateError::InvalidThemeData(format!(
                "Invalid hex color '{}': expected #RRGGBB format",
                hex
            )));
        }

        // Parse hex values
        let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| {
            SlateError::InvalidThemeData(format!(
                "Invalid hex color '{}': invalid red component",
                hex
            ))
        })?;
        let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| {
            SlateError::InvalidThemeData(format!(
                "Invalid hex color '{}': invalid green component",
                hex
            ))
        })?;
        let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| {
            SlateError::InvalidThemeData(format!(
                "Invalid hex color '{}': invalid blue component",
                hex
            ))
        })?;

        Ok((r, g, b))
    }

    /// Convert RGB to ANSI 24-bit format: "38;2;R;G;B"
    pub fn rgb_to_ansi_24bit(r: u8, g: u8, b: u8) -> String {
        format!("38;2;{};{};{}", r, g, b)
    }

    /// Convert RGB to hex format for zsh: #RRGGBB
    /// Used by to_shell_vars_from_pairs() for ZSH_HIGHLIGHT_STYLES
    /// zsh requires hex format, not ANSI 24-bit escapes
    pub fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    }

    /// Render Palette to TOML format for Starship.
    pub fn to_toml(palette: &Palette, semantic_map: &HashMap<&str, &str>) -> Result<String> {
        if semantic_map.is_empty() {
            return Err(SlateError::InvalidThemeData(
                "semantic_map cannot be empty".to_string(),
            ));
        }

        let mut output = String::new();
        let colors = Self::extract_palette_colors(palette);

        for (palette_key, output_key) in semantic_map.iter() {
            if let Some(color) = colors.get(palette_key as &str) {
                output.push_str(&format!("{} = \"{}\"\n", output_key, color));
            }
        }

        Ok(output)
    }

    /// Render Palette to YAML format for eza, lazygit.
    pub fn to_yaml(palette: &Palette, semantic_map: &HashMap<&str, &str>) -> Result<String> {
        if semantic_map.is_empty() {
            return Err(SlateError::InvalidThemeData(
                "semantic_map cannot be empty".to_string(),
            ));
        }

        let colors = Self::extract_palette_colors(palette);
        let mut yaml_lines = Vec::new();
        let mut last_prefix = String::new();

        let mut sorted_entries: Vec<_> = semantic_map.iter().collect();
        sorted_entries.sort_by_key(|&(_, output)| *output);

        for (_palette_key, output_key) in sorted_entries {
            let parts: Vec<&str> = output_key.split('.').collect();
            if parts.is_empty() {
                continue;
            }

            let prefix = if parts.len() > 1 {
                parts[0]
            } else {
                *output_key
            };

            if prefix != last_prefix {
                yaml_lines.push(format!("{}:", prefix));
                last_prefix = prefix.to_string();
            }

            let field = parts[parts.len() - 1];

            for (palette_key, color) in colors.iter() {
                if output_key.ends_with(&format!(".{}", palette_key)) || output_key == palette_key {
                    yaml_lines.push(format!("  {}: '{}'", field, color));
                    break;
                }
            }
        }

        let output = yaml_lines.join("\n");
        if output.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("{}\n", output))
        }
    }

    /// Render Palette to shell environment variables format.
    pub fn to_shell_vars(palette: &Palette, semantic_map: &HashMap<&str, &str>) -> Result<String> {
        let semantic_pairs: Vec<_> = semantic_map
            .iter()
            .map(|(palette_key, shell_var)| (*palette_key, *shell_var))
            .collect();

        Self::to_shell_vars_from_pairs(palette, &semantic_pairs)
    }

    /// Render Palette to shell environment variables format while preserving duplicate mappings.
    pub fn to_shell_vars_from_pairs(
        palette: &Palette,
        semantic_pairs: &[(&str, &str)],
    ) -> Result<String> {
        if semantic_pairs.is_empty() {
            return Err(SlateError::InvalidThemeData(
                "semantic_map cannot be empty".to_string(),
            ));
        }

        let colors = Self::extract_palette_colors(palette);
        let mut segments = Vec::new();

        for (palette_key, shell_var) in semantic_pairs.iter() {
            if let Some(color) = colors.get(*palette_key) {
                let (r, g, b) = Self::hex_to_rgb(color)?;
                let hex_color = Self::rgb_to_hex(r, g, b);
                segments.push(format!("{}={}", shell_var, hex_color));
            }
        }

        if segments.is_empty() {
            return Ok(String::new());
        }

        // ZSH_HIGHLIGHT_STYLES is a zsh associative array — must be set per-key,
        // not exported as a flat string.
        let mut output = String::from("typeset -gA ZSH_HIGHLIGHT_STYLES\n");
        for segment in &segments {
            if let Some((token, color)) = segment.split_once('=') {
                output.push_str(&format!("ZSH_HIGHLIGHT_STYLES[{}]='fg={}'\n", token, color));
            }
        }
        Ok(output)
    }

    /// Render Palette to tmux format.
    pub fn to_tmux(palette: &Palette, semantic_map: &HashMap<&str, &str>) -> Result<String> {
        if semantic_map.is_empty() {
            return Err(SlateError::InvalidThemeData(
                "semantic_map cannot be empty".to_string(),
            ));
        }

        let colors = Self::extract_palette_colors(palette);
        let mut output = String::new();

        let mut styles: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for (palette_key, tmux_spec) in semantic_map.iter() {
            if let Some(color) = colors.get(palette_key as &str) {
                let parts: Vec<&str> = tmux_spec.split(':').collect();
                if parts.len() >= 2 {
                    let style = parts[0].to_string();
                    let key = parts[1].to_string();
                    styles
                        .entry(style)
                        .or_default()
                        .push((key, color.clone()));
                }
            }
        }

        let mut sorted_styles: Vec<_> = styles.into_iter().collect();
        sorted_styles.sort_by(|a, b| a.0.cmp(&b.0));

        for (style_name, mut pairs) in sorted_styles {
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let style_value = pairs
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(" ");
            output.push_str(&format!("set -g {} \"{}\"\n", style_name, style_value));
        }

        Ok(output)
    }

    /// Render Palette to JSONC format for fastfetch.
    pub fn to_jsonc(palette: &Palette, semantic_map: &HashMap<&str, &str>) -> Result<String> {
        if semantic_map.is_empty() {
            return Err(SlateError::InvalidThemeData(
                "semantic_map cannot be empty".to_string(),
            ));
        }

        let colors = Self::extract_palette_colors(palette);
        let mut output = String::from("{\n");

        let mut prefixes: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for (palette_key, jsonc_path) in semantic_map.iter() {
            if let Some(color) = colors.get(palette_key as &str) {
                let parts: Vec<&str> = jsonc_path.split('.').collect();
                if !parts.is_empty() {
                    let prefix = parts[0].to_string();
                    let field = if parts.len() > 1 {
                        parts[1].to_string()
                    } else {
                        parts[0].to_string()
                    };

                    let (r, g, b) = Self::hex_to_rgb(color)?;
                    let ansi_24bit = Self::rgb_to_ansi_24bit(r, g, b);

                    prefixes
                        .entry(prefix)
                        .or_default()
                        .push((field, ansi_24bit));
                }
            }
        }

        let mut sorted_prefixes: Vec<_> = prefixes.into_iter().collect();
        sorted_prefixes.sort_by(|a, b| a.0.cmp(&b.0));

        let mut first_prefix = true;
        for (prefix, mut fields) in sorted_prefixes {
            if !first_prefix {
                output.push(',');
            }
            output.push_str(&format!("  \"{}\": {{\n", prefix));

            fields.sort_by(|a, b| a.0.cmp(&b.0));
            for (i, (field, color)) in fields.iter().enumerate() {
                output.push_str(&format!(
                    "    \"{}\": \"{}\"{}  // ANSI 24-bit RGB\n",
                    field,
                    color,
                    if i + 1 < fields.len() { "," } else { "" }
                ));
            }

            output.push_str("  }\n");
            first_prefix = false;
        }

        output.push_str("}\n");
        Ok(output)
    }

    /// Extract all available palette colors into a HashMap.
    fn extract_palette_colors(palette: &Palette) -> HashMap<String, String> {
        let mut colors = HashMap::new();

        colors.insert("foreground".to_string(), palette.foreground.clone());
        colors.insert("background".to_string(), palette.background.clone());
        colors.insert("black".to_string(), palette.black.clone());
        colors.insert("red".to_string(), palette.red.clone());
        colors.insert("green".to_string(), palette.green.clone());
        colors.insert("yellow".to_string(), palette.yellow.clone());
        colors.insert("blue".to_string(), palette.blue.clone());
        colors.insert("magenta".to_string(), palette.magenta.clone());
        colors.insert("cyan".to_string(), palette.cyan.clone());
        colors.insert("white".to_string(), palette.white.clone());
        colors.insert("bright_black".to_string(), palette.bright_black.clone());
        colors.insert("bright_red".to_string(), palette.bright_red.clone());
        colors.insert("bright_green".to_string(), palette.bright_green.clone());
        colors.insert("bright_yellow".to_string(), palette.bright_yellow.clone());
        colors.insert("bright_blue".to_string(), palette.bright_blue.clone());
        colors.insert("bright_magenta".to_string(), palette.bright_magenta.clone());
        colors.insert("bright_cyan".to_string(), palette.bright_cyan.clone());
        colors.insert("bright_white".to_string(), palette.bright_white.clone());

        if let Some(v) = &palette.rosewater {
            colors.insert("rosewater".to_string(), v.clone());
        }
        if let Some(v) = &palette.flamingo {
            colors.insert("flamingo".to_string(), v.clone());
        }
        if let Some(v) = &palette.pink {
            colors.insert("pink".to_string(), v.clone());
        }
        if let Some(v) = &palette.mauve {
            colors.insert("mauve".to_string(), v.clone());
        }
        if let Some(v) = &palette.lavender {
            colors.insert("lavender".to_string(), v.clone());
        }
        if let Some(v) = &palette.text {
            colors.insert("text".to_string(), v.clone());
        }
        if let Some(v) = &palette.subtext1 {
            colors.insert("subtext1".to_string(), v.clone());
        }
        if let Some(v) = &palette.subtext0 {
            colors.insert("subtext0".to_string(), v.clone());
        }
        if let Some(v) = &palette.overlay2 {
            colors.insert("overlay2".to_string(), v.clone());
        }
        if let Some(v) = &palette.overlay1 {
            colors.insert("overlay1".to_string(), v.clone());
        }
        if let Some(v) = &palette.overlay0 {
            colors.insert("overlay0".to_string(), v.clone());
        }
        if let Some(v) = &palette.surface2 {
            colors.insert("surface2".to_string(), v.clone());
        }
        if let Some(v) = &palette.surface1 {
            colors.insert("surface1".to_string(), v.clone());
        }
        if let Some(v) = &palette.surface0 {
            colors.insert("surface0".to_string(), v.clone());
        }
        if let Some(v) = &palette.bg_dim {
            colors.insert("base".to_string(), v.clone());
        }
        if let Some(v) = &palette.bg_darker {
            colors.insert("mantle".to_string(), v.clone());
        }
        if let Some(v) = &palette.bg_darkest {
            colors.insert("crust".to_string(), v.clone());
        }

        colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_palette() -> Palette {
        Palette {
            foreground: "#cdd6f4".to_string(),
            background: "#1e1e2e".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            black: "#45475a".to_string(),
            red: "#f38ba8".to_string(),
            green: "#a6e3a1".to_string(),
            yellow: "#f9e2af".to_string(),
            blue: "#89b4fa".to_string(),
            magenta: "#f5c2e7".to_string(),
            cyan: "#94e2d5".to_string(),
            white: "#bac2de".to_string(),
            bright_black: "#585b70".to_string(),
            bright_red: "#f38ba8".to_string(),
            bright_green: "#a6e3a1".to_string(),
            bright_yellow: "#f9e2af".to_string(),
            bright_blue: "#89b4fa".to_string(),
            bright_magenta: "#f5c2e7".to_string(),
            bright_cyan: "#94e2d5".to_string(),
            bright_white: "#cdd6f4".to_string(),
            rosewater: Some("#f5e0dc".to_string()),
            flamingo: Some("#f2cdcd".to_string()),
            pink: Some("#f5c2e7".to_string()),
            mauve: Some("#cba6f7".to_string()),
            lavender: Some("#b4befe".to_string()),
            text: Some("#cdd6f4".to_string()),
            subtext1: Some("#bac2de".to_string()),
            subtext0: Some("#a6adc8".to_string()),
            overlay2: Some("#9399b2".to_string()),
            overlay1: Some("#7f849c".to_string()),
            overlay0: Some("#6c7086".to_string()),
            surface2: Some("#585b70".to_string()),
            surface1: Some("#45475a".to_string()),
            surface0: Some("#313244".to_string()),
            bg_dim: Some("#313244".to_string()),
            bg_darker: Some("#292c3c".to_string()),
            bg_darkest: Some("#11111b".to_string()),
            extras: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_hex_to_rgb_valid() {
        let (r, g, b) = PaletteRenderer::hex_to_rgb("#f5e0dc").unwrap();
        assert_eq!(r, 245);
        assert_eq!(g, 224);
        assert_eq!(b, 220);
    }

    #[test]
    fn test_hex_to_rgb_without_prefix() {
        let (r, g, b) = PaletteRenderer::hex_to_rgb("f5e0dc").unwrap();
        assert_eq!(r, 245);
        assert_eq!(g, 224);
        assert_eq!(b, 220);
    }

    #[test]
    fn test_hex_to_rgb_invalid_length() {
        let result = PaletteRenderer::hex_to_rgb("#fff");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_to_rgb_invalid_hex_chars() {
        let result = PaletteRenderer::hex_to_rgb("#gggggg");
        assert!(result.is_err());
    }

    #[test]
    fn test_rgb_to_ansi_24bit() {
        let result = PaletteRenderer::rgb_to_ansi_24bit(245, 224, 220);
        assert_eq!(result, "38;2;245;224;220");
    }

    #[test]
    fn test_to_toml_renders_colors() {
        let palette = create_test_palette();
        let mut semantic_map = HashMap::new();
        semantic_map.insert("red", "red");
        semantic_map.insert("green", "green");

        let result = PaletteRenderer::to_toml(&palette, &semantic_map).unwrap();
        assert!(result.contains("red = \"#f38ba8\""));
        assert!(result.contains("green = \"#a6e3a1\""));
    }

    #[test]
    fn test_to_toml_empty_map_error() {
        let palette = create_test_palette();
        let semantic_map = HashMap::new();
        let result = PaletteRenderer::to_toml(&palette, &semantic_map);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_yaml_renders_nested_structure() {
        let palette = create_test_palette();
        let mut semantic_map = HashMap::new();
        semantic_map.insert("red", "colors.red");
        semantic_map.insert("green", "colors.green");

        let result = PaletteRenderer::to_yaml(&palette, &semantic_map).unwrap();
        assert!(result.contains("colors:"));
    }

    #[test]
    fn test_to_shell_vars_converts_to_24bit() {
        let palette = create_test_palette();
        let mut semantic_map = HashMap::new();
        semantic_map.insert("red", "error");

        let result = PaletteRenderer::to_shell_vars(&palette, &semantic_map).unwrap();
        assert!(result.starts_with("typeset -gA ZSH_HIGHLIGHT_STYLES\n"));
        assert!(result.contains("ZSH_HIGHLIGHT_STYLES[error]='fg=#"));
    }

    #[test]
    fn test_to_shell_vars_from_pairs_preserves_duplicate_palette_keys() {
        let palette = create_test_palette();
        let semantic_pairs = vec![("red", "error"), ("red", "arg0")];

        let result = PaletteRenderer::to_shell_vars_from_pairs(&palette, &semantic_pairs).unwrap();
        assert!(result.starts_with("typeset -gA ZSH_HIGHLIGHT_STYLES\n"));
        assert!(result.contains("ZSH_HIGHLIGHT_STYLES[error]='fg=#"));
        assert!(result.contains("ZSH_HIGHLIGHT_STYLES[arg0]='fg=#"));
    }

    #[test]
    fn test_to_tmux_renders_set_commands() {
        let palette = create_test_palette();
        let mut semantic_map = HashMap::new();
        semantic_map.insert("background", "status-style:bg");
        semantic_map.insert("foreground", "status-style:fg");

        let result = PaletteRenderer::to_tmux(&palette, &semantic_map).unwrap();
        assert!(result.contains("set -g"));
        assert!(result.contains("status-style"));
    }

    #[test]
    fn test_to_jsonc_renders_json_structure() {
        let palette = create_test_palette();
        let mut semantic_map = HashMap::new();
        semantic_map.insert("red", "colors.red");

        let result = PaletteRenderer::to_jsonc(&palette, &semantic_map).unwrap();
        assert!(result.contains("\"colors\""));
        assert!(result.contains("38;2;"));
    }

    #[test]
    fn test_extract_palette_colors_includes_optional() {
        let palette = create_test_palette();
        let colors = PaletteRenderer::extract_palette_colors(&palette);

        assert!(colors.contains_key("red"));
        assert!(colors.contains_key("green"));
        assert!(colors.contains_key("rosewater"));
        assert!(colors.contains_key("text"));
    }

    #[test]
    fn test_to_yaml_with_catppuccin_colors() {
        let palette = create_test_palette();
        let mut semantic_map = HashMap::new();
        semantic_map.insert("rosewater", "palette.rosewater");
        semantic_map.insert("text", "palette.text");

        let result = PaletteRenderer::to_yaml(&palette, &semantic_map).unwrap();
        assert!(result.contains("palette:"));
    }
}
