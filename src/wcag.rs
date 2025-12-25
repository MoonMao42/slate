/// WCAG 2.1 contrast ratio audit for theme colors.
/// Reference: https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html
/// Implements relative_luminance and contrast_ratio calculations
/// for verification and warning generation during theme application.

use crate::theme::Palette;

/// Contrast audit result for a single color pair.
#[derive(Debug, Clone)]
pub struct ContrastAudit {
    pub color_name: String,
    pub foreground: String,
    pub background: String,
    pub ratio: f64,
    pub is_accessible: bool,
}

/// Audit all colors in a palette for WCAG accessibility.
/// Returns list of audit results with warnings for colors below 4.5:1 contrast.
/// Warning mode: all issues reported but non-blocking.
pub fn audit_palette(palette: &Palette) -> Vec<ContrastAudit> {
    let mut audits = Vec::new();

    // Check foreground vs background (primary contrast)
    let fg_rgb = hex_to_rgb(&palette.foreground);
    let bg_rgb = hex_to_rgb(&palette.background);

    if let (Ok(fg), Ok(bg)) = (fg_rgb, bg_rgb) {
        let ratio = contrast_ratio(
            relative_luminance(fg.0, fg.1, fg.2),
            relative_luminance(bg.0, bg.1, bg.2),
        );

        audits.push(ContrastAudit {
            color_name: "foreground".to_string(),
            foreground: palette.foreground.clone(),
            background: palette.background.clone(),
            ratio,
            is_accessible: ratio >= 4.5,
        });
    }

    // Check ANSI colors against background
    let color_pairs = vec![
        ("black", &palette.black),
        ("red", &palette.red),
        ("green", &palette.green),
        ("yellow", &palette.yellow),
        ("blue", &palette.blue),
        ("magenta", &palette.magenta),
        ("cyan", &palette.cyan),
        ("white", &palette.white),
    ];

    for (name, color) in color_pairs {
        if let (Ok(fg), Ok(bg)) = (hex_to_rgb(color), hex_to_rgb(&palette.background)) {
            let ratio = contrast_ratio(
                relative_luminance(fg.0, fg.1, fg.2),
                relative_luminance(bg.0, bg.1, bg.2),
            );

            audits.push(ContrastAudit {
                color_name: name.to_string(),
                foreground: color.clone(),
                background: palette.background.clone(),
                ratio,
                is_accessible: ratio >= 4.5,
            });
        }
    }

    audits
}

/// Log WCAG audit results (warnings only, non-blocking).
pub fn log_audit_warnings(theme_name: &str, audits: &[ContrastAudit]) {
    for audit in audits {
        if !audit.is_accessible {
            eprintln!(
                "⚠ WCAG Warning: {} color '{}' has contrast ratio {:.2} (minimum 4.5)",
                theme_name, audit.color_name, audit.ratio
            );
        }
    }
}

/// Calculate relative luminance per WCAG 2.1 formula.
fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let r = (r as f64) / 255.0;
    let g = (g as f64) / 255.0;
    let b = (b as f64) / 255.0;

    let r = if r <= 0.03928 {
        r / 12.92
    } else {
        ((r + 0.055) / 1.055).powf(2.4)
    };

    let g = if g <= 0.03928 {
        g / 12.92
    } else {
        ((g + 0.055) / 1.055).powf(2.4)
    };

    let b = if b <= 0.03928 {
        b / 12.92
    } else {
        ((b + 0.055) / 1.055).powf(2.4)
    };

    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Calculate contrast ratio between two luminances.
fn contrast_ratio(l1: f64, l2: f64) -> f64 {
    let lighter = l1.max(l2);
    let darker = l1.min(l2);
    (lighter + 0.05) / (darker + 0.05)
}

/// Convert hex color #RRGGBB to (R, G, B) tuple.
fn hex_to_rgb(hex: &str) -> Result<(u8, u8, u8), String> {
    if !hex.starts_with('#') || hex.len() != 7 {
        return Err(format!("Invalid hex color format: {}", hex));
    }

    let r = u8::from_str_radix(&hex[1..3], 16)
        .map_err(|_| format!("Invalid hex color: {}", hex))?;
    let g = u8::from_str_radix(&hex[3..5], 16)
        .map_err(|_| format!("Invalid hex color: {}", hex))?;
    let b = u8::from_str_radix(&hex[5..7], 16)
        .map_err(|_| format!("Invalid hex color: {}", hex))?;

    Ok((r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_luminance() {
        // Pure white
        let lum_white = relative_luminance(255, 255, 255);
        assert!((lum_white - 1.0).abs() < 0.01);

        // Pure black
        let lum_black = relative_luminance(0, 0, 0);
        assert!(lum_black < 0.01);
    }

    #[test]
    fn test_contrast_ratio() {
        // Black vs white should be 21:1
        let ratio = contrast_ratio(0.0, 1.0);
        assert!((ratio - 21.0).abs() < 0.5);
    }

    #[test]
    fn test_hex_to_rgb() {
        assert_eq!(hex_to_rgb("#FFFFFF"), Ok((255, 255, 255)));
        assert_eq!(hex_to_rgb("#000000"), Ok((0, 0, 0)));
        assert_eq!(hex_to_rgb("#FF0000"), Ok((255, 0, 0)));
    }
}
