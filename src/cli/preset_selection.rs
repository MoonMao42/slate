/// Style presets for quick-mode setup.
/// Each preset locks theme + font + terminal visual settings.
/// Per and from 02-, four locked presets are defined.


/// Terminal visual settings that bundle with presets
#[derive(Debug, Clone)]
pub struct TerminalVisuals {
    /// Background opacity (0.0 = transparent, 1.0 = opaque)
    pub background_opacity: f32,
    /// macOS blur radius (in pixels, 0 = no blur)
    pub blur_radius: u32,
    /// Window padding in pixels
    pub padding_x: u32,
    pub padding_y: u32,
    /// Cursor style: "block", "underline", "bar"
    pub cursor_style: &'static str,
}

/// Style preset: combines theme, font, and terminal visual settings
#[derive(Debug, Clone)]
pub struct StylePreset {
    /// Preset identifier (e.g., "modern-dark")
    pub id: &'static str,
    /// Display name (e.g., "Modern Dark")
    pub name: &'static str,
    /// One-line description
    pub description: &'static str,
    /// Theme variant ID (e.g., "catppuccin-mocha")
    pub theme_id: &'static str,
    /// Font option ID (e.g., "jetbrains-mono")
    pub font_id: &'static str,
    /// Terminal visual settings
    pub visuals: TerminalVisuals,
}

/// Central registry of all style presets (locked per)
pub struct PresetCatalog;

impl PresetCatalog {
    /// Get all available presets
    pub fn all_presets() -> Vec<StylePreset> {
        vec![
            StylePreset {
                id: "modern-dark",
                name: "Modern Dark",
                description: "Sleek dark palette with JetBrains Mono",
                theme_id: "catppuccin-mocha",
                font_id: "jetbrains-mono",
                visuals: TerminalVisuals {
                    background_opacity: 0.95,
                    blur_radius: 10,
                    padding_x: 12,
                    padding_y: 12,
                    cursor_style: "block",
                },
            },
            StylePreset {
                id: "minimal-frost",
                name: "Minimal Frost",
                description: "Clean Nordic aesthetic with Hack font",
                theme_id: "nord",
                font_id: "hack",
                visuals: TerminalVisuals {
                    background_opacity: 1.0,
                    blur_radius: 0,
                    padding_x: 16,
                    padding_y: 16,
                    cursor_style: "underline",
                },
            },
            StylePreset {
                id: "retro-warm",
                name: "Retro Warm",
                description: "Warm vintage palette with Iosevka Term",
                theme_id: "gruvbox-dark",
                font_id: "iosevka-term",
                visuals: TerminalVisuals {
                    background_opacity: 0.98,
                    blur_radius: 5,
                    padding_x: 14,
                    padding_y: 14,
                    cursor_style: "bar",
                },
            },
            StylePreset {
                id: "clean-light",
                name: "Clean Light",
                description: "Bright palette with Fira Code",
                theme_id: "catppuccin-latte",
                font_id: "fira-code",
                visuals: TerminalVisuals {
                    background_opacity: 1.0,
                    blur_radius: 0,
                    padding_x: 12,
                    padding_y: 12,
                    cursor_style: "block",
                },
            },
        ]
    }

    /// Get preset by ID
    pub fn get_preset(id: &str) -> Option<StylePreset> {
        Self::all_presets().into_iter().find(|p| p.id == id)
    }

    /// Get default preset ("modern-dark" per)
    pub fn default_preset() -> StylePreset {
        Self::get_preset("modern-dark").expect("Default preset must exist")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_presets_exist() {
        let presets = PresetCatalog::all_presets();
        assert_eq!(presets.len(), 4, "Must have exactly 4 locked presets per ");
    }

    #[test]
    fn test_preset_ids_unique() {
        let presets = PresetCatalog::all_presets();
        let mut ids = vec![];
        for p in &presets {
            assert!(!ids.contains(&p.id), "Preset ID must be unique: {}", p.id);
            ids.push(p.id);
        }
    }

    #[test]
    fn test_preset_theme_and_font_locked() {
        // Verify the locked mappings from 
        let modern = PresetCatalog::get_preset("modern-dark").unwrap();
        assert_eq!(modern.theme_id, "catppuccin-mocha");
        assert_eq!(modern.font_id, "jetbrains-mono");

        let minimal = PresetCatalog::get_preset("minimal-frost").unwrap();
        assert_eq!(minimal.theme_id, "nord");
        assert_eq!(minimal.font_id, "hack");

        let retro = PresetCatalog::get_preset("retro-warm").unwrap();
        assert_eq!(retro.theme_id, "gruvbox-dark");
        assert_eq!(retro.font_id, "iosevka-term");

        let clean = PresetCatalog::get_preset("clean-light").unwrap();
        assert_eq!(clean.theme_id, "catppuccin-latte");
        assert_eq!(clean.font_id, "fira-code");
    }

    #[test]
    fn test_default_preset_is_modern_dark() {
        let default = PresetCatalog::default_preset();
        assert_eq!(default.id, "modern-dark");
    }

    #[test]
    fn test_preset_visual_settings_reasonable() {
        for preset in PresetCatalog::all_presets() {
            assert!(
                preset.visuals.background_opacity > 0.0 && preset.visuals.background_opacity <= 1.0,
                "Invalid opacity for preset {}",
                preset.id
            );
            assert!(
                matches!(preset.visuals.cursor_style, "block" | "underline" | "bar"),
                "Invalid cursor style for preset {}",
                preset.id
            );
        }
    }
}
