//! Opacity preset infrastructure for terminal transparency.
//! Provides three discrete opacity presets (Solid/Frosted/Clear) with terminal-specific
//! configurations. Per ,.

use std::str::FromStr;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;

/// Discrete opacity presets for terminal windows.
/// Three levels with specific opacity values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpacityPreset {
    /// Fully opaque (opacity = 1.0, blur = 0)
    /// Best for light themes and contexts where transparency would reduce contrast.
    Solid,
    /// macOS-style frosted glass effect (opacity = 0.85, blur = 20)
    /// Provides subtle transparency with Gaussian blur for visual depth.
    Frosted,
    /// Highly transparent (opacity = 0.75, blur = 0)
    /// Maximizes background visibility; can impact readability with light themes.
    Clear,
}

impl OpacityPreset {
    /// Convert opacity preset to f32 value in [0.0, 1.0].
    /// Solid=1.0, Frosted=0.85, Clear=0.75.
    pub fn to_f32(self) -> f32 {
        match self {
            OpacityPreset::Solid => 1.0,
            OpacityPreset::Frosted => 0.85,
            OpacityPreset::Clear => 0.75,
        }
    }

    /// Get blur radius in pixels for terminal window.
    /// Solid→0, Frosted→20, Clear→0.
    /// Only Frosted applies blur; others use 0 (Ghostty ignores when 0).
    pub fn blur_radius(self) -> u32 {
        match self {
            OpacityPreset::Solid => 0,
            OpacityPreset::Frosted => 20,
            OpacityPreset::Clear => 0,
        }
    }
}

impl FromStr for OpacityPreset {
    type Err = SlateError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "solid" => Ok(OpacityPreset::Solid),
            "frosted" => Ok(OpacityPreset::Frosted),
            "clear" => Ok(OpacityPreset::Clear),
            _ => Err(SlateError::InvalidThemeData(
                format!("Unknown opacity preset: '{}'. Use: solid, frosted, or clear", s)
            )),
        }
    }
}

impl std::fmt::Display for OpacityPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpacityPreset::Solid => write!(f, "Solid"),
            OpacityPreset::Frosted => write!(f, "Frosted"),
            OpacityPreset::Clear => write!(f, "Clear"),
        }
    }
}

/// Recommended opacity preset based on theme lightness.
/// Light themes prefer Solid; dark themes prefer Frosted.
pub fn recommended_opacity_for_theme(theme: &ThemeVariant) -> OpacityPreset {
    // Heuristic: if theme name contains "light", "latte", or "day", recommend Solid.
    // Otherwise (dark themes), recommend Frosted.
    let name_lower = theme.name.to_lowercase();
    if name_lower.contains("light") || name_lower.contains("latte") || name_lower.contains("day") {
        OpacityPreset::Solid
    } else {
        OpacityPreset::Frosted
    }
}

