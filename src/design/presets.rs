//! Curated preset configurations for terminal beautification.
//! Each preset combines a theme variant with a recommended opacity level,
//! font family, and visual characteristics. Per.

use crate::error::{Result, SlateError};
use crate::opacity::OpacityPreset;
use crate::theme::{ThemeRegistry, ThemeVariant};

/// A complete visual preset combining theme, opacity, and font.
/// background_opacity field holds OpacityPreset instead of f32.
#[derive(Debug, Clone)]
pub struct PresetVisuals {
    pub name: &'static str,
    pub theme_id: &'static str,
    pub recommended_font: &'static str,
    pub background_opacity: OpacityPreset,
}

impl PresetVisuals {
    /// Load the full theme variant for this preset.
    pub fn load_theme(&self, registry: &ThemeRegistry) -> Result<ThemeVariant> {
        registry.get(self.theme_id).cloned().ok_or_else(|| {
            SlateError::InvalidThemeData(format!("Theme not found: {}", self.theme_id))
        })
    }
}

/// The four primary preset configurations.
/// Each preset has an OpacityPreset instead of f32 opacity.
pub const PRESET_MODERN_DARK: PresetVisuals = PresetVisuals {
    name: "Modern Dark",
    theme_id: "catppuccin-mocha",
    recommended_font: "JetBrainsMono Nerd Font",
    background_opacity: OpacityPreset::Frosted,
};

pub const PRESET_MINIMAL_FROST: PresetVisuals = PresetVisuals {
    name: "Minimal Frost",
    theme_id: "nord",
    recommended_font: "JetBrainsMono Nerd Font",
    background_opacity: OpacityPreset::Frosted,
};

pub const PRESET_RETRO_WARM: PresetVisuals = PresetVisuals {
    name: "Retro Warm",
    theme_id: "gruvbox-dark",
    recommended_font: "JetBrainsMono Nerd Font",
    background_opacity: OpacityPreset::Solid,
};

pub const PRESET_CLEAN_LIGHT: PresetVisuals = PresetVisuals {
    name: "Clean Light",
    theme_id: "catppuccin-latte",
    recommended_font: "JetBrainsMono Nerd Font",
    background_opacity: OpacityPreset::Solid,
};

/// Get all four preset configurations.
pub fn all_presets() -> &'static [PresetVisuals] {
    &[
        PRESET_MODERN_DARK,
        PRESET_MINIMAL_FROST,
        PRESET_RETRO_WARM,
        PRESET_CLEAN_LIGHT,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_presets_have_valid_names() {
        for preset in all_presets() {
            assert!(!preset.name.is_empty());
            assert!(!preset.theme_id.is_empty());
            assert!(!preset.recommended_font.is_empty());
        }
    }

    #[test]
    fn test_preset_opacity_values() {
        assert_eq!(
            PRESET_MODERN_DARK.background_opacity,
            OpacityPreset::Frosted
        );
        assert_eq!(
            PRESET_MINIMAL_FROST.background_opacity,
            OpacityPreset::Frosted
        );
        assert_eq!(PRESET_RETRO_WARM.background_opacity, OpacityPreset::Solid);
        assert_eq!(PRESET_CLEAN_LIGHT.background_opacity, OpacityPreset::Solid);
    }

    #[test]
    fn test_preset_light_theme_has_solid_opacity() {
        // Clean Light is a light theme and should use Solid opacity
        assert_eq!(PRESET_CLEAN_LIGHT.background_opacity, OpacityPreset::Solid);
    }

    #[test]
    fn test_all_presets_has_four_entries() {
        assert_eq!(all_presets().len(), 4);
    }
}
