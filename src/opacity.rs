//! Opacity preset infrastructure for terminal transparency.
//! Provides three discrete opacity presets (Solid/Frosted/Clear) with terminal-specific
//! configurations.

use crate::error::{Result, SlateError};
use std::str::FromStr;

mod managed;
pub(crate) use managed::{managed_paths, ManagedFile, PreparedOpacity, MANAGED_FILES};

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
    /// Maximizes background visibility; can reduce text contrast against the desktop.
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
            _ => Err(SlateError::InvalidThemeData(format!(
                "Unknown opacity preset: '{}'. Use: solid, frosted, or clear",
                s
            ))),
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
        assert_eq!(
            "solid".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Solid
        );
        assert_eq!(
            "Solid".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Solid
        );
        assert_eq!(
            "SOLID".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Solid
        );
    }

    #[test]
    fn test_parse_frosted() {
        assert_eq!(
            "frosted".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Frosted
        );
        assert_eq!(
            "Frosted".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Frosted
        );
        assert_eq!(
            "FROSTED".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Frosted
        );
    }

    #[test]
    fn test_parse_clear() {
        assert_eq!(
            "clear".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Clear
        );
        assert_eq!(
            "Clear".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Clear
        );
        assert_eq!(
            "CLEAR".parse::<OpacityPreset>().unwrap(),
            OpacityPreset::Clear
        );
    }

    #[test]
    fn test_parse_invalid() {
        let result = "translucent".parse::<OpacityPreset>();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown opacity preset"));
    }

    #[test]
    fn test_display() {
        assert_eq!(OpacityPreset::Solid.to_string(), "Solid");
        assert_eq!(OpacityPreset::Frosted.to_string(), "Frosted");
        assert_eq!(OpacityPreset::Clear.to_string(), "Clear");
    }

    #[test]
    fn test_round_trip() {
        for preset in &[
            OpacityPreset::Solid,
            OpacityPreset::Frosted,
            OpacityPreset::Clear,
        ] {
            let s = preset.to_string();
            let parsed: OpacityPreset = s.parse().unwrap();
            assert_eq!(&parsed, preset);
        }
    }
}
