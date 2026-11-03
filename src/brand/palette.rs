//! Brand palette primitives.
//! Pure-render helpers used by the Role API (`src/brand/roles.rs`) and the
//! `RenderContext` builder.
//!
//! - `BRAND_LAVENDER_FIXED` is the single source of truth for brand
//!   anchors (slate logo, ✦, ◆, ★). Everyday role surfaces pull the active
//!   theme's `brand_accent` via [`theme_brand_accent`].
//! - Per-appearance blend ratios: dark themes blend 14 % accent, light themes
//!   blend 24 % accent so pills remain legible against bright backgrounds.

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::error::Result;
use crate::theme::{Palette, ThemeAppearance};

/// Slate's source lavender, adjusted for contrast by the role renderer for the
/// `slate` wordmark, the ✦ glyph, ◆ section headings, and the completion ★.
pub const BRAND_LAVENDER_FIXED: &str = "#7287fd";

/// Dark-appearance pill blend ratio — accent weighted at 14 % against bg.
const DARK_BLEND_RATIO: f32 = 0.14;
/// Light-appearance pill blend ratio — accent weighted at 24 % against bg.
const LIGHT_BLEND_RATIO: f32 = 0.24;

/// Resolve the active theme's `brand_accent` slot.
pub fn theme_brand_accent(palette: &Palette) -> &str {
    &palette.brand_accent
}

/// Compute `accent × ratio + bg × (1-ratio)` per appearance (14 % dark / 24 % light).
pub fn pill_background_rgb(
    accent_hex: &str,
    background_hex: &str,
    appearance: ThemeAppearance,
) -> Result<(u8, u8, u8)> {
    let (ar, ag, ab) = PaletteRenderer::hex_to_rgb(accent_hex)?;
    let (br, bg, bb) = PaletteRenderer::hex_to_rgb(background_hex)?;
    let ratio = match appearance {
        ThemeAppearance::Dark => DARK_BLEND_RATIO,
        ThemeAppearance::Light => LIGHT_BLEND_RATIO,
    };
    Ok((
        blend_channel(ar, br, ratio),
        blend_channel(ag, bg, ratio),
        blend_channel(ab, bb, ratio),
    ))
}

/// Linear per-channel blend: `accent * ratio + bg * (1-ratio)`, rounded and
/// clamped into the 0..=255 range.
fn blend_channel(accent: u8, background: u8, ratio: f32) -> u8 {
    let a = f32::from(accent);
    let b = f32::from(background);
    let mixed = a * ratio + b * (1.0 - ratio);
    mixed.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_lavender_fixed_is_7287fd() {
        assert_eq!(BRAND_LAVENDER_FIXED, "#7287fd");
    }

    /// Catppuccin Mocha sanity check: `#7287fd × 14% + #1e1e2e × 86%`
    /// lands on `(42, 45, 75)` = `#2a2d4b`.
    #[test]
    fn alpha_blend_matches_expected_catppuccin_mocha() {
        let blended =
            pill_background_rgb("#7287fd", "#1e1e2e", ThemeAppearance::Dark).expect("valid hex");
        assert_eq!(blended, (42, 45, 75));
    }

    #[test]
    fn dark_pill_blend_is_not_identical_to_background() {
        let (r, g, b) =
            pill_background_rgb("#7287fd", "#1e1e2e", ThemeAppearance::Dark).expect("valid hex");
        assert_ne!(
            (r, g, b),
            (0x1e, 0x1e, 0x2e),
            "pill bg should differ from raw theme bg"
        );
    }

    #[test]
    fn theme_brand_accent_returns_palette_field() {
        let mut palette = sample_palette();
        palette.brand_accent = "#abcdef".to_string();
        assert_eq!(theme_brand_accent(&palette), "#abcdef");
    }

    fn sample_palette() -> Palette {
        Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            brand_accent: "#7287fd".to_string(),
            black: "#000000".to_string(),
            red: "#ff0000".to_string(),
            green: "#00ff00".to_string(),
            yellow: "#ffff00".to_string(),
            blue: "#0000ff".to_string(),
            magenta: "#ff00ff".to_string(),
            cyan: "#00ffff".to_string(),
            white: "#ffffff".to_string(),
            bright_black: "#555555".to_string(),
            bright_red: "#ff5555".to_string(),
            bright_green: "#55ff55".to_string(),
            bright_yellow: "#ffff55".to_string(),
            bright_blue: "#5555ff".to_string(),
            bright_magenta: "#ff55ff".to_string(),
            bright_cyan: "#55ffff".to_string(),
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
            extras: std::collections::HashMap::new(),
        }
    }
}