/// Check if a translucent opacity would degrade light theme legibility.
/// Per D-26b: Light themes with Frosted/Clear should warn user.
pub fn should_warn_for_translucent_light_theme(theme: &ThemeVariant, preset: OpacityPreset) -> bool {
    let is_light_theme = {
        let name_lower = theme.name.to_lowercase();
        name_lower.contains("light") || name_lower.contains("latte") || name_lower.contains("day")
    };

    let is_translucent = preset == OpacityPreset::Frosted || preset == OpacityPreset::Clear;

    is_light_theme && is_translucent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opacity_preset_solid_values() {
        assert_eq!(OpacityPreset::Solid.to_f32(), 1.0);
        assert_eq!(OpacityPreset::Solid.blur_radius(), 0);
    }

    #[test]
    fn test_opacity_preset_frosted_values() {
        assert_eq!(OpacityPreset::Frosted.to_f32(), 0.85);
        assert_eq!(OpacityPreset::Frosted.blur_radius(), 20);
    }

    #[test]
    fn test_opacity_preset_clear_values() {
        assert_eq!(OpacityPreset::Clear.to_f32(), 0.75);
        assert_eq!(OpacityPreset::Clear.blur_radius(), 0);
    }

    #[test]
    fn test_parse_solid() {
        assert_eq!("solid".parse::<OpacityPreset>().unwrap(), OpacityPreset::Solid);
        assert_eq!("Solid".parse::<OpacityPreset>().unwrap(), OpacityPreset::Solid);
        assert_eq!("SOLID".parse::<OpacityPreset>().unwrap(), OpacityPreset::Solid);
    }

    #[test]
    fn test_parse_frosted() {
        assert_eq!("frosted".parse::<OpacityPreset>().unwrap(), OpacityPreset::Frosted);
        assert_eq!("Frosted".parse::<OpacityPreset>().unwrap(), OpacityPreset::Frosted);
        assert_eq!("FROSTED".parse::<OpacityPreset>().unwrap(), OpacityPreset::Frosted);
    }

    #[test]
    fn test_parse_clear() {
        assert_eq!("clear".parse::<OpacityPreset>().unwrap(), OpacityPreset::Clear);
        assert_eq!("Clear".parse::<OpacityPreset>().unwrap(), OpacityPreset::Clear);
        assert_eq!("CLEAR".parse::<OpacityPreset>().unwrap(), OpacityPreset::Clear);
    }

    #[test]
    fn test_parse_invalid() {
        let result = "translucent".parse::<OpacityPreset>();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown opacity preset"));
    }

    #[test]
    fn test_display() {
        assert_eq!(OpacityPreset::Solid.to_string(), "Solid");
        assert_eq!(OpacityPreset::Frosted.to_string(), "Frosted");
        assert_eq!(OpacityPreset::Clear.to_string(), "Clear");
    }

    #[test]
    fn test_round_trip() {
        for preset in &[OpacityPreset::Solid, OpacityPreset::Frosted, OpacityPreset::Clear] {
            let s = preset.to_string();
            let parsed: OpacityPreset = s.parse().unwrap();
            assert_eq!(&parsed, preset);
        }
    }

    #[test]
    fn test_recommended_opacity_for_light_theme() {
        use crate::theme::Palette;
        use std::collections::HashMap;

        let light_theme = ThemeVariant {
            id: "test-light".to_string(),
            name: "Clean Light".to_string(),
            family: "Test".to_string(),
            tool_refs: HashMap::new(),
            palette: Palette {
                foreground: "#000000".to_string(),
                background: "#ffffff".to_string(),
                cursor: None,
                selection_bg: None,
                selection_fg: None,
                black: "#000000".to_string(),
                red: "#ff0000".to_string(),
                green: "#00ff00".to_string(),
                yellow: "#ffff00".to_string(),
                blue: "#0000ff".to_string(),
                magenta: "#ff00ff".to_string(),
                cyan: "#00ffff".to_string(),
                white: "#ffffff".to_string(),
                bright_black: "#808080".to_string(),
                bright_red: "#ff8080".to_string(),
                bright_green: "#80ff80".to_string(),
                bright_yellow: "#ffff80".to_string(),
                bright_blue: "#8080ff".to_string(),
                bright_magenta: "#ff80ff".to_string(),
                bright_cyan: "#80ffff".to_string(),
                bright_white: "#ffffff".to_string(),
                bg_dim: None,
                bg_darker: None,
                bg_darkest: None,
                rosewater: None,
                flamingo: None,
                pink: None,
                mauve: None,
                lavender: None,
                text: None,
                subtext1: None,
                subtext0: None,
                overlay2: None,
                overlay1: None,
                overlay0: None,
                surface2: None,
                surface1: None,
                surface0: None,
                extras: HashMap::new(),
            },
            appearance: crate::theme::ThemeAppearance::Light,
            auto_pair: None,
        };

        assert_eq!(
            recommended_opacity_for_theme(&light_theme),
            OpacityPreset::Solid
        );
    }

    #[test]
    fn test_recommended_opacity_for_dark_theme() {
        use crate::theme::Palette;
        use std::collections::HashMap;

        let dark_theme = ThemeVariant {
            id: "test-dark".to_string(),
            name: "Modern Dark".to_string(),
            family: "Test".to_string(),
            tool_refs: HashMap::new(),
            palette: Palette {
                foreground: "#ffffff".to_string(),
                background: "#000000".to_string(),
                cursor: None,
                selection_bg: None,
                selection_fg: None,
                black: "#000000".to_string(),
                red: "#ff0000".to_string(),
                green: "#00ff00".to_string(),
                yellow: "#ffff00".to_string(),
                blue: "#0000ff".to_string(),
                magenta: "#ff00ff".to_string(),
                cyan: "#00ffff".to_string(),
                white: "#ffffff".to_string(),
                bright_black: "#808080".to_string(),
                bright_red: "#ff8080".to_string(),
                bright_green: "#80ff80".to_string(),
                bright_yellow: "#ffff80".to_string(),
                bright_blue: "#8080ff".to_string(),
                bright_magenta: "#ff80ff".to_string(),
                bright_cyan: "#80ffff".to_string(),
                bright_white: "#ffffff".to_string(),
                bg_dim: None,
                bg_darker: None,
                bg_darkest: None,
                rosewater: None,
                flamingo: None,
                pink: None,
                mauve: None,
                lavender: None,
                text: None,
                subtext1: None,
                subtext0: None,
                overlay2: None,
                overlay1: None,
                overlay0: None,
                surface2: None,
                surface1: None,
                surface0: None,
                extras: HashMap::new(),
            },
            appearance: crate::theme::ThemeAppearance::Dark,
            auto_pair: None,
        };

        assert_eq!(
            recommended_opacity_for_theme(&dark_theme),
            OpacityPreset::Frosted
        );
    }

    #[test]
    fn test_warn_for_translucent_light_theme() {
        use crate::theme::Palette;
        use std::collections::HashMap;

        let light_theme = ThemeVariant {
            id: "test-light".to_string(),
            name: "Catppuccin Latte".to_string(),
            family: "Test".to_string(),
            tool_refs: HashMap::new(),
            palette: Palette {
                foreground: "#000000".to_string(),
                background: "#ffffff".to_string(),
                cursor: None,
                selection_bg: None,
                selection_fg: None,
                black: "#000000".to_string(),
                red: "#ff0000".to_string(),
                green: "#00ff00".to_string(),
                yellow: "#ffff00".to_string(),
                blue: "#0000ff".to_string(),
                magenta: "#ff00ff".to_string(),
                cyan: "#00ffff".to_string(),
                white: "#ffffff".to_string(),
                bright_black: "#808080".to_string(),
                bright_red: "#ff8080".to_string(),
                bright_green: "#80ff80".to_string(),
                bright_yellow: "#ffff80".to_string(),
                bright_blue: "#8080ff".to_string(),
                bright_magenta: "#ff80ff".to_string(),
                bright_cyan: "#80ffff".to_string(),
                bright_white: "#ffffff".to_string(),
                bg_dim: None,
                bg_darker: None,
                bg_darkest: None,
                rosewater: None,
                flamingo: None,
                pink: None,
                mauve: None,
                lavender: None,
                text: None,
                subtext1: None,
                subtext0: None,
                overlay2: None,
                overlay1: None,
                overlay0: None,
                surface2: None,
                surface1: None,
                surface0: None,
                extras: HashMap::new(),
            },
            appearance: crate::theme::ThemeAppearance::Light,
            auto_pair: None,
        };

        // Light theme + Frosted should warn
        assert!(should_warn_for_translucent_light_theme(&light_theme, OpacityPreset::Frosted));
        // Light theme + Clear should warn
        assert!(should_warn_for_translucent_light_theme(&light_theme, OpacityPreset::Clear));
        // Light theme + Solid should not warn
        assert!(!should_warn_for_translucent_light_theme(&light_theme, OpacityPreset::Solid));
    }
}
